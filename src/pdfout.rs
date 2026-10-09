//! A tiny PDF writer: just enough to emit images-as-pages and text overlays.
//! Anything that edits existing PDFs goes through qpdf instead.

use anyhow::{bail, Result};
use std::fmt::Write as _;

pub struct PdfBuilder {
    buf: Vec<u8>,
    offsets: Vec<Option<usize>>,
}

impl PdfBuilder {
    pub fn new() -> Self {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
        Self {
            buf,
            offsets: Vec::new(),
        }
    }

    /// Allocate the next object number (1-based).
    pub fn reserve(&mut self) -> u32 {
        self.offsets.push(None);
        self.offsets.len() as u32
    }

    pub fn put(&mut self, id: u32, body: &str) {
        self.begin(id);
        self.buf.extend_from_slice(body.as_bytes());
        self.buf.extend_from_slice(b"\nendobj\n");
    }

    /// `dict_entries` are the extra dictionary keys (without `<<`/`>>` or /Length).
    pub fn put_stream(&mut self, id: u32, dict_entries: &str, data: &[u8]) {
        self.begin(id);
        let head = format!("<< {dict_entries} /Length {} >>\nstream\n", data.len());
        self.buf.extend_from_slice(head.as_bytes());
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn begin(&mut self, id: u32) {
        self.offsets[(id - 1) as usize] = Some(self.buf.len());
        self.buf
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    }

    pub fn finish(mut self, root: u32) -> Result<Vec<u8>> {
        let xref_pos = self.buf.len();
        let n = self.offsets.len() + 1;
        let mut x = String::new();
        let _ = write!(x, "xref\n0 {n}\n0000000000 65535 f \n");
        for (i, off) in self.offsets.iter().enumerate() {
            match off {
                Some(o) => {
                    let _ = write!(x, "{o:010} 00000 n \n");
                }
                None => bail!("internal error: object {} was never written", i + 1),
            }
        }
        let _ = write!(
            x,
            "trailer\n<< /Size {n} /Root {root} 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n"
        );
        self.buf.extend_from_slice(x.as_bytes());
        Ok(self.buf)
    }
}

/// Escape a string for use inside a PDF literal string `( ... )`.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '(' | ')') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Helvetica advance widths (1/1000 em) for ASCII 32..=126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 32-47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
    278, 278, 584, 584, 584, 556, 1015, // : ; < = > ? @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // A-M
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // N-Z
    278, 278, 278, 469, 556, 333, // [ \ ] ^ _ `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // a-m
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // n-z
    334, 260, 334, 584, // { | } ~
];

/// Width in points of printable-ASCII `s` set in Helvetica at `size`.
pub fn text_width(s: &str, size: f64) -> f64 {
    s.bytes()
        .map(|b| {
            if (32..=126).contains(&b) {
                HELVETICA[(b - 32) as usize] as f64
            } else {
                556.0
            }
        })
        .sum::<f64>()
        * size
        / 1000.0
}

/// Overlay text is limited to printable ASCII (standard Helvetica, no embedding).
pub fn ensure_ascii(s: &str) -> Result<()> {
    if let Some(c) = s.chars().find(|c| !(' '..='~').contains(c)) {
        bail!("character `{c}` is not supported yet; use plain ASCII text");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_table_is_complete() {
        assert_eq!(HELVETICA.len(), 95);
        // Spot checks against the Helvetica AFM.
        assert_eq!(HELVETICA[(b' ' - 32) as usize], 278);
        assert_eq!(HELVETICA[(b'A' - 32) as usize], 667);
        assert_eq!(HELVETICA[(b'W' - 32) as usize], 944);
        assert_eq!(HELVETICA[(b'a' - 32) as usize], 556);
        assert_eq!(HELVETICA[(b'~' - 32) as usize], 584);
    }

    #[test]
    fn width_scales_with_size() {
        assert!((text_width("AA", 10.0) - 13.34).abs() < 1e-9);
    }

    #[test]
    fn escapes() {
        assert_eq!(escape(r"a(b)\c"), r"a\(b\)\\c");
    }

    #[test]
    fn ascii_check() {
        assert!(ensure_ascii("Page 1 of 2").is_ok());
        assert!(ensure_ascii("café").is_err());
    }
}
