//! Thin wrapper around the external engines (Ghostscript, qpdf, ...).
//! Every feature shells out through here so tool discovery and error
//! reporting live in one place.

use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Searched after $PATH, so the tool also works when launched from a GUI
/// (Finder-launched apps get a minimal PATH).
const EXTRA_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

pub fn find(tool: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(EXTRA_DIRS.iter().map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(tool)).find(|c| c.is_file())
}

pub fn require(tool: &str, brew_pkg: &str) -> Result<PathBuf> {
    find(tool).with_context(|| {
        format!("`{tool}` not found. Install it with: brew install {brew_pkg}")
    })
}

pub fn qpdf() -> Result<PathBuf> {
    require("qpdf", "qpdf")
}

/// qpdf exits 3 for "succeeded with warnings", which we treat as success.
fn finish(tool: &Path, out: Output) -> Result<String> {
    let is_qpdf = tool.file_name().is_some_and(|n| n == "qpdf");
    let ok = out.status.success() || (is_qpdf && out.status.code() == Some(3));
    if !ok {
        bail!(
            "{} failed ({}):\n{}",
            tool.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run `tool` and return its stdout.
pub fn run(tool: &Path, args: &[OsString]) -> Result<String> {
    let out = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("failed to start {}", tool.display()))?;
    finish(tool, out)
}

/// Run `tool` feeding `stdin_data` on standard input. Used to hand secrets
/// to qpdf (`qpdf @-` reads its arguments from stdin) so they never appear
/// in the process list.
pub fn run_stdin(tool: &Path, args: &[OsString], stdin_data: &str) -> Result<String> {
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {}", tool.display()))?;
    child
        .stdin
        .take()
        .context("could not open stdin")?
        .write_all(stdin_data.as_bytes())?;
    let out = child.wait_with_output()?;
    finish(tool, out)
}

/// Exit code only, for yes/no probes like `qpdf --is-encrypted`.
pub fn exit_code(tool: &Path, args: &[OsString]) -> Result<i32> {
    let st = Command::new(tool)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to start {}", tool.display()))?;
    Ok(st.code().unwrap_or(-1))
}

/// Used by the `doctor` command: (tool, brew package, what needs it).
pub const TOOLS: &[(&str, &str, &str)] = &[
    ("gs", "ghostscript", "compress"),
    (
        "qpdf",
        "qpdf",
        "merge, split, pages, rotate, protect, unlock, strip-metadata, info, watermark, number-pages",
    ),
    ("pdftoppm", "poppler", "to-images"),
    ("pdfinfo", "poppler", "watermark, number-pages"),
    ("pdftotext", "poppler", "text"),
    ("tesseract", "tesseract", "OCR (planned)"),
];
