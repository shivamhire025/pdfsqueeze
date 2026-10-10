//! PDF -> images and PDF -> text, both through Ghostscript so the toolkit
//! needs only three engines (Ghostscript, qpdf, Tesseract) on every platform.

use crate::engine;
use crate::pages::page_count;
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum ImageFormat {
    Png,
    Jpg,
}

impl ImageFormat {
    fn ext(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpg => "jpg",
        }
    }
    fn device(self) -> &'static str {
        match self {
            ImageFormat::Png => "png16m",
            ImageFormat::Jpg => "jpeg",
        }
    }
}

/// A scratch folder that removes itself.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Result<Self> {
        let d = std::env::temp_dir().join(format!("pdfsqueeze-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d)?;
        Ok(Scratch(d))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn out_arg(pattern: &Path) -> OsString {
    let mut a = OsString::from("-sOutputFile=");
    a.push(pattern);
    a
}

/// Files in `dir` named `<prefix><number>.<ext>`, ordered by number.
fn numbered(dir: &Path, prefix: &str, ext: &str) -> Result<Vec<(u32, PathBuf)>> {
    let mut v: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            let n = name
                .strip_prefix(prefix)?
                .strip_suffix(&format!(".{ext}"))?
                .parse::<u32>()
                .ok()?;
            Some((n, p))
        })
        .collect();
    v.sort_by_key(|(n, _)| *n);
    Ok(v)
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    if std::fs::rename(from, to).is_err() {
        // different volume
        std::fs::copy(from, to).with_context(|| format!("cannot write {}", to.display()))?;
        std::fs::remove_file(from)?;
    }
    Ok(())
}

pub fn to_images(
    input: &Path,
    out_dir: &Path,
    format: ImageFormat,
    dpi: u32,
    first: Option<u32>,
    last: Option<u32>,
) -> Result<Vec<PathBuf>> {
    if dpi == 0 || dpi > 1200 {
        bail!("--dpi must be between 1 and 1200");
    }
    let gs = engine::require("gs", "ghostscript")?;
    let total = page_count(input)?;
    let (f, l) = (first.unwrap_or(1), last.unwrap_or(total));
    if f == 0 || f > total {
        bail!("page {f} is out of range (document has {total} pages)");
    }
    if l == 0 || l > total {
        bail!("page {l} is out of range (document has {total} pages)");
    }
    if f > l {
        bail!("--first ({f}) must not be after --last ({l})");
    }

    std::fs::create_dir_all(out_dir)?;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".into());
    let scratch = Scratch::new("render")?;
    let ext = format.ext();

    let mut args: Vec<OsString> = vec![
        "-q".into(),
        "-dNOPAUSE".into(),
        "-dBATCH".into(),
        format!("-sDEVICE={}", format.device()).into(),
        format!("-r{dpi}").into(),
        "-dTextAlphaBits=4".into(),
        "-dGraphicsAlphaBits=4".into(),
        format!("-dFirstPage={f}").into(),
        format!("-dLastPage={l}").into(),
    ];
    if matches!(format, ImageFormat::Jpg) {
        args.push("-dJPEGQ=90".into());
    }
    args.push(out_arg(&scratch.0.join(format!("p-%d.{ext}"))));
    args.push(input.into());
    engine::run(&gs, &args)?;

    // Ghostscript numbers outputs from 1; name them by the source page number,
    // zero-padded to the document's page count (page-07 of 120 pages is page-007).
    let width = total.to_string().len();
    let mut files = Vec::new();
    for (i, path) in numbered(&scratch.0, "p-", ext)? {
        let page = f + i - 1;
        let dest = out_dir.join(format!("{stem}-{page:0width$}.{ext}"));
        move_file(&path, &dest)?;
        files.push(dest);
    }
    if files.is_empty() {
        bail!("no pages were rendered");
    }
    Ok(files)
}

/// Render one page as grayscale PNG (used for OCR).
pub fn render_page_gray(input: &Path, page: u32, dpi: u32, out_png: &Path) -> Result<()> {
    let gs = engine::require("gs", "ghostscript")?;
    let args: Vec<OsString> = vec![
        "-q".into(),
        "-dNOPAUSE".into(),
        "-dBATCH".into(),
        "-sDEVICE=pnggray".into(),
        format!("-r{dpi}").into(),
        "-dTextAlphaBits=4".into(),
        "-dGraphicsAlphaBits=4".into(),
        format!("-dFirstPage={page}").into(),
        format!("-dLastPage={page}").into(),
        out_arg(out_png),
        input.into(),
    ];
    engine::run(&gs, &args)?;
    Ok(())
}

/// Raw text of every page, index 0 = page 1. Includes invisible text such as
/// an OCR layer.
pub fn page_texts(input: &Path, total: u32) -> Result<Vec<String>> {
    let gs = engine::require("gs", "ghostscript")?;
    let scratch = Scratch::new("text")?;
    let args: Vec<OsString> = vec![
        "-q".into(),
        "-dNOPAUSE".into(),
        "-dBATCH".into(),
        "-sDEVICE=txtwrite".into(),
        out_arg(&scratch.0.join("t-%d.txt")),
        input.into(),
    ];
    engine::run(&gs, &args)?;

    let mut texts = vec![String::new(); total as usize];
    for (n, path) in numbered(&scratch.0, "t-", "txt")? {
        if n >= 1 && (n as usize) <= texts.len() {
            texts[(n - 1) as usize] = String::from_utf8_lossy(&std::fs::read(&path)?).into_owned();
        }
    }
    Ok(texts)
}

/// Remove the page-wide left indent Ghostscript's absolute positioning adds,
/// trim trailing spaces and, unless `keep_layout`, squeeze runs of spaces.
pub fn tidy(text: &str, keep_layout: bool) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let indent = lines
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out = Vec::with_capacity(lines.len());
    for l in lines {
        let l = if l.len() >= indent { &l[indent..] } else { l };
        if keep_layout {
            out.push(l.to_string());
        } else {
            let lead = l.len() - l.trim_start().len();
            let body: Vec<&str> = l.split_whitespace().collect();
            out.push(format!("{}{}", " ".repeat(lead), body.join(" ")));
        }
    }
    // collapse 3+ blank lines left by sparse pages
    let mut joined = out.join("\n");
    while joined.contains("\n\n\n") {
        joined = joined.replace("\n\n\n", "\n\n");
    }
    joined.trim_matches('\n').to_string()
}

/// Write the document's text to `output`, pages separated by a form feed.
/// Returns the number of non-whitespace characters written.
pub fn to_text(input: &Path, output: &Path, keep_layout: bool) -> Result<u64> {
    let total = page_count(input)?;
    let texts = page_texts(input, total)?;
    let tidied: Vec<String> = texts.iter().map(|t| tidy(t, keep_layout)).collect();
    let body = tidied.join("\n\u{c}\n");
    std::fs::write(output, &body).with_context(|| format!("cannot write {}", output.display()))?;
    Ok(body.chars().filter(|c| !c.is_whitespace()).count() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_removes_common_indent() {
        let raw = "      Title here\n      Left      Right\n\n      end\n";
        assert_eq!(tidy(raw, true), "Title here\nLeft      Right\n\nend");
        assert_eq!(tidy(raw, false), "Title here\nLeft Right\n\nend");
    }

    #[test]
    fn tidy_keeps_relative_indent_in_layout_mode() {
        let raw = "    Heading\n        indented line\n";
        assert_eq!(tidy(raw, true), "Heading\n    indented line");
    }

    #[test]
    fn tidy_collapses_blank_runs() {
        assert_eq!(tidy("a\n\n\n\n\nb", true), "a\n\nb");
    }
}
