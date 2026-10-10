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

/// Folder holding the bundled engines (`bin/`, `lib/`, `share/`), if any.
/// Checked in order: `$PDFSQUEEZE_ENGINES`, then `engines/` next to the
/// executable (release tarball layout), then `../engines/`.
pub fn bundle_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("PDFSQUEEZE_ENGINES") {
        candidates.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe().and_then(|e| e.canonicalize()) {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("engines"));
            candidates.push(dir.join("../engines"));
        }
    }
    candidates
        .into_iter()
        .find(|c| c.join("bin").is_dir())
        .and_then(|c| c.canonicalize().ok())
}

/// Bundled engines win over anything on the system, so behaviour does not
/// depend on whatever versions a machine happens to have installed.
pub fn find(tool: &str) -> Option<PathBuf> {
    if let Some(b) = bundle_dir() {
        let p = b.join("bin").join(tool);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(EXTRA_DIRS.iter().map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(tool)).find(|c| c.is_file())
}

pub fn is_bundled(tool: &Path) -> bool {
    bundle_dir().is_some_and(|b| tool.starts_with(b.join("bin")))
}

/// A command for `tool`. Bundled engines are pointed at their own resources
/// so they never look for (or depend on) a Homebrew install.
fn command(tool: &Path) -> Command {
    let mut c = Command::new(tool);
    if let Some(b) = bundle_dir().filter(|b| tool.starts_with(b.join("bin"))) {
        let gs = b.join("share").join("ghostscript");
        let paths: Vec<String> = ["Resource/Init", "lib", "Resource/Font", "fonts"]
            .iter()
            .map(|d| gs.join(d).to_string_lossy().into_owned())
            .collect();
        c.env("GS_LIB", paths.join(":"));
        let fc = b.join("etc").join("fonts.conf");
        if fc.is_file() {
            c.env("FONTCONFIG_FILE", fc);
        }
    }
    c
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
    let out = command(tool)
        .args(args)
        .output()
        .with_context(|| format!("failed to start {}", tool.display()))?;
    finish(tool, out)
}

/// Run `tool` feeding `stdin_data` on standard input. Used to hand secrets
/// to qpdf (`qpdf @-` reads its arguments from stdin) so they never appear
/// in the process list.
pub fn run_stdin(tool: &Path, args: &[OsString], stdin_data: &str) -> Result<String> {
    let mut child = command(tool)
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
    let st = command(tool)
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
