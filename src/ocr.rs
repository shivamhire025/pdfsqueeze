//! OCR: make scanned PDFs searchable.
//!
//! Each page without text is rendered to an image, Tesseract produces a PDF
//! containing *only* an invisible text layer (`textonly_pdf`), and qpdf lays
//! that layer over the original page. The original page content is never
//! re-encoded, so image quality and file size are unchanged apart from the
//! added text.

use crate::convert;
use crate::engine;
use crate::pages::page_count;
use crate::util::ensure_distinct;
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub struct Options {
    /// Tesseract language codes joined with `+`, e.g. `eng` or `eng+deu`.
    pub lang: String,
    /// Resolution pages are rendered at for recognition.
    pub dpi: u32,
    /// Pages processed at once.
    pub jobs: usize,
    /// OCR every page, even ones that already contain text.
    pub force: bool,
    /// Directory with `<lang>.traineddata` files (overrides the default).
    pub tessdata: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Report {
    pub total_pages: u32,
    pub ocr_pages: u32,
    pub already_text_pages: u32,
    /// Non-whitespace characters in the result.
    pub characters: usize,
}

/// A page with fewer than this many visible characters counts as "no text"
/// (stray page numbers or stamps should not stop a scan from being OCR'd).
const TEXT_THRESHOLD: usize = 25;

fn visible_chars(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// Which pages already carry real text, given each page's extracted text.
pub fn pages_with_text(page_texts: &[String]) -> Vec<bool> {
    page_texts
        .iter()
        .map(|t| visible_chars(t) >= TEXT_THRESHOLD)
        .collect()
}

fn env_for(opts: &Options) -> Vec<(&'static str, OsString)> {
    // Tesseract is multi-threaded itself; we parallelise across pages instead.
    let mut e: Vec<(&'static str, OsString)> = vec![("OMP_THREAD_LIMIT", "1".into())];
    if let Some(d) = &opts.tessdata {
        e.push(("TESSDATA_PREFIX", d.as_os_str().to_owned()));
    }
    e
}

fn check_languages(tesseract: &Path, opts: &Options) -> Result<()> {
    let env = env_for(opts);
    let out = engine::run_env(tesseract, &["--list-langs".into()], &env)?;
    let available: Vec<&str> = out
        .lines()
        .skip(1) // header line
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "osd")
        .collect();
    for want in opts.lang.split('+') {
        if !available.contains(&want) {
            bail!(
                "language `{want}` is not installed. Available: {}.\n\
                 Download `{want}.traineddata` from https://github.com/tesseract-ocr/tessdata_fast \
                 and pass its folder with --tessdata.",
                available.join(", ")
            );
        }
    }
    Ok(())
}

fn ocr_page(tesseract: &Path, input: &Path, dir: &Path, n: u32, opts: &Options) -> Result<PathBuf> {
    let png = dir.join(format!("p{n}.png"));
    convert::render_page_gray(input, n, opts.dpi, &png)
        .with_context(|| format!("rendering page {n} failed"))?;

    let out_base = dir.join(format!("o{n}"));
    let args: Vec<OsString> = vec![
        (&png).into(),
        (&out_base).into(),
        "-l".into(),
        opts.lang.clone().into(),
        "--dpi".into(),
        opts.dpi.to_string().into(),
        "-c".into(),
        "textonly_pdf=1".into(),
        "pdf".into(),
    ];
    engine::run_env(tesseract, &args, &env_for(opts))
        .with_context(|| format!("OCR failed on page {n}"))?;
    let _ = std::fs::remove_file(&png);
    Ok(out_base.with_extension("pdf"))
}

pub fn ocr(
    input: &Path,
    output: &Path,
    opts: &Options,
    progress: &(dyn Fn(u32, u32) + Sync),
) -> Result<Report> {
    ensure_distinct(input, output)?;
    let tesseract = engine::require("tesseract", "tesseract")?;
    let qpdf = engine::qpdf()?;
    check_languages(&tesseract, opts)?;

    let total = page_count(input)?;
    let has_text: Vec<bool> = if opts.force {
        vec![false; total as usize]
    } else {
        pages_with_text(&convert::page_texts(input, total)?)
    };
    let todo: Vec<u32> = (1..=total).filter(|&n| !has_text[(n - 1) as usize]).collect();
    let already = total - todo.len() as u32;

    if todo.is_empty() {
        std::fs::copy(input, output)?;
        return Ok(Report {
            total_pages: total,
            ocr_pages: 0,
            already_text_pages: already,
            characters: 0,
        });
    }

    let dir = std::env::temp_dir().join(format!("pdfsqueeze-ocr-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let cleanup = Cleanup(dir.clone());

    // Recognise pages in parallel.
    let jobs = opts.jobs.clamp(1, todo.len());
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<(u32, Result<PathBuf>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(&n) = todo.get(i) else { break };
                let r = ocr_page(&tesseract, input, &dir, n, opts);
                let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                progress(d as u32, todo.len() as u32);
                results.lock().unwrap().push((n, r));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(n, _)| *n);
    let mut layers: Vec<PathBuf> = Vec::new();
    for (_, r) in results {
        layers.push(r?);
    }

    // One overlay PDF whose page k holds the text layer of todo[k].
    let overlay = dir.join("overlay.pdf");
    let mut lines: Vec<String> = vec!["--empty".into(), "--pages".into()];
    lines.extend(layers.iter().map(|p| p.to_string_lossy().into_owned()));
    lines.push("--".into());
    lines.push(overlay.to_string_lossy().into_owned());
    engine::run_stdin(&qpdf, &["@-".into()], &(lines.join("\n") + "\n"))?;

    let to = todo.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let lines: Vec<String> = vec![
        input.to_string_lossy().into_owned(),
        "--overlay".into(),
        overlay.to_string_lossy().into_owned(),
        format!("--from=1-{}", todo.len()),
        format!("--to={to}"),
        "--".into(),
        output.to_string_lossy().into_owned(),
    ];
    engine::run_stdin(&qpdf, &["@-".into()], &(lines.join("\n") + "\n"))?;
    drop(cleanup);

    let characters = convert::page_texts(output, total)
        .map(|t| t.iter().map(|p| visible_chars(p)).sum())
        .unwrap_or(0);
    Ok(Report {
        total_pages: total,
        ocr_pages: todo.len() as u32,
        already_text_pages: already,
        characters,
    })
}

/// Removes the scratch folder even on early return.
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_text_pages() {
        let real = "This page has plenty of real text on it, definitely.".to_string();
        let pages = vec![real.clone(), "3".to_string(), real];
        assert_eq!(pages_with_text(&pages), vec![true, false, true]);
    }

    #[test]
    fn whitespace_does_not_count() {
        assert_eq!(pages_with_text(&["   \n\n  ".to_string()]), vec![false]);
    }
}
