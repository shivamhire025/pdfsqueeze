//! Images -> PDF, written in pure Rust (no external engine).
//! JPEGs are embedded as-is (no quality loss); everything else is decoded
//! and stored losslessly with Flate.

use crate::pdfout::PdfBuilder;
use crate::util::ensure_distinct;
use anyhow::{bail, Context, Result};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum PageSize {
    /// Page is exactly the image size at the given DPI.
    Fit,
    A4,
    Letter,
}

struct Img {
    w: u32,
    h: u32,
    colorspace: &'static str,
    filter: &'static str,
    data: Vec<u8>,
}

/// PDF user space is 72 units per inch; the spec tops out at 14400.
const MAX_PAGE_PT: f64 = 14400.0;
const MARGIN_PT: f64 = 18.0;

/// Read width, height and component count from a JPEG's SOF marker.
pub fn jpeg_info(b: &[u8]) -> Option<(u32, u32, u8)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1; // fill byte
            continue;
        }
        i += 2;
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue; // no payload
        }
        if i + 2 > b.len() {
            return None;
        }
        let len = u16::from_be_bytes([b[i], b[i + 1]]) as usize;
        let is_sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            if i + 8 > b.len() {
                return None;
            }
            let precision = b[i + 2];
            let h = u16::from_be_bytes([b[i + 3], b[i + 4]]) as u32;
            let w = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let n = b[i + 7];
            if precision != 8 || w == 0 || h == 0 {
                return None;
            }
            return Some((w, h, n));
        }
        i += len;
    }
    None
}

fn load(path: &Path) -> Result<Img> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;

    if let Some((w, h, n)) = jpeg_info(&bytes) {
        let cs = match n {
            1 => Some("/DeviceGray"),
            3 => Some("/DeviceRGB"),
            _ => None, // CMYK etc.: fall through and decode
        };
        if let Some(colorspace) = cs {
            return Ok(Img {
                w,
                h,
                colorspace,
                filter: "/DCTDecode",
                data: bytes,
            });
        }
    }

    let rgba = image::load_from_memory(&bytes)
        .with_context(|| format!("{} is not a supported image", path.display()))?
        .to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for p in rgba.pixels() {
        let a = p[3] as u32;
        for c in 0..3 {
            // composite onto white so transparent areas do not turn black
            rgb.push(((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8);
        }
    }
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(&rgb)?;
    Ok(Img {
        w,
        h,
        colorspace: "/DeviceRGB",
        filter: "/FlateDecode",
        data: z.finish()?,
    })
}

/// A picture split for embedding with transparency: Flate-compressed RGB, and a
/// matching 8-bit alpha channel to use as a soft mask. Large images are scaled
/// down to `max_width` pixels so a signature does not bloat the document.
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub rgb: Vec<u8>,
    pub alpha: Vec<u8>,
}

pub fn load_rgba(path: &Path, max_width: u32) -> Result<Rgba> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut img = image::load_from_memory(&bytes)
        .with_context(|| format!("{} is not a supported image", path.display()))?
        .to_rgba8();
    if img.width() > max_width {
        let h = ((img.height() as f64) * (max_width as f64) / (img.width() as f64)).round().max(1.0) as u32;
        img = image::imageops::resize(&img, max_width, h, image::imageops::FilterType::Lanczos3);
    }
    let (w, h) = img.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for p in img.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alpha.push(p[3]);
    }
    let z = |data: &[u8]| -> Result<Vec<u8>> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(data)?;
        Ok(e.finish()?)
    };
    Ok(Rgba { w, h, rgb: z(&rgb)?, alpha: z(&alpha)? })
}

pub fn images_to_pdf(
    inputs: &[PathBuf],
    output: &Path,
    size: PageSize,
    dpi: f64,
) -> Result<usize> {
    if inputs.is_empty() {
        bail!("give at least one image");
    }
    if dpi <= 0.0 {
        bail!("--dpi must be positive");
    }
    for i in inputs {
        ensure_distinct(i, output)?;
    }

    let mut pdf = PdfBuilder::new();
    let catalog = pdf.reserve();
    let pages_id = pdf.reserve();
    let mut kids = String::new();

    for path in inputs {
        let img = load(path)?;
        let (w, h) = (img.w as f64, img.h as f64);

        // page size, and where/how big the image sits on it (all in points)
        let (pw, ph, x, y, dw, dh) = match size {
            PageSize::Fit => {
                let mut scale = 72.0 / dpi;
                let longest = (w * scale).max(h * scale);
                if longest > MAX_PAGE_PT {
                    scale *= MAX_PAGE_PT / longest;
                }
                (w * scale, h * scale, 0.0, 0.0, w * scale, h * scale)
            }
            PageSize::A4 | PageSize::Letter => {
                let (mut pw, mut ph) = match size {
                    PageSize::A4 => (595.28, 841.89),
                    _ => (612.0, 792.0),
                };
                if w > h {
                    std::mem::swap(&mut pw, &mut ph); // landscape image -> landscape page
                }
                let scale = ((pw - 2.0 * MARGIN_PT) / w).min((ph - 2.0 * MARGIN_PT) / h);
                let (dw, dh) = (w * scale, h * scale);
                (pw, ph, (pw - dw) / 2.0, (ph - dh) / 2.0, dw, dh)
            }
        };

        let img_id = pdf.reserve();
        let content_id = pdf.reserve();
        let page_id = pdf.reserve();

        pdf.put_stream(
            img_id,
            &format!(
                "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter {}",
                img.w, img.h, img.colorspace, img.filter
            ),
            &img.data,
        );
        let content = format!("q {dw:.2} 0 0 {dh:.2} {x:.2} {y:.2} cm /Im0 Do Q");
        pdf.put_stream(content_id, "", content.as_bytes());
        pdf.put(
            page_id,
            &format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
                 /Resources << /XObject << /Im0 {img_id} 0 R >> >> /Contents {content_id} 0 R >>"
            ),
        );
        let _ = write!(kids, "{page_id} 0 R ");
    }

    pdf.put(
        pages_id,
        &format!(
            "<< /Type /Pages /Kids [{kids}] /Count {} >>",
            inputs.len()
        ),
    );
    pdf.put(catalog, &format!("<< /Type /Catalog /Pages {pages_id} 0 R >>"));

    std::fs::write(output, pdf.finish(catalog)?)
        .with_context(|| format!("cannot write {}", output.display()))?;
    Ok(inputs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_jpeg_sof() {
        let b = [
            0xFF, 0xD8, // SOI
            0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x20, 0x03, // SOF0: 32x16, 3 comps
            0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
        ];
        assert_eq!(jpeg_info(&b), Some((32, 16, 3)));
    }

    #[test]
    fn skips_other_segments_and_rejects_non_jpeg() {
        let b = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46, // APP0 (len 4)
            0xFF, 0xC2, 0x00, 0x0B, 0x08, 0x00, 0x08, 0x00, 0x08, 0x01, 0x01, 0x11, 0x00,
        ];
        assert_eq!(jpeg_info(&b), Some((8, 8, 1)));
        assert_eq!(jpeg_info(b"\x89PNG\r\n\x1a\n"), None);
    }
}
