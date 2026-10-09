//! PDF compression via Ghostscript's pdfwrite device.
//!
//! Most of a PDF's weight is embedded images, so "compress" mostly means
//! downsampling and re-encoding them at a chosen DPI.

use crate::engine;
use crate::util::ensure_distinct;
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum Level {
    /// Light compression, best quality (200 dpi images).
    Low,
    /// Good balance for email and sharing (150 dpi).
    Recommended,
    /// Smallest files, screen quality (72 dpi).
    Extreme,
}

impl Level {
    pub fn dpi(self) -> u32 {
        match self {
            Level::Low => 200,
            Level::Recommended => 150,
            Level::Extreme => 72,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Goal {
    Level(Level),
    Dpi(u32),
    /// Find the highest quality that fits under this many bytes.
    TargetBytes(u64),
}

#[derive(Debug)]
pub struct Outcome {
    pub before: u64,
    pub after: u64,
    pub dpi: Option<u32>,
    /// Compression would have made the file bigger, so the original was kept.
    pub kept_original: bool,
    /// Only meaningful for `Goal::TargetBytes`.
    pub target_met: bool,
}

/// Highest quality first; the first rung that fits the target wins.
const TARGET_LADDER: &[u32] = &[300, 225, 150, 120, 96, 72, 60, 48];

pub fn compress(input: &Path, output: &Path, goal: Goal) -> Result<Outcome> {
    ensure_distinct(input, output)?;
    let gs = engine::require("gs", "ghostscript")?;
    let before = fs::metadata(input)
        .with_context(|| format!("cannot read {}", input.display()))?
        .len();

    let (best_tmp, dpi, target_met) = match goal {
        Goal::Level(l) => single(&gs, input, l.dpi())?,
        Goal::Dpi(d) => single(&gs, input, d)?,
        Goal::TargetBytes(target) => ladder(&gs, input, target)?,
    };

    let after = fs::metadata(&best_tmp)?.len();
    let kept_original = after >= before;
    if kept_original {
        fs::copy(input, output)?;
    } else {
        fs::copy(&best_tmp, output)?;
    }
    let _ = fs::remove_file(&best_tmp);

    Ok(Outcome {
        before,
        after: if kept_original { before } else { after },
        dpi: Some(dpi),
        kept_original,
        target_met,
    })
}

fn single(gs: &Path, input: &Path, dpi: u32) -> Result<(PathBuf, u32, bool)> {
    let tmp = tmp_path(dpi);
    run_gs(gs, input, &tmp, dpi)?;
    Ok((tmp, dpi, true))
}

fn ladder(gs: &Path, input: &Path, target: u64) -> Result<(PathBuf, u32, bool)> {
    let mut last: Option<(PathBuf, u32)> = None;
    for &dpi in TARGET_LADDER {
        let tmp = tmp_path(dpi);
        run_gs(gs, input, &tmp, dpi)?;
        let size = fs::metadata(&tmp)?.len();
        if let Some((prev, _)) = last.take() {
            let _ = fs::remove_file(prev);
        }
        if size <= target {
            return Ok((tmp, dpi, true));
        }
        last = Some((tmp, dpi));
    }
    match last {
        Some((tmp, dpi)) => Ok((tmp, dpi, false)),
        None => bail!("no compression attempt ran"),
    }
}

fn tmp_path(dpi: u32) -> PathBuf {
    std::env::temp_dir().join(format!("pdfsqueeze-{}-{dpi}.pdf", std::process::id()))
}

fn run_gs(gs: &Path, input: &Path, output: &Path, dpi: u32) -> Result<()> {
    let preset = if dpi >= 225 {
        "/printer"
    } else if dpi >= 110 {
        "/ebook"
    } else {
        "/screen"
    };

    let mut args: Vec<OsString> = vec![
        "-sDEVICE=pdfwrite".into(),
        "-dCompatibilityLevel=1.5".into(),
        format!("-dPDFSETTINGS={preset}").into(),
        "-dNOPAUSE".into(),
        "-dBATCH".into(),
        "-dQUIET".into(),
        "-dDetectDuplicateImages=true".into(),
        "-dCompressFonts=true".into(),
        "-dSubsetFonts=true".into(),
    ];
    for kind in ["Color", "Gray", "Mono"] {
        args.push(format!("-dDownsample{kind}Images=true").into());
        args.push(format!("-d{kind}ImageResolution={dpi}").into());
        args.push(format!("-d{kind}ImageDownsampleThreshold=1.0").into());
        args.push(format!("-d{kind}ImageDownsampleType=/Bicubic").into());
    }

    let mut out_arg = OsString::from("-sOutputFile=");
    out_arg.push(output);
    args.push(out_arg);
    args.push(input.into());

    engine::run(gs, &args)?;
    Ok(())
}
