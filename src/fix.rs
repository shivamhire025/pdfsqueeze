//! Repair, validation, web optimisation and PDF/A conversion.

use crate::engine;
use crate::util::ensure_distinct;
use anyhow::{bail, Result};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn warnings_in(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("qpdf: operation succeeded"))
        .map(String::from)
        .collect()
}

#[derive(Debug)]
pub struct CheckReport {
    /// 0 = clean, 1 = recoverable problems, 2 = unreadable
    pub severity: u8,
    pub messages: Vec<String>,
}

/// Structural validation. Does not judge visual content.
pub fn check(input: &Path) -> Result<CheckReport> {
    let c = engine::run_capture(
        &engine::qpdf()?,
        &["--check".into(), input.into()],
    )?;
    let mut messages = warnings_in(&c.stderr);
    // qpdf --check prints its verdict on stdout; keep only lines that are findings.
    messages.extend(
        c.stdout
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("WARNING") || l.starts_with("ERROR"))
            .map(String::from),
    );
    messages.dedup();
    let severity = match c.code {
        0 => 0,
        3 => 1,
        _ => 2,
    };
    Ok(CheckReport { severity, messages })
}

#[derive(Debug)]
pub struct RepairReport {
    pub method: &'static str,
    pub warnings: Vec<String>,
    /// The rewritten file passes a structural check.
    pub clean: bool,
    /// qpdf reported stream data it could not recover (typically a file that
    /// was cut off, e.g. an interrupted download). That content is gone.
    pub data_lost: bool,
}

fn lost_data(warnings: &[String]) -> bool {
    warnings
        .iter()
        .any(|w| w.contains("unable to recover stream data") || w.contains("treating stream as empty"))
}

/// Rewrite a damaged PDF. qpdf rebuilds a broken cross-reference table and
/// keeps content untouched; if it cannot read the file at all, Ghostscript
/// re-renders it, which salvages more but may drop features like forms.
pub fn repair(input: &Path, output: &Path) -> Result<RepairReport> {
    ensure_distinct(input, output)?;
    let q = engine::run_capture(&engine::qpdf()?, &[input.into(), output.into()])?;
    if q.code == 0 || q.code == 3 {
        let warnings = warnings_in(&q.stderr);
        let clean = check(output).map(|c| c.severity == 0).unwrap_or(false);
        return Ok(RepairReport {
            method: "qpdf (structure rebuilt, content untouched)",
            data_lost: lost_data(&warnings),
            warnings,
            clean,
        });
    }

    let gs = engine::require("gs", "ghostscript")?;
    let mut out_arg = OsString::from("-sOutputFile=");
    out_arg.push(output);
    let args: Vec<OsString> = vec![
        "-sDEVICE=pdfwrite".into(),
        "-dNOPAUSE".into(),
        "-dSAFER".into(),
        "-dBATCH".into(),
        "-dQUIET".into(),
        out_arg,
        input.into(),
    ];
    let g = engine::run_capture(&gs, &args)?;
    let wrote = std::fs::metadata(output).map(|m| m.len() > 0).unwrap_or(false);
    if g.code == 0 && wrote {
        let mut warnings = warnings_in(&q.stderr);
        warnings.push("qpdf could not read the file; it was re-rendered with Ghostscript".into());
        let clean = check(output).map(|c| c.severity == 0).unwrap_or(false);
        return Ok(RepairReport {
            method: "Ghostscript (re-rendered; interactive features may be lost)",
            data_lost: lost_data(&warnings),
            warnings,
            clean,
        });
    }
    let _ = std::fs::remove_file(output);
    bail!(
        "could not repair {}: the file is too damaged.\nqpdf: {}\nghostscript: {}",
        input.display(),
        q.stderr.trim(),
        g.stderr.trim()
    )
}

/// "Fast web view": reorder so the first page can display before the whole
/// file has downloaded. Lossless.
pub fn linearize(input: &Path, output: &Path) -> Result<()> {
    ensure_distinct(input, output)?;
    let args: Vec<OsString> = vec!["--linearize".into(), input.into(), output.into()];
    engine::run(&engine::qpdf()?, &args)?;
    Ok(())
}

/// Find an sRGB ICC profile: the bundle first, then next to the gs install.
fn find_icc(gs: &Path) -> Result<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(b) = engine::bundle_dir() {
        bases.push(b);
    }
    if let Ok(real) = gs.canonicalize() {
        // <prefix>/bin/gs -> <prefix>; Windows installs are <gs-dir>\bin\gswin64c.exe
        if let Some(root) = real.parent().and_then(|p| p.parent()) {
            bases.push(engine::plain_path(root.to_path_buf()));
        }
    }
    for base in bases {
        if let Some(root) = engine::ghostscript_root(&base) {
            let p = root.join("iccprofiles").join("default_rgb.icc");
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    bail!("could not find Ghostscript's RGB colour profile (default_rgb.icc)")
}

fn ps_string(s: &str) -> String {
    let mut o = String::from("(");
    for c in s.chars() {
        if matches!(c, '(' | ')' | '\\') {
            o.push('\\');
        }
        o.push(c);
    }
    o.push(')');
    o
}

#[derive(Debug)]
pub struct PdfaReport {
    pub has_output_intent: bool,
    pub declares_pdfa: bool,
}

/// Convert to PDF/A (archival). `part` is 1, 2 or 3.
///
/// Best effort via Ghostscript's PDF/A mode: fonts are embedded, colours are
/// converted to RGB, and an output intent is added. The result is *declared*
/// PDF/A but not validated here; use a validator such as veraPDF for
/// certification.
pub fn to_pdfa(input: &Path, output: &Path, part: u8) -> Result<PdfaReport> {
    ensure_distinct(input, output)?;
    if !(1..=3).contains(&part) {
        bail!("PDF/A part must be 1, 2 or 3");
    }
    let gs = engine::require("gs", "ghostscript")?;
    let icc = find_icc(&gs)?;

    let def = format!(
        "%!\n\
         /ICCProfile {icc} def\n\
         [/_objdef {{icc_PDFA}} /type /stream /OBJ pdfmark\n\
         [{{icc_PDFA}} << /N 3 >> /PUT pdfmark\n\
         [{{icc_PDFA}} ICCProfile (r) file /PUT pdfmark\n\
         [/_objdef {{OutputIntent_PDFA}} /type /dict /OBJ pdfmark\n\
         [{{OutputIntent_PDFA}} <<\n\
           /Type /OutputIntent /S /GTS_PDFA1\n\
           /DestOutputProfile {{icc_PDFA}}\n\
           /OutputConditionIdentifier (sRGB)\n\
         >> /PUT pdfmark\n\
         [{{Catalog}} << /OutputIntents [ {{OutputIntent_PDFA}} ] >> /PUT pdfmark\n",
        icc = ps_string(&icc.to_string_lossy())
    );
    let def_path =
        std::env::temp_dir().join(format!("pdfsqueeze-pdfa-{}.ps", std::process::id()));
    std::fs::write(&def_path, def)?;

    let mut out_arg = OsString::from("-sOutputFile=");
    out_arg.push(output);
    let mut permit = OsString::from("--permit-file-read=");
    permit.push(&icc);
    let args: Vec<OsString> = vec![
        format!("-dPDFA={part}").into(),
        "-dPDFACompatibilityPolicy=1".into(),
        "-sColorConversionStrategy=RGB".into(),
        "-sProcessColorModel=DeviceRGB".into(),
        "-sDEVICE=pdfwrite".into(),
        "-dNOPAUSE".into(),
        "-dSAFER".into(),
        "-dBATCH".into(),
        "-dQUIET".into(),
        permit,
        out_arg,
        (&def_path).into(),
        input.into(),
    ];
    let res = engine::run(&gs, &args);
    let _ = std::fs::remove_file(&def_path);
    res?;

    // Evidence it took, not a compliance guarantee.
    let q = engine::qpdf()?;
    let json = engine::run(
        &q,
        &[
            "--json=2".into(),
            "--json-key=qpdf".into(),
            "--json-object=trailer".into(),
            output.into(),
        ],
    )?;
    let v: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    let root_ref = v["qpdf"][1]["trailer"]["value"]["/Root"].as_str().unwrap_or("");
    let root_num = root_ref.split_whitespace().next().unwrap_or("");
    let mut report = PdfaReport {
        has_output_intent: false,
        declares_pdfa: false,
    };
    if !root_num.is_empty() {
        let rj = engine::run(
            &q,
            &[
                "--json=2".into(),
                "--json-key=qpdf".into(),
                format!("--json-object={root_num}").into(),
                output.into(),
            ],
        )?;
        let rv: Value = serde_json::from_str(&rj).unwrap_or(Value::Null);
        let root = &rv["qpdf"][1][format!("obj:{root_ref}")]["value"];
        report.has_output_intent = root.get("/OutputIntents").is_some();
        if let Some(m) = root.get("/Metadata").and_then(Value::as_str) {
            let n = m.split_whitespace().next().unwrap_or("");
            let xmp = engine::run(
                &q,
                &[format!("--show-object={n}").into(), "--filtered-stream-data".into(), output.into()],
            )
            .unwrap_or_default();
            report.declares_pdfa = xmp.contains("pdfaid");
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_strings_are_escaped() {
        assert_eq!(ps_string(r"/a (b)\c"), r"(/a \(b\)\\c)");
    }

    #[test]
    fn warnings_skip_summary_line() {
        let w = warnings_in("WARNING: x.pdf: file is damaged\nqpdf: operation succeeded with warnings\n");
        assert_eq!(w, vec!["WARNING: x.pdf: file is damaged"]);
    }
}
