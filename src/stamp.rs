//! Watermarks and page numbers.
//!
//! We generate a small overlay PDF (one page per target page, same size, text
//! in standard Helvetica with optional transparency) and let qpdf lay it over
//! the original. The original pages are never re-rendered.

use crate::engine;
use crate::pages::page_count;
use crate::pdfout::{ensure_ascii, escape, text_width, PdfBuilder};
use crate::util::ensure_distinct;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum Position {
    BottomCenter,
    BottomLeft,
    BottomRight,
    TopCenter,
    TopLeft,
    TopRight,
}

/// Parse `pdfinfo -f 1 -l N` output into displayed (width, height) per page.
/// pdfinfo reports the unrotated size plus a rotation, so swap for 90/270.
pub fn parse_pdfinfo(text: &str) -> BTreeMap<u32, (f64, f64)> {
    let mut size: BTreeMap<u32, (f64, f64)> = BTreeMap::new();
    let mut rot: BTreeMap<u32, u32> = BTreeMap::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 4 || t[0] != "Page" {
            continue;
        }
        let Ok(n) = t[1].parse::<u32>() else { continue };
        match t[2] {
            "size:" if t.len() >= 6 => {
                if let (Ok(w), Ok(h)) = (t[3].parse::<f64>(), t[5].parse::<f64>()) {
                    size.insert(n, (w, h));
                }
            }
            "rot:" => {
                if let Ok(r) = t[3].parse::<u32>() {
                    rot.insert(n, r % 360);
                }
            }
            _ => {}
        }
    }
    for (n, (w, h)) in size.iter_mut() {
        if matches!(rot.get(n), Some(90) | Some(270)) {
            std::mem::swap(w, h);
        }
    }
    size
}

fn page_sizes(input: &Path, total: u32) -> Result<Vec<(f64, f64)>> {
    let tool = engine::require("pdfinfo", "poppler")?;
    let args: Vec<OsString> = vec![
        "-f".into(),
        "1".into(),
        "-l".into(),
        total.to_string().into(),
        input.into(),
    ];
    let out = engine::run(&tool, &args)?;
    let map = parse_pdfinfo(&out);
    (1..=total)
        .map(|n| {
            map.get(&n)
                .copied()
                .with_context(|| format!("could not read size of page {n}"))
        })
        .collect()
}

/// Build the overlay: `content(index, w, h)` returns the content stream for
/// page `index` (0-based), which is `w` x `h` points.
fn build_overlay(
    sizes: &[(f64, f64)],
    opacity: f64,
    content: impl Fn(usize, f64, f64) -> String,
) -> Result<Vec<u8>> {
    let mut pdf = PdfBuilder::new();
    let catalog = pdf.reserve();
    let pages = pdf.reserve();
    let font = pdf.reserve();
    let gs = pdf.reserve();
    pdf.put(
        font,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    pdf.put(
        gs,
        &format!("<< /Type /ExtGState /ca {opacity:.3} /CA {opacity:.3} >>"),
    );

    let mut kids = String::new();
    for (i, &(w, h)) in sizes.iter().enumerate() {
        let c = pdf.reserve();
        let p = pdf.reserve();
        pdf.put_stream(c, "", content(i, w, h).as_bytes());
        pdf.put(
            p,
            &format!(
                "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {w:.2} {h:.2}] \
                 /Resources << /Font << /F1 {font} 0 R >> /ExtGState << /GS1 {gs} 0 R >> >> \
                 /Contents {c} 0 R >>"
            ),
        );
        let _ = write!(kids, "{p} 0 R ");
    }
    pdf.put(
        pages,
        &format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", sizes.len()),
    );
    pdf.put(catalog, &format!("<< /Type /Catalog /Pages {pages} 0 R >>"));
    pdf.finish(catalog)
}

fn apply_overlay(input: &Path, overlay: &[u8], output: &Path) -> Result<()> {
    let tmp: PathBuf =
        std::env::temp_dir().join(format!("pdfsqueeze-overlay-{}.pdf", std::process::id()));
    std::fs::write(&tmp, overlay)?;
    let args: Vec<OsString> = vec![
        input.into(),
        "--overlay".into(),
        (&tmp).into(),
        "--".into(),
        output.into(),
    ];
    let res = engine::run(&engine::qpdf()?, &args);
    let _ = std::fs::remove_file(&tmp);
    res.map(|_| ())
}

pub fn watermark(
    input: &Path,
    text: &str,
    opacity: f64,
    size: Option<f64>,
    angle: f64,
    output: &Path,
) -> Result<u32> {
    ensure_distinct(input, output)?;
    ensure_ascii(text)?;
    if text.trim().is_empty() {
        bail!("watermark text is empty");
    }
    if !(0.0..=1.0).contains(&opacity) || opacity == 0.0 {
        bail!("--opacity must be between 0 and 1");
    }
    let total = page_count(input)?;
    let sizes = page_sizes(input, total)?;
    let esc = escape(text);
    let rad = angle.to_radians();
    let (sin, cos) = rad.sin_cos();

    let overlay = build_overlay(&sizes, opacity, |_, w, h| {
        // Without --size, fit the text to ~60% of the page diagonal.
        let fs = size.unwrap_or_else(|| {
            let diag = (w * w + h * h).sqrt();
            (0.6 * diag / text_width(text, 1.0).max(0.001)).clamp(12.0, 160.0)
        });
        let tw = text_width(text, fs);
        format!(
            "q /GS1 gs 0.5 g {cos:.5} {sin:.5} {:.5} {cos:.5} {:.2} {:.2} cm \
             BT /F1 {fs:.2} Tf {:.2} {:.2} Td ({esc}) Tj ET Q",
            -sin,
            w / 2.0,
            h / 2.0,
            -tw / 2.0,
            -fs * 0.35,
        )
    })?;
    apply_overlay(input, &overlay, output)?;
    Ok(total)
}

pub fn number_pages(
    input: &Path,
    position: Position,
    format: &str,
    font_size: f64,
    start: i64,
    output: &Path,
) -> Result<u32> {
    ensure_distinct(input, output)?;
    ensure_ascii(format)?;
    if font_size <= 0.0 {
        bail!("--size must be positive");
    }
    let total = page_count(input)?;
    let sizes = page_sizes(input, total)?;
    let margin = 28.0;
    let last = start + total as i64 - 1;

    let overlay = build_overlay(&sizes, 1.0, |i, w, h| {
        let n = start + i as i64;
        let label = format
            .replace("{n}", &n.to_string())
            .replace("{total}", &last.to_string());
        let tw = text_width(&label, font_size);
        let x = match position {
            Position::BottomLeft | Position::TopLeft => margin,
            Position::BottomRight | Position::TopRight => w - margin - tw,
            Position::BottomCenter | Position::TopCenter => (w - tw) / 2.0,
        };
        let y = match position {
            Position::TopLeft | Position::TopCenter | Position::TopRight => h - margin,
            _ => margin,
        };
        format!(
            "q /GS1 gs 0 g BT /F1 {font_size:.2} Tf {x:.2} {y:.2} Td ({}) Tj ET Q",
            escape(&label)
        )
    })?;
    apply_overlay(input, &overlay, output)?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pdfinfo_and_swaps_rotated() {
        let text = "\
Title:           x
Page    1 size: 612 x 792 pts (letter)
Page    1 rot:  0
Page    2 size: 612 x 792 pts (letter)
Page    2 rot:  90
Page    3 size: 595.276 x 841.89 pts (A4)
Page    3 rot:  270
";
        let m = parse_pdfinfo(text);
        assert_eq!(m[&1], (612.0, 792.0));
        assert_eq!(m[&2], (792.0, 612.0));
        assert!((m[&3].0 - 841.89).abs() < 1e-6);
    }
}
