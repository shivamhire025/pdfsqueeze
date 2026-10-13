//! Watermarks and page numbers.
//!
//! We generate a small overlay PDF (one page per target page, same size, text
//! in standard Helvetica with optional transparency) and let qpdf lay it over
//! the original. The original pages are never re-rendered.

use crate::engine;
use crate::imgpdf;
use crate::pages::{page_count, parse_page_spec};
use crate::pdfout::{ensure_ascii, escape, text_width, PdfBuilder};
use crate::util::ensure_distinct;
use anyhow::{bail, Result};
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

fn page_sizes(input: &Path, total: u32) -> Result<Vec<(f64, f64)>> {
    let sizes = crate::pages::page_sizes(input)?;
    if sizes.len() != total as usize {
        bail!(
            "could not read the size of every page (found {} of {total})",
            sizes.len()
        );
    }
    Ok(sizes)
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

/// Where a signature goes on a page of `pw` x `ph` points. `x` and `y` are the
/// left and top edges and `width` the signature width, all as percentages of the
/// page; `aspect` is image height over width. Returns (left, bottom, width,
/// height) in PDF points, kept fully on the page.
pub fn signature_box(pw: f64, ph: f64, x: f64, y: f64, width: f64, aspect: f64) -> (f64, f64, f64, f64) {
    let mut sw = (width / 100.0 * pw).clamp(8.0, pw);
    let mut sh = sw * aspect;
    if sh > ph {
        sh = ph;
        sw = sh / aspect;
    }
    let left = (x / 100.0 * pw).clamp(0.0, pw - sw);
    let top = (y / 100.0 * ph).clamp(0.0, ph - sh);
    (left, ph - top - sh, sw, sh)
}

pub struct SignReport {
    pub total_pages: u32,
    pub signed_pages: Vec<u32>,
}

/// Place a picture of a signature on one or more pages. This is a visual
/// signature (the picture of a handwritten or typed name), not a certificate
/// based digital signature.
pub fn sign(
    input: &Path,
    image: &Path,
    page_spec: &str,
    x: f64,
    y: f64,
    width: f64,
    output: &Path,
) -> Result<SignReport> {
    ensure_distinct(input, output)?;
    if !(0.0..=100.0).contains(&x) || !(0.0..=100.0).contains(&y) {
        bail!("--x and --y must be between 0 and 100 (percent of the page)");
    }
    if !(1.0..=100.0).contains(&width) {
        bail!("--width must be between 1 and 100 (percent of the page width)");
    }
    let total = page_count(input)?;
    let mut pages = parse_page_spec(page_spec, total)?;
    pages.sort_unstable();
    pages.dedup();
    let sizes = page_sizes(input, total)?;
    let pic = imgpdf::load_rgba(image, 1000)?;
    let aspect = pic.h as f64 / pic.w as f64;

    let mut pdf = PdfBuilder::new();
    let catalog = pdf.reserve();
    let pages_id = pdf.reserve();
    let img_id = pdf.reserve();
    let mask_id = pdf.reserve();
    pdf.put_stream(
        mask_id,
        &format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray \
             /BitsPerComponent 8 /Filter /FlateDecode",
            pic.w, pic.h
        ),
        &pic.alpha,
    );
    pdf.put_stream(
        img_id,
        &format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /FlateDecode /SMask {mask_id} 0 R",
            pic.w, pic.h
        ),
        &pic.rgb,
    );

    let mut kids = String::new();
    for &n in &pages {
        let (pw, ph) = sizes[(n - 1) as usize];
        let (l, b, w, h) = signature_box(pw, ph, x, y, width, aspect);
        let c = pdf.reserve();
        let p = pdf.reserve();
        pdf.put_stream(c, "", format!("q {w:.2} 0 0 {h:.2} {l:.2} {b:.2} cm /Sig Do Q").as_bytes());
        pdf.put(
            p,
            &format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
                 /Resources << /XObject << /Sig {img_id} 0 R >> >> /Contents {c} 0 R >>"
            ),
        );
        let _ = write!(kids, "{p} 0 R ");
    }
    pdf.put(pages_id, &format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()));
    pdf.put(catalog, &format!("<< /Type /Catalog /Pages {pages_id} 0 R >>"));
    let overlay = pdf.finish(catalog)?;

    let tmp: PathBuf = std::env::temp_dir().join(format!("pdfsqueeze-sign-{}.pdf", std::process::id()));
    std::fs::write(&tmp, &overlay)?;
    let to = pages.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let args: Vec<OsString> = vec![
        input.into(),
        "--overlay".into(),
        (&tmp).into(),
        format!("--from=1-{}", pages.len()).into(),
        format!("--to={to}").into(),
        "--".into(),
        output.into(),
    ];
    let res = engine::run(&engine::qpdf()?, &args);
    let _ = std::fs::remove_file(&tmp);
    res?;
    Ok(SignReport { total_pages: total, signed_pages: pages })
}

#[cfg(test)]
mod sign_tests {
    use super::*;

    #[test]
    fn signature_stays_on_the_page() {
        // 600x800 page, signature 25% wide, image twice as wide as tall
        let (l, b, w, h) = signature_box(600.0, 800.0, 60.0, 80.0, 25.0, 0.5);
        assert_eq!((w, h), (150.0, 75.0));
        assert_eq!(l, 360.0);
        assert_eq!(b, 800.0 - 640.0 - 75.0); // top edge at 80% from the top
        // asking for a corner never leaves the page
        let (l, b, w, h) = signature_box(600.0, 800.0, 100.0, 100.0, 25.0, 0.5);
        assert_eq!(l + w, 600.0);
        assert_eq!(b, 0.0);
        assert!(h > 0.0);
        // a very tall image is shrunk to fit the page height
        let (_, _, w, h) = signature_box(600.0, 800.0, 0.0, 0.0, 100.0, 4.0);
        assert!(h <= 800.0 + 1e-9 && (w * 4.0 - h).abs() < 1e-6);
    }
}
