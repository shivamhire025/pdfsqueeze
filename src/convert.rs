//! PDF -> images and PDF -> text, via Poppler.

use crate::engine;
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
    let tool = engine::require("pdftoppm", "poppler")?;
    std::fs::create_dir_all(out_dir)?;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".into());

    let mut args: Vec<OsString> = vec!["-r".into(), dpi.to_string().into()];
    match format {
        ImageFormat::Png => args.push("-png".into()),
        ImageFormat::Jpg => {
            args.push("-jpeg".into());
            args.push("-jpegopt".into());
            args.push("quality=90".into());
        }
    }
    if let Some(f) = first {
        args.push("-f".into());
        args.push(f.to_string().into());
    }
    if let Some(l) = last {
        args.push("-l".into());
        args.push(l.to_string().into());
    }
    args.push(input.into());
    args.push(out_dir.join(&stem).into());
    engine::run(&tool, &args)?;

    // pdftoppm names files <stem>-<page>.<ext>, zero-padded to the page count.
    let prefix = format!("{stem}-");
    let ext = format.ext();
    let mut files: Vec<PathBuf> = std::fs::read_dir(out_dir)
        .with_context(|| format!("cannot list {}", out_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(&format!(".{ext}")))
        })
        .collect();
    files.sort();
    Ok(files)
}

pub fn to_text(input: &Path, output: &Path, keep_layout: bool) -> Result<u64> {
    let tool = engine::require("pdftotext", "poppler")?;
    let mut args: Vec<OsString> = Vec::new();
    if keep_layout {
        args.push("-layout".into());
    }
    args.push(input.into());
    args.push(output.into());
    engine::run(&tool, &args)?;
    // pdftotext emits form feeds between pages, so an image-only PDF still
    // yields a non-empty file; count only real characters.
    let text = std::fs::read(output)?;
    let real = String::from_utf8_lossy(&text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .count();
    Ok(real as u64)
}
