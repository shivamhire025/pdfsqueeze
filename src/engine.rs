//! Thin wrapper around the external engines (Ghostscript, qpdf, ...).
//! Every feature shells out through here so tool discovery and error
//! reporting live in one place.

use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Operating systems we ship for. Passed explicitly to the pure helpers below
/// so Windows and Linux behaviour can be unit-tested from any machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Mac,
    Linux,
    Windows,
}

pub fn host_os() -> Os {
    if cfg!(target_os = "windows") {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Mac
    } else {
        Os::Linux
    }
}

/// File names to try for an engine. Windows executables carry `.exe`, and
/// Ghostscript's console build is `gswin64c.exe` / `gswin32c.exe` there.
pub fn exe_names(tool: &str, os: Os) -> Vec<String> {
    match (os, tool) {
        (Os::Windows, "gs") => ["gswin64c.exe", "gswin32c.exe", "gs.exe"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        (Os::Windows, t) => vec![format!("{t}.exe")],
        (_, t) => vec![t.to_string()],
    }
}

/// How to get an engine on this OS, shown when one is missing.
pub fn install_hint(tool: &str, pkg: &str, os: Os) -> String {
    match os {
        Os::Mac => format!("brew install {pkg}"),
        Os::Linux => {
            let apt = if tool == "tesseract" { "tesseract-ocr" } else { pkg };
            format!("sudo apt install {apt} (or your distribution's package manager)")
        }
        Os::Windows => format!(
            "the pdfsqueeze installer (it bundles {tool}), or install {pkg} and add it to PATH"
        ),
    }
}

/// Where engines usually live outside $PATH. Needed because apps started from
/// a GUI often get a minimal PATH.
fn extra_dirs(os: Os) -> Vec<PathBuf> {
    match os {
        Os::Mac => ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
            .iter()
            .map(PathBuf::from)
            .collect(),
        Os::Linux => ["/usr/local/bin", "/usr/bin", "/snap/bin"]
            .iter()
            .map(PathBuf::from)
            .collect(),
        Os::Windows => {
            let mut v = Vec::new();
            for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
                let Some(base) = std::env::var_os(var).map(PathBuf::from) else {
                    continue;
                };
                // Ghostscript: <Program Files>\gs\gs10.xx\bin, qpdf: ...\qpdf*\bin
                for parent in ["gs", ""] {
                    let dir = if parent.is_empty() { base.clone() } else { base.join(parent) };
                    if let Ok(rd) = std::fs::read_dir(&dir) {
                        for e in rd.flatten() {
                            let name = e.file_name().to_string_lossy().to_lowercase();
                            if name.starts_with("gs") || name.starts_with("qpdf") {
                                v.push(e.path().join("bin"));
                            }
                        }
                    }
                }
                v.push(base.join("Tesseract-OCR"));
            }
            v
        }
    }
}

/// Strip the `\\?\` prefix Windows adds when canonicalising. Ghostscript and
/// other C programs do not understand it.
pub fn plain_path(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if !rest.starts_with("UNC") {
            return PathBuf::from(rest);
        }
    }
    p
}

/// Folder holding the bundled engines (`bin/`, `share/`), if any.
/// Checked in order: `$PDFSQUEEZE_ENGINES`, then `engines/` next to the
/// executable (release package layout), then `../engines/`.
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
        .map(plain_path)
}

fn find_in(dir: &Path, tool: &str, os: Os) -> Option<PathBuf> {
    exe_names(tool, os)
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
}

/// Look for `tool` inside a bundle: `bin/` first, then a folder named after
/// the tool. The second form lets an engine keep its own DLLs apart from the
/// others (Windows loads DLLs from the executable's folder, and two engines
/// can ship different builds of the same library).
pub fn find_in_bundle(bundle: &Path, tool: &str, os: Os) -> Option<PathBuf> {
    find_in(&bundle.join("bin"), tool, os).or_else(|| find_in(&bundle.join(tool), tool, os))
}

/// Bundled engines win over anything on the system, so behaviour does not
/// depend on whatever versions a machine happens to have installed.
pub fn find(tool: &str) -> Option<PathBuf> {
    let os = host_os();
    if let Some(b) = bundle_dir() {
        if let Some(p) = find_in_bundle(&b, tool, os) {
            return Some(p);
        }
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(extra_dirs(os));
    dirs.iter().find_map(|d| find_in(d, tool, os))
}

pub fn is_bundled(tool: &Path) -> bool {
    bundle_dir().is_some_and(|b| tool.starts_with(&b))
}

/// The folder inside a Ghostscript install/bundle that holds `Resource/Init`,
/// `lib/` and `iccprofiles/`. Distributions differ: Homebrew puts them
/// directly under `share/ghostscript`, Debian, Arch and MSYS2 add a version
/// folder, and the Windows installer uses `<install>\Resource`.
pub fn ghostscript_root(base: &Path) -> Option<PathBuf> {
    let has_init = |d: &Path| d.join("Resource").join("Init").is_dir();
    let share = base.join("share").join("ghostscript");
    if has_init(&share) {
        return Some(share);
    }
    if let Ok(rd) = std::fs::read_dir(&share) {
        let mut subs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| has_init(p)).collect();
        subs.sort();
        if let Some(last) = subs.pop() {
            return Some(last); // newest version folder
        }
    }
    if has_init(base) {
        return Some(base.to_path_buf());
    }
    None
}

/// A command for `tool`. Bundled engines are pointed at their own resources
/// so they never look for (or depend on) a system install.
fn command(tool: &Path) -> Command {
    let mut c = Command::new(tool);
    if let Some(b) = bundle_dir().filter(|b| tool.starts_with(b)) {
        if let Some(gs) = ghostscript_root(&b) {
            let dirs: Vec<PathBuf> = ["Resource/Init", "lib", "Resource/Font", "fonts"]
                .iter()
                .map(|d| gs.join(d))
                .collect();
            // join_paths uses ':' or ';' as the platform requires
            if let Ok(joined) = std::env::join_paths(dirs) {
                c.env("GS_LIB", joined);
            }
        }
        let td = b.join("share").join("tessdata");
        if td.is_dir() {
            c.env("TESSDATA_PREFIX", td);
        }
        let fc = b.join("etc").join("fonts.conf");
        if fc.is_file() {
            c.env("FONTCONFIG_FILE", fc);
        }
    }
    c
}

pub fn require(tool: &str, pkg: &str) -> Result<PathBuf> {
    find(tool).with_context(|| {
        format!(
            "`{tool}` not found. Install it with: {}",
            install_hint(tool, pkg, host_os())
        )
    })
}

/// Oldest qpdf we support: JSON output version 2 and `--update-from-json`
/// arrived in qpdf 11. (Ubuntu 22.04 and Debian 11 still ship 10.x.)
const MIN_QPDF_MAJOR: u32 = 11;

/// "qpdf version 12.4.2" -> (12, 4, 2)
pub fn parse_qpdf_version(output: &str) -> Option<(u32, u32, u32)> {
    let rest = output.lines().find_map(|l| l.trim().strip_prefix("qpdf version "))?;
    let mut it = rest.trim().split(|c: char| !c.is_ascii_digit());
    let major = it.next()?.parse().ok()?;
    let minor = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let patch = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

fn check_qpdf_version(path: &Path) -> std::result::Result<(), String> {
    let out = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| format!("could not run {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    match parse_qpdf_version(&text) {
        Some((major, minor, patch)) if major < MIN_QPDF_MAJOR => Err(format!(
            "qpdf {major}.{minor}.{patch} at {} is too old: pdfsqueeze needs qpdf {MIN_QPDF_MAJOR} or newer. \
             Use the pdfsqueeze package (it bundles a current qpdf) or upgrade qpdf",
            path.display()
        )),
        _ => Ok(()), // new enough, or an unrecognised banner: do not block
    }
}

/// qpdf, checked once per run for a supported version.
pub fn qpdf() -> Result<PathBuf> {
    static CHECKED: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    let path = require("qpdf", "qpdf")?;
    match CHECKED.get_or_init(|| check_qpdf_version(&path)) {
        Ok(()) => Ok(path),
        Err(msg) => bail!("{msg}"),
    }
}

/// qpdf exits 3 for "succeeded with warnings", which we treat as success.
fn finish(tool: &Path, out: Output) -> Result<String> {
    let is_qpdf = tool.file_stem().is_some_and(|n| n == "qpdf");
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

/// Everything a finished process produced; non-zero exit is not an error here.
pub struct Captured {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Run `tool`, returning its exit code and both output streams. Only failing
/// to start the process is an error.
pub fn run_capture(tool: &Path, args: &[OsString]) -> Result<Captured> {
    let out = command(tool)
        .args(args)
        .output()
        .with_context(|| format!("failed to start {}", tool.display()))?;
    Ok(Captured {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Like `run`, with extra environment variables for the child.
pub fn run_env(tool: &Path, args: &[OsString], env: &[(&str, OsString)]) -> Result<String> {
    let mut c = command(tool);
    c.args(args);
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c
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
    (
        "gs",
        "ghostscript",
        "compress, to-images, text, ocr, pdfa, repair (fallback)",
    ),
    (
        "qpdf",
        "qpdf",
        "merge, split, pages, rotate, protect, unlock, strip-metadata, info, watermark, number-pages, repair, check, linearize, ocr",
    ),
    ("tesseract", "tesseract", "ocr"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_names_get_exe_and_ghostscript_variants() {
        assert_eq!(exe_names("qpdf", Os::Windows), vec!["qpdf.exe"]);
        assert_eq!(
            exe_names("gs", Os::Windows),
            vec!["gswin64c.exe", "gswin32c.exe", "gs.exe"]
        );
        assert_eq!(exe_names("gs", Os::Mac), vec!["gs"]);
        assert_eq!(exe_names("tesseract", Os::Linux), vec!["tesseract"]);
    }

    #[test]
    fn install_hints_are_per_platform() {
        assert_eq!(install_hint("gs", "ghostscript", Os::Mac), "brew install ghostscript");
        assert!(install_hint("tesseract", "tesseract", Os::Linux).contains("tesseract-ocr"));
        assert!(install_hint("qpdf", "qpdf", Os::Windows).contains("installer"));
    }

    #[test]
    fn missing_engine_message_keeps_the_classifiable_prefix() {
        // commands::classify relies on this exact phrase
        let msg = format!(
            "`gs` not found. Install it with: {}",
            install_hint("gs", "ghostscript", Os::Windows)
        );
        assert!(msg.to_lowercase().contains("not found. install it with"));
    }

    #[test]
    fn strips_windows_verbatim_prefix() {
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\C:\Apps\pdfsqueeze")),
            PathBuf::from(r"C:\Apps\pdfsqueeze")
        );
        // UNC shares keep their prefix
        let unc = PathBuf::from(r"\\?\UNC\server\share");
        assert_eq!(plain_path(unc.clone()), unc);
        assert_eq!(plain_path(PathBuf::from("/usr/bin")), PathBuf::from("/usr/bin"));
    }

    #[test]
    fn parses_qpdf_versions() {
        assert_eq!(parse_qpdf_version("qpdf version 12.4.2\nRun qpdf --copyright"), Some((12, 4, 2)));
        assert_eq!(parse_qpdf_version("qpdf version 10.6.3\n"), Some((10, 6, 3)));
        assert_eq!(parse_qpdf_version("qpdf version 11.9.1-rc1"), Some((11, 9, 1)));
        assert_eq!(parse_qpdf_version("something else"), None);
    }

    #[test]
    fn old_qpdf_is_rejected_with_a_clear_message() {
        // a fake "qpdf" that reports version 10 (unix only: needs a shell script)
        if !cfg!(unix) {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pdfsqueeze-fakeqpdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("qpdf");
        std::fs::write(&fake, "#!/bin/sh\necho 'qpdf version 10.6.3'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let err = check_qpdf_version(&fake).unwrap_err();
        assert!(err.contains("too old") && err.contains("11"), "{err}");
        std::fs::write(&fake, "#!/bin/sh\necho 'qpdf version 12.4.2'\n").unwrap();
        assert!(check_qpdf_version(&fake).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn engines_can_live_in_their_own_folder() {
        let root = std::env::temp_dir().join(format!("pdfsqueeze-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("tesseract")).unwrap();
        std::fs::write(root.join("bin/qpdf.exe"), b"x").unwrap();
        std::fs::write(root.join("tesseract/tesseract.exe"), b"x").unwrap();
        assert_eq!(find_in_bundle(&root, "qpdf", Os::Windows), Some(root.join("bin/qpdf.exe")));
        assert_eq!(
            find_in_bundle(&root, "tesseract", Os::Windows),
            Some(root.join("tesseract/tesseract.exe"))
        );
        assert_eq!(find_in_bundle(&root, "gs", Os::Windows), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn finds_ghostscript_resources_in_every_layout() {
        let root = std::env::temp_dir().join(format!("pdfsqueeze-gsroot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mk = |p: &str| std::fs::create_dir_all(root.join(p)).unwrap();

        // Homebrew: share/ghostscript/Resource/Init
        mk("brew/share/ghostscript/Resource/Init");
        assert_eq!(
            ghostscript_root(&root.join("brew")),
            Some(root.join("brew/share/ghostscript"))
        );
        // Debian / MSYS2: share/ghostscript/<version>/Resource/Init (newest wins)
        mk("deb/share/ghostscript/10.02.1/Resource/Init");
        mk("deb/share/ghostscript/10.05.0/Resource/Init");
        assert_eq!(
            ghostscript_root(&root.join("deb")),
            Some(root.join("deb/share/ghostscript/10.05.0"))
        );
        // Windows installer: <base>/Resource/Init
        mk("win/Resource/Init");
        assert_eq!(ghostscript_root(&root.join("win")), Some(root.join("win")));
        // nothing there
        mk("empty");
        assert_eq!(ghostscript_root(&root.join("empty")), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
