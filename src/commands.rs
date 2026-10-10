//! Command execution. Every command returns a [`Done`]: text for humans, a
//! JSON value for programs, warnings, and an exit code. Nothing here decides
//! how it is shown, so the CLI, `--json` and the MCP server share one path.

use crate::cli::*;
use crate::compress::{self, Goal, Level};
use crate::util::{human, parse_size, sibling};
use crate::{agent, batch, convert, engine, fix, imgpdf, ocr, pages, security, stamp};
use anyhow::{bail, Result};
use pages::SplitMode;
use serde_json::{json, Value};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// Exit codes, documented in AGENTS.md. Keep stable.
pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
/// The command ran, but the outcome has problems (damaged file, some batch
/// items failed, ...). Output may still have been produced.
pub const EXIT_PARTIAL: i32 = 3;

pub struct Ctx {
    /// Emit JSON and keep stdout machine-clean.
    pub json: bool,
    /// May we ask the user questions (passwords)? False for agents/scripts.
    pub interactive: bool,
}

impl Ctx {
    pub fn from_flag(json: bool) -> Self {
        Ctx {
            json,
            interactive: !json && std::io::stdin().is_terminal(),
        }
    }
}

pub struct Done {
    pub human: String,
    pub data: Value,
    pub warnings: Vec<String>,
    pub exit_code: i32,
}

impl Done {
    fn ok(human: impl Into<String>, data: Value) -> Self {
        Done {
            human: human.into(),
            data,
            warnings: Vec::new(),
            exit_code: EXIT_OK,
        }
    }
    fn warn(mut self, w: impl Into<String>) -> Self {
        self.warnings.push(w.into());
        self
    }
    fn code(mut self, c: i32) -> Self {
        self.exit_code = c;
        self
    }
}

/// An error with a stable machine-readable code.
#[derive(Debug)]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CliError {}

pub fn fail(code: &'static str, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(CliError {
        code,
        message: message.into(),
    })
}

/// Stable error codes agents can branch on.
pub const ERROR_CODES: &[(&str, &str)] = &[
    ("usage", "Invalid command line or tool arguments"),
    ("input_not_found", "An input file or folder does not exist or cannot be read"),
    ("no_input", "A folder contained no PDFs"),
    ("output_conflict", "The output would overwrite an input or another output"),
    ("invalid_argument", "A value is out of range or malformed (pages, sizes, angles, ...)"),
    ("engine_missing", "A required engine (Ghostscript, qpdf, ...) is not installed"),
    ("engine_failed", "An engine ran but reported an error"),
    ("engine_too_old", "An installed engine is older than pdfsqueeze supports (qpdf 11+)"),
    ("damaged_input", "The PDF is too damaged to process"),
    ("password_required", "The file is encrypted, or a password is needed, and none was supplied"),
    ("invalid_password", "The supplied password is wrong"),
    ("language_unavailable", "The requested OCR language data is not installed"),
    ("path_not_allowed", "The path is outside the folders this server may access"),
    ("not_available", "The feature is not available in this build"),
    ("error", "Any other failure"),
];

/// Map any error to a stable code. Typed errors win; the rest are recognised
/// by message so existing `bail!` sites do not all need touching.
pub fn classify(e: &anyhow::Error) -> &'static str {
    if let Some(c) = e.downcast_ref::<CliError>() {
        return c.code;
    }
    let m = format!("{e:#}").to_lowercase();
    let has = |s: &str| m.contains(s);
    if has("is too old: pdfsqueeze needs") {
        "engine_too_old"
    } else if has("not found. install it with") {
        "engine_missing"
    } else if has("wrong password") || has("invalid password") {
        "invalid_password"
    } else if has("would overwrite") || has("would be overwritten") || has("same file") {
        "output_conflict"
    } else if has("no pdfs found") {
        "no_input"
    } else if has("does not exist") || has("cannot read") || has("no such file") {
        "input_not_found"
    } else if has("is not installed. available") {
        "language_unavailable"
    } else if has("too damaged") {
        "damaged_input"
    } else if has("out of range")
        || has("must list every page")
        || has("empty page selection")
        || has("invalid page")
        || has("must be")
        || has("too low")
        || has("unknown size unit")
        || has("not supported yet")
        || has("angle must")
        || has("delete every page")
        || has("give either")
        || has("at least")
    {
        "invalid_argument"
    } else if has(" failed (") || has("failed on page") || has("ocr failed") {
        "engine_failed"
    } else {
        "error"
    }
}

/// The JSON envelope every command (and MCP tool call) returns.
pub fn envelope(command: &str, r: Result<Done>) -> (Value, i32) {
    match r {
        Ok(d) => {
            let v = json!({
                "ok": d.exit_code == EXIT_OK,
                "command": command,
                "result": d.data,
                "warnings": d.warnings,
                "exit_code": d.exit_code,
            });
            (v, d.exit_code)
        }
        Err(e) => {
            let code = classify(&e);
            let v = json!({
                "ok": false,
                "command": command,
                "error": { "code": code, "message": format!("{e:#}") },
                "exit_code": if code == "usage" { EXIT_USAGE } else { EXIT_ERROR },
            });
            (v, if code == "usage" { EXIT_USAGE } else { EXIT_ERROR })
        }
    }
}

pub fn command_name(c: &Cmd) -> &'static str {
    match c {
        Cmd::Compress { .. } => "compress",
        Cmd::Merge { .. } => "merge",
        Cmd::Split { .. } => "split",
        Cmd::Extract { .. } => "extract",
        Cmd::Delete { .. } => "delete",
        Cmd::Reorder { .. } => "reorder",
        Cmd::Rotate { .. } => "rotate",
        Cmd::ToImages { .. } => "to-images",
        Cmd::ImagesToPdf { .. } => "images-to-pdf",
        Cmd::Text { .. } => "text",
        Cmd::Protect { .. } => "protect",
        Cmd::Unlock { .. } => "unlock",
        Cmd::StripMetadata { .. } => "strip-metadata",
        Cmd::Info { .. } => "info",
        Cmd::Watermark { .. } => "watermark",
        Cmd::NumberPages { .. } => "number-pages",
        Cmd::Ocr { .. } => "ocr",
        Cmd::Repair { .. } => "repair",
        Cmd::Check { .. } => "check",
        Cmd::Linearize { .. } => "linearize",
        Cmd::Pdfa { .. } => "pdfa",
        Cmd::Doctor => "doctor",
        Cmd::Tools => "tools",
        Cmd::Mcp { .. } => "mcp",
    }
}

fn p(path: &Path) -> String {
    path.display().to_string()
}

fn saved_percent(before: u64, after: u64) -> f64 {
    let v = 100.0 - (after as f64 / before.max(1) as f64 * 100.0);
    (v * 10.0).round() / 10.0
}

/// Password for protect/unlock: --password-file, then $PDFSQUEEZE_PASSWORD,
/// then a hidden prompt, but only when a person is actually there to answer.
fn get_password(ctx: &Ctx, file: Option<&Path>, prompt: &str, confirm: bool) -> Result<String> {
    let pw = if let Some(f) = file {
        let raw = std::fs::read_to_string(f)
            .map_err(|e| fail("input_not_found", format!("cannot read {}: {e}", f.display())))?;
        raw.trim_end_matches(['\n', '\r']).to_string()
    } else if let Ok(v) = std::env::var("PDFSQUEEZE_PASSWORD") {
        v
    } else if ctx.interactive {
        let p = rpassword::prompt_password(prompt)?;
        if confirm && rpassword::prompt_password("Repeat password: ")? != p {
            bail!("passwords do not match");
        }
        p
    } else {
        return Err(fail(
            "password_required",
            "a password is required: pass --password-file <file> or set PDFSQUEEZE_PASSWORD",
        ));
    };
    security::check_password(&pw)?;
    Ok(pw)
}

pub fn execute(cmd: Cmd, ctx: &Ctx) -> Result<Done> {
    match cmd {
        Cmd::Compress {
            inputs,
            output,
            out_dir,
            recursive,
            level,
            dpi,
            quality,
            target_size,
        } => compress_cmd(ctx, inputs, output, out_dir, recursive, level, dpi, quality, target_size),

        Cmd::Merge { inputs, output } => {
            pages::merge(&inputs, &output)?;
            Ok(Done::ok(
                format!("Merged {} files into {}", inputs.len(), output.display()),
                json!({ "output": p(&output), "inputs": inputs.len() }),
            ))
        }

        Cmd::Split {
            input,
            every,
            ranges,
            out_dir,
        } => {
            let mode = match (every, ranges) {
                (Some(n), None) => SplitMode::Every(n),
                (None, Some(r)) => SplitMode::Ranges(r),
                _ => bail!("give either --every N or --ranges 1-3,4-9"),
            };
            let dir = out_dir.unwrap_or_else(|| {
                input
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
            });
            let files = pages::split(&input, mode, &dir)?;
            let mut human_out: String = files.iter().map(|f| format!("{}\n", f.display())).collect();
            human_out.push_str(&format!("Wrote {} files", files.len()));
            Ok(Done::ok(
                human_out,
                json!({ "outputs": files.iter().map(|f| p(f)).collect::<Vec<_>>(), "count": files.len() }),
            ))
        }

        Cmd::Extract {
            input,
            pages: spec,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "extract"));
            let n = pages::extract(&input, &spec, &output)?;
            Ok(Done::ok(
                format!("Kept {n} pages -> {}", output.display()),
                json!({ "output": p(&output), "pages_kept": n }),
            ))
        }

        Cmd::Delete {
            input,
            pages: spec,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "trimmed"));
            let n = pages::delete(&input, &spec, &output)?;
            Ok(Done::ok(
                format!("{n} pages remain -> {}", output.display()),
                json!({ "output": p(&output), "pages_remaining": n }),
            ))
        }

        Cmd::Reorder {
            input,
            order,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "reordered"));
            pages::reorder(&input, &order, &output)?;
            Ok(Done::ok(
                format!("Reordered -> {}", output.display()),
                json!({ "output": p(&output) }),
            ))
        }

        Cmd::Rotate {
            input,
            angle,
            pages: spec,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "rotated"));
            pages::rotate(&input, angle, spec.as_deref(), &output)?;
            Ok(Done::ok(
                format!("Rotated -> {}", output.display()),
                json!({ "output": p(&output), "angle": angle }),
            ))
        }

        Cmd::ToImages {
            input,
            out_dir,
            format,
            dpi,
            first,
            last,
        } => {
            let dir = out_dir.unwrap_or_else(|| {
                let stem = input
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "pages".into());
                input.with_file_name(format!("{stem}-images"))
            });
            let fmt = match format {
                ImgFmtArg::Png => convert::ImageFormat::Png,
                ImgFmtArg::Jpg => convert::ImageFormat::Jpg,
            };
            let files = convert::to_images(&input, &dir, fmt, dpi, first, last)?;
            Ok(Done::ok(
                format!("Wrote {} images to {}", files.len(), dir.display()),
                json!({ "out_dir": p(&dir), "files": files.iter().map(|f| p(f)).collect::<Vec<_>>(), "count": files.len() }),
            ))
        }

        Cmd::ImagesToPdf {
            inputs,
            output,
            page,
            dpi,
        } => {
            let size = match page {
                PageSizeArg::Fit => imgpdf::PageSize::Fit,
                PageSizeArg::A4 => imgpdf::PageSize::A4,
                PageSizeArg::Letter => imgpdf::PageSize::Letter,
            };
            let n = imgpdf::images_to_pdf(&inputs, &output, size, dpi)?;
            Ok(Done::ok(
                format!("Created {} ({n} pages)", output.display()),
                json!({ "output": p(&output), "pages": n }),
            ))
        }

        Cmd::Text {
            input,
            output,
            layout,
        } => {
            let output = output.unwrap_or_else(|| input.with_extension("txt"));
            let chars = convert::to_text(&input, &output, layout)?;
            let mut d = Done::ok(
                format!("Wrote {chars} characters to {}", output.display()),
                json!({ "output": p(&output), "characters": chars }),
            );
            if chars == 0 {
                d = d.warn("no text found. This may be a scanned PDF; try the `ocr` command.");
            }
            Ok(d)
        }

        Cmd::Protect {
            input,
            output,
            password_file,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "protected"));
            let pw = get_password(ctx, password_file.as_deref(), "New password: ", true)?;
            security::protect(&input, &pw, &output)?;
            Ok(Done::ok(
                format!("Encrypted (AES-256) -> {}", output.display()),
                json!({ "output": p(&output), "encryption": "AES-256" }),
            ))
        }

        Cmd::Unlock {
            input,
            output,
            password_file,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "unlocked"));
            // Many files have no user password (only edit restrictions), so try
            // without one first and only ask when it is actually needed.
            let given = if password_file.is_some() || std::env::var("PDFSQUEEZE_PASSWORD").is_ok() {
                Some(get_password(ctx, password_file.as_deref(), "Password: ", false)?)
            } else {
                None
            };
            match security::unlock(&input, given.as_deref(), &output) {
                Ok(()) => {}
                Err(security::UnlockError::NeedsPassword) if given.is_none() => {
                    let pw = get_password(ctx, None, "Password: ", false)?;
                    match security::unlock(&input, Some(&pw), &output) {
                        Ok(()) => {}
                        Err(security::UnlockError::NeedsPassword) => {
                            return Err(fail("invalid_password", "wrong password"))
                        }
                        Err(security::UnlockError::Other(e)) => return Err(e),
                    }
                }
                Err(security::UnlockError::NeedsPassword) => {
                    return Err(fail("invalid_password", "wrong password"))
                }
                Err(security::UnlockError::Other(e)) => return Err(e),
            }
            Ok(Done::ok(
                format!("Unlocked -> {}", output.display()),
                json!({ "output": p(&output) }),
            ))
        }

        Cmd::StripMetadata { input, output } => {
            let output = output.unwrap_or_else(|| sibling(&input, "clean"));
            let r = security::strip_metadata(&input, &output)?;
            Ok(Done::ok(
                format!(
                    "Info dictionary: {}\nXMP metadata:    {}\nSaved to {}",
                    if r.removed_info { "removed" } else { "none found" },
                    if r.removed_xmp { "removed" } else { "none found" },
                    output.display()
                ),
                json!({ "output": p(&output), "removed_info": r.removed_info, "removed_xmp": r.removed_xmp }),
            ))
        }

        Cmd::Info { input } => {
            let r = security::info(&input)?;
            let data = json!({
                "file": r.file,
                "size_bytes": r.size_bytes,
                "encrypted": r.encrypted,
                "pages": r.pages,
                "locked": r.encrypted && r.pages.is_none(),
                "xmp_present": r.xmp,
                "info": r.info,
            });
            Ok(Done::ok(r.to_human().trim_end().to_string(), data))
        }

        Cmd::Watermark {
            input,
            text,
            opacity,
            size,
            angle,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "watermarked"));
            let n = stamp::watermark(&input, &text, opacity, size, angle, &output)?;
            Ok(Done::ok(
                format!("Watermarked {n} pages -> {}", output.display()),
                json!({ "output": p(&output), "pages": n }),
            ))
        }

        Cmd::NumberPages {
            input,
            position,
            format,
            size,
            start,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "numbered"));
            let pos = match position {
                PositionArg::BottomCenter => stamp::Position::BottomCenter,
                PositionArg::BottomLeft => stamp::Position::BottomLeft,
                PositionArg::BottomRight => stamp::Position::BottomRight,
                PositionArg::TopCenter => stamp::Position::TopCenter,
                PositionArg::TopLeft => stamp::Position::TopLeft,
                PositionArg::TopRight => stamp::Position::TopRight,
            };
            let n = stamp::number_pages(&input, pos, &format, size, start, &output)?;
            Ok(Done::ok(
                format!("Numbered {n} pages -> {}", output.display()),
                json!({ "output": p(&output), "pages": n }),
            ))
        }

        Cmd::Ocr {
            input,
            output,
            lang,
            dpi,
            jobs,
            all,
            tessdata,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "ocr"));
            let jobs = jobs.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(2)
            });
            let opts = ocr::Options {
                lang,
                dpi,
                jobs,
                force: all,
                tessdata,
            };
            let show_progress = !ctx.json && std::io::stderr().is_terminal();
            let r = ocr::ocr(&input, &output, &opts, &|done, total| {
                if show_progress {
                    eprint!("\rRecognising pages: {done}/{total}");
                }
            })?;
            if show_progress && r.ocr_pages > 0 {
                eprintln!();
            }
            let data = json!({
                "output": p(&output),
                "total_pages": r.total_pages,
                "ocr_pages": r.ocr_pages,
                "already_text_pages": r.already_text_pages,
                "characters": r.characters,
            });
            let mut d = if r.ocr_pages == 0 {
                Done::ok(
                    format!(
                        "Nothing to do: all {} pages already contain text (use --all to force). Copied to {}",
                        r.total_pages,
                        output.display()
                    ),
                    data,
                )
            } else {
                Done::ok(
                    format!(
                        "Recognised {} of {} pages ({} already had text); {} characters of text now in the file.\nSaved to {}",
                        r.ocr_pages, r.total_pages, r.already_text_pages, r.characters, output.display()
                    ),
                    data,
                )
            };
            if r.ocr_pages > 0 && r.characters == 0 {
                d = d.warn("no text was recognised. Is the language right (--lang)? Is the scan very low quality?");
            }
            Ok(d)
        }

        Cmd::Repair { input, output } => {
            let output = output.unwrap_or_else(|| sibling(&input, "repaired"));
            let r = fix::repair(&input, &output)?;
            let mut text = if r.clean {
                format!("Repaired with {} -> {}", r.method, output.display())
            } else {
                format!(
                    "Partially recovered with {} -> {}\nThe result still has problems.",
                    r.method,
                    output.display()
                )
            };
            if r.data_lost {
                text.push_str(
                    "\nSome content could not be recovered: the file looks incomplete \
                     (for example an interrupted download). Re-downloading it is the real fix.",
                );
            }
            for w in r.warnings.iter().take(8) {
                text.push_str(&format!("\n  note: {w}"));
            }
            if r.warnings.len() > 8 {
                text.push_str(&format!("\n  note: ... and {} more", r.warnings.len() - 8));
            }
            let d = Done::ok(
                text,
                json!({
                    "output": p(&output),
                    "method": r.method,
                    "clean": r.clean,
                    "data_lost": r.data_lost,
                    "notes": r.warnings,
                }),
            );
            Ok(if r.clean { d } else { d.code(EXIT_PARTIAL) })
        }

        Cmd::Check { input } => {
            let r = fix::check(&input)?;
            let (status, head) = match r.severity {
                0 => ("ok", "OK: no problems found"),
                1 => ("problems", "Readable, but has problems (try `repair`):"),
                _ => ("damaged", "Damaged and could not be fully read (try `repair`):"),
            };
            let mut text = head.to_string();
            for m in r.messages.iter().take(8) {
                text.push_str(&format!("\n  {m}"));
            }
            if r.messages.len() > 8 {
                text.push_str(&format!("\n  ... and {} more", r.messages.len() - 8));
            }
            let d = Done::ok(
                text,
                json!({ "status": status, "severity": r.severity, "messages": r.messages }),
            );
            Ok(match r.severity {
                0 => d,
                1 => d.code(EXIT_PARTIAL),
                _ => d.code(EXIT_ERROR),
            })
        }

        Cmd::Linearize { input, output } => {
            let output = output.unwrap_or_else(|| sibling(&input, "web"));
            fix::linearize(&input, &output)?;
            Ok(Done::ok(
                format!("Optimised for web viewing -> {}", output.display()),
                json!({ "output": p(&output) }),
            ))
        }

        Cmd::Pdfa {
            input,
            part,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "pdfa"));
            let r = fix::to_pdfa(&input, &output, part)?;
            Ok(Done::ok(
                format!(
                    "Converted -> {}\n  output intent: {}\n  declares PDF/A: {}\n  note: best effort and not validated; use veraPDF to certify compliance.",
                    output.display(),
                    if r.has_output_intent { "yes" } else { "NO" },
                    if r.declares_pdfa { "yes" } else { "NO" }
                ),
                json!({
                    "output": p(&output),
                    "part": part,
                    "has_output_intent": r.has_output_intent,
                    "declares_pdfa": r.declares_pdfa,
                    "validated": false,
                }),
            ))
        }

        Cmd::Doctor => {
            let mut text = String::new();
            let mut engines = Vec::new();
            for (tool, pkg, needed_for) in engine::TOOLS {
                match engine::find(tool) {
                    Some(path) => {
                        let bundled = engine::is_bundled(&path);
                        text.push_str(&format!(
                            "ok       {tool:<10} {}{}  ({needed_for})\n",
                            path.display(),
                            if bundled { "  [bundled]" } else { "" }
                        ));
                        engines.push(json!({ "name": tool, "found": true, "path": p(&path), "bundled": bundled, "needed_for": needed_for }));
                    }
                    None => {
                        text.push_str(&format!(
                            "MISSING  {tool:<10} brew install {pkg}  ({needed_for})\n"
                        ));
                        engines.push(json!({ "name": tool, "found": false, "path": null, "bundled": false, "needed_for": needed_for }));
                    }
                }
            }
            Ok(Done::ok(text.trim_end().to_string(), json!({ "engines": engines })))
        }

        Cmd::Tools => Ok(agent::describe()),

        Cmd::Mcp { .. } => bail!("`mcp` runs as a server and cannot be called as a tool"),
    }
}

#[allow(clippy::too_many_arguments)]
fn compress_cmd(
    ctx: &Ctx,
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    recursive: bool,
    level: Option<LevelArg>,
    dpi: Option<u32>,
    quality: Option<u8>,
    target_size: Option<String>,
) -> Result<Done> {
    let goal = if let Some(t) = target_size {
        Goal::TargetBytes(parse_size(&t)?)
    } else {
        let mut st = match level.unwrap_or(LevelArg::Recommended) {
            LevelArg::Low => Level::Low,
            LevelArg::Recommended => Level::Recommended,
            LevelArg::Extreme => Level::Extreme,
        }
        .settings();
        if let Some(d) = dpi {
            if d < 10 {
                bail!("--dpi {d} is too low to be useful");
            }
            st.dpi = d;
        }
        if let Some(q) = quality {
            st.quality = q;
        }
        Goal::Settings(st)
    };
    let wants_target = matches!(goal, Goal::TargetBytes(_));

    let jobs = batch::plan(&inputs, output.as_deref(), out_dir.as_deref(), recursive)?;
    let total = jobs.len();
    let batch_mode = total > 1;
    let stream = batch_mode && !ctx.json;

    let (mut failed, mut kept, mut missed) = (0usize, 0usize, 0usize);
    let (mut before_sum, mut after_sum) = (0u64, 0u64);
    let mut files: Vec<Value> = Vec::new();
    let mut single_text = String::new();
    let mut warnings: Vec<String> = Vec::new();

    for (i, job) in jobs.iter().enumerate() {
        if let Some(parent) = job.output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match compress::compress(&job.input, &job.output, goal) {
            Err(e) if !batch_mode => return Err(e),
            Err(e) => {
                failed += 1;
                let msg = format!("{e:#}");
                if stream {
                    eprintln!("[{}/{total}] FAILED {}: {msg}", i + 1, job.input.display());
                }
                files.push(json!({
                    "input": p(&job.input),
                    "status": "failed",
                    "error": { "code": classify(&e), "message": msg },
                }));
            }
            Ok(o) => {
                before_sum += o.before;
                after_sum += o.after;
                if o.kept_original {
                    kept += 1;
                }
                if wants_target && !o.target_met {
                    missed += 1;
                }
                let saved = saved_percent(o.before, o.after);
                // Next to the originals an identical copy is just clutter; with
                // --out-dir the copy keeps the output set complete.
                let leave_alone = batch_mode && o.kept_original && out_dir.is_none();
                if leave_alone {
                    let _ = std::fs::remove_file(&job.output);
                }
                files.push(json!({
                    "input": p(&job.input),
                    "output": if leave_alone { Value::Null } else { json!(p(&job.output)) },
                    "status": "ok",
                    "before_bytes": o.before,
                    "after_bytes": o.after,
                    "saved_percent": saved,
                    "dpi": o.settings.dpi,
                    "quality": o.settings.quality,
                    "kept_original": o.kept_original,
                    "target_met": if wants_target { json!(o.target_met) } else { Value::Null },
                }));

                let name = job.input.display();
                if stream {
                    let line = if leave_alone {
                        format!("[{}/{total}] {name}: already optimal ({}), left as is", i + 1, human(o.before))
                    } else if o.kept_original {
                        format!("[{}/{total}] {name}: already optimal ({}), copied unchanged", i + 1, human(o.before))
                    } else {
                        format!("[{}/{total}] {name}: {} -> {} ({saved:.0}% smaller)", i + 1, human(o.before), human(o.after))
                    };
                    println!("{line}");
                } else if !batch_mode {
                    single_text = if o.kept_original {
                        format!(
                            "Already optimal: compression would not make it smaller. Copied unchanged to {}",
                            job.output.display()
                        )
                    } else {
                        format!(
                            "{} -> {} ({saved:.0}% smaller, images at {} dpi, JPEG quality {})\nSaved to {}",
                            human(o.before),
                            human(o.after),
                            o.settings.dpi,
                            o.settings.quality,
                            job.output.display()
                        )
                    };
                }
            }
        }
    }

    if wants_target && missed > 0 && !batch_mode {
        warnings.push("could not reach the target size; this is the smallest result".into());
    }

    let processed = total - failed;
    let summary = json!({
        "files": total,
        "processed": processed,
        "failed": failed,
        "already_optimal": kept,
        "missed_target": if wants_target { json!(missed) } else { Value::Null },
        "before_bytes": before_sum,
        "after_bytes": after_sum,
        "saved_percent": saved_percent(before_sum, after_sum),
    });

    let human_text = if batch_mode {
        let mut t = format!(
            "{processed}/{total} files processed: {} -> {} ({:.0}% smaller overall)",
            human(before_sum),
            human(after_sum),
            saved_percent(before_sum, after_sum)
        );
        if kept > 0 {
            t.push_str(&format!("\n{kept} already optimal (not made smaller)"));
        }
        if missed > 0 {
            t.push_str(&format!("\n{missed} could not reach the target size (smallest result kept)"));
        }
        if failed > 0 {
            t.push_str(&format!("\n{failed} file(s) failed"));
        }
        t
    } else {
        single_text
    };

    let mut d = Done::ok(human_text, json!({ "files": files, "summary": summary }));
    d.warnings = warnings;
    if failed > 0 {
        d = d.code(if failed == total { EXIT_ERROR } else { EXIT_PARTIAL });
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(msg: &str) -> &'static str {
        classify(&anyhow::anyhow!("{msg}"))
    }

    #[test]
    fn classifies_known_failures() {
        assert_eq!(c("`gs` not found. Install it with: brew install ghostscript"), "engine_missing");
        assert_eq!(c("nope.pdf does not exist"), "input_not_found");
        assert_eq!(c("cannot read x.pdf: No such file"), "input_not_found");
        assert_eq!(c("wrong password"), "invalid_password");
        assert_eq!(c("output would overwrite the input file; choose a different --output"), "output_conflict");
        assert_eq!(c("no PDFs found in inbox"), "no_input");
        assert_eq!(c("page 9 is out of range (document has 4 pages)"), "invalid_argument");
        assert_eq!(c("reorder must list every page 1..=4 exactly once"), "invalid_argument");
        assert_eq!(c("--dpi 3 is too low to be useful"), "invalid_argument");
        assert_eq!(c("language `deu` is not installed. Available: eng."), "language_unavailable");
        assert_eq!(c("could not repair x.pdf: the file is too damaged."), "damaged_input");
        assert_eq!(c("/x/gs failed (exit status: 1):\nboom"), "engine_failed");
        assert_eq!(
            c("qpdf 10.6.3 at /usr/bin/qpdf is too old: pdfsqueeze needs qpdf 11 or newer."),
            "engine_too_old"
        );
        assert_eq!(c("something unexpected"), "error");
    }

    #[test]
    fn typed_errors_win() {
        let e = fail("password_required", "a password is required");
        assert_eq!(classify(&e), "password_required");
    }

    #[test]
    fn envelope_shapes() {
        let (v, code) = envelope("merge", Ok(Done::ok("x", json!({"output": "a.pdf"}))));
        assert_eq!(code, 0);
        assert_eq!(v["ok"], true);
        assert_eq!(v["result"]["output"], "a.pdf");

        let (v, code) = envelope("merge", Err(anyhow::anyhow!("nope.pdf does not exist")));
        assert_eq!(code, 1);
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "input_not_found");

        let (v, code) = envelope("check", Ok(Done::ok("x", json!({})).code(EXIT_PARTIAL)));
        assert_eq!(code, 3);
        assert_eq!(v["ok"], false);
        assert_eq!(v["exit_code"], 3);
    }

    #[test]
    fn percent_rounds_sensibly() {
        assert_eq!(saved_percent(1000, 250), 75.0);
        assert_eq!(saved_percent(0, 0), 100.0 - 0.0);
    }
}
