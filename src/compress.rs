//! PDF compression via Ghostscript's pdfwrite device.
//!
//! Most of a PDF's weight is embedded images, so "compress" means
//! downsampling them to a chosen DPI and re-encoding as JPEG at a chosen
//! quality. Both knobs are explicit, which keeps results predictable: lower
//! DPI or lower quality always means a smaller file. (Letting Ghostscript pick
//! lossless-vs-JPEG per image made sizes jump around.)

use crate::engine;
use crate::pages::page_count;
use crate::progress;
use crate::util::ensure_distinct;
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum Level {
    /// Light compression, best quality.
    Low,
    /// Good balance for email and sharing.
    Recommended,
    /// Smallest files, screen quality.
    Extreme,
}

impl Level {
    pub fn settings(self) -> Settings {
        match self {
            Level::Low => Settings { dpi: 200, quality: 85 },
            Level::Recommended => Settings { dpi: 150, quality: 72 },
            Level::Extreme => Settings { dpi: 72, quality: 50 },
        }
    }
}

/// Image resolution cap (dots per inch) and JPEG quality (1-100).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub dpi: u32,
    pub quality: u8,
}

#[derive(Clone, Copy, Debug)]
pub enum Goal {
    Settings(Settings),
    /// Find the highest quality that fits under this many bytes.
    TargetBytes(u64),
}

#[derive(Debug)]
pub struct Outcome {
    pub before: u64,
    pub after: u64,
    pub settings: Settings,
    /// Compression would have made the file bigger, so the original was kept.
    pub kept_original: bool,
    /// Only meaningful for `Goal::TargetBytes`.
    pub target_met: bool,
}

/// Best quality first; every step is no larger than the one before it, so the
/// first rung that fits the target is the highest quality that does.
const TARGET_LADDER: &[Settings] = &[
    Settings { dpi: 300, quality: 90 },
    Settings { dpi: 250, quality: 85 },
    Settings { dpi: 200, quality: 80 },
    Settings { dpi: 150, quality: 75 },
    Settings { dpi: 150, quality: 65 },
    Settings { dpi: 120, quality: 60 },
    Settings { dpi: 100, quality: 55 },
    Settings { dpi: 72, quality: 50 },
    Settings { dpi: 72, quality: 40 },
    Settings { dpi: 60, quality: 35 },
    Settings { dpi: 48, quality: 30 },
];

/// Map JPEG quality (1-100, as in most image editors) to Ghostscript's
/// QFactor, which is libjpeg's quality scaling divided by 100.
pub fn qfactor(quality: u8) -> f64 {
    let q = quality.clamp(1, 100) as f64;
    let scale = if q < 50.0 { 5000.0 / q } else { 200.0 - 2.0 * q };
    (scale / 100.0).max(0.01)
}

pub fn compress(input: &Path, output: &Path, goal: Goal) -> Result<Outcome> {
    ensure_distinct(input, output)?;
    let gs = engine::require("gs", "ghostscript")?;
    let before = fs::metadata(input)
        .with_context(|| format!("cannot read {}", input.display()))?
        .len();

    let (best_tmp, settings, target_met) = match goal {
        Goal::Settings(s) => {
            let tmp = tmp_path(s);
            run_gs(&gs, input, &tmp, s, &Step { base: 0.0, span: 1.0, label: "Compressing".into() })?;
            (tmp, s, true)
        }
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
        settings,
        kept_original,
        target_met,
    })
}

fn ladder(gs: &Path, input: &Path, target: u64) -> Result<(PathBuf, Settings, bool)> {
    let mut last: Option<(PathBuf, Settings)> = None;
    let attempts = TARGET_LADDER.len();
    for (k, &s) in TARGET_LADDER.iter().enumerate() {
        let tmp = tmp_path(s);
        let step = Step {
            base: k as f64 / attempts as f64,
            span: 1.0 / attempts as f64,
            label: format!("Attempt {} of up to {attempts}", k + 1),
        };
        run_gs(gs, input, &tmp, s, &step)?;
        let size = fs::metadata(&tmp)?.len();
        if let Some((prev, _)) = last.take() {
            let _ = fs::remove_file(prev);
        }
        if size <= target {
            return Ok((tmp, s, true));
        }
        last = Some((tmp, s));
    }
    match last {
        Some((tmp, s)) => Ok((tmp, s, false)),
        None => bail!("no compression attempt ran"),
    }
}

fn tmp_path(s: Settings) -> PathBuf {
    std::env::temp_dir().join(format!(
        "pdfsqueeze-{}-{}-{}.pdf",
        std::process::id(),
        s.dpi,
        s.quality
    ))
}

/// Where one Ghostscript pass sits inside the whole job, for progress reports.
struct Step {
    base: f64,
    span: f64,
    label: String,
}

fn run_gs(gs: &Path, input: &Path, output: &Path, s: Settings, step: &Step) -> Result<()> {
    let preset = if s.dpi >= 225 {
        "/printer"
    } else if s.dpi >= 110 {
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
        "-dDetectDuplicateImages=true".into(),
        "-dCompressFonts=true".into(),
        "-dSubsetFonts=true".into(),
    ];
    for kind in ["Color", "Gray"] {
        args.push(format!("-dAutoFilter{kind}Images=false").into());
        args.push(format!("-d{kind}ImageFilter=/DCTEncode").into());
    }
    for kind in ["Color", "Gray", "Mono"] {
        args.push(format!("-dDownsample{kind}Images=true").into());
        args.push(format!("-d{kind}ImageResolution={}", s.dpi).into());
        args.push(format!("-d{kind}ImageDownsampleThreshold=1.0").into());
        args.push(format!("-d{kind}ImageDownsampleType=/Bicubic").into());
    }

    let mut out_arg = OsString::from("-sOutputFile=");
    out_arg.push(output);
    args.push(out_arg);

    // JPEG quality can only be set through distiller params. Chroma
    // subsampling (4:2:0) is used below quality 80 for extra savings.
    let q = qfactor(s.quality);
    let (h, v) = if s.quality < 80 {
        ("[2 1 1 2]", "[2 1 1 2]")
    } else {
        ("[1 1 1 1]", "[1 1 1 1]")
    };
    args.push("-c".into());
    args.push(
        format!(
            "<< /ColorImageDict << /QFactor {q:.3} /Blend 1 /HSamples {h} /VSamples {v} >> \
             /GrayImageDict << /QFactor {q:.3} /Blend 1 /HSamples [1 1 1 1] /VSamples [1 1 1 1] >> >> \
             setdistillerparams"
        )
        .into(),
    );
    args.push("-f".into());
    args.push(input.into());

    // Ghostscript prints "Page N" as it finishes each page; that is the progress.
    let pages = page_count(input).unwrap_or(0);
    progress::set(Some(step.base), &format!("{} · starting", step.label));
    engine::run_lines(gs, &args, |line| {
        if let Some(n) = engine::gs_page_line(line) {
            if pages > 0 {
                let f = step.base + step.span * (n.min(pages) as f64 / pages as f64);
                progress::set(Some(f), &format!("{} · page {} of {pages}", step.label, n.min(pages)));
            }
        }
    })?;
    progress::set(Some(step.base + step.span), &step.label);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qfactor_matches_libjpeg_scaling() {
        assert!((qfactor(75) - 0.5).abs() < 1e-9);
        assert!((qfactor(90) - 0.2).abs() < 1e-9);
        assert!((qfactor(50) - 1.0).abs() < 1e-9);
        assert!((qfactor(25) - 2.0).abs() < 1e-9);
        assert!(qfactor(100) > 0.0);
    }

    #[test]
    fn ladder_never_gets_better() {
        for w in TARGET_LADDER.windows(2) {
            assert!(w[1].dpi <= w[0].dpi, "dpi must not increase");
            assert!(w[1].quality <= w[0].quality, "quality must not increase");
        }
    }
}
