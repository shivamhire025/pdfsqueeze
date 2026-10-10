//! Passwords, metadata and document info, all via qpdf.

use crate::engine;
use crate::util::{ensure_distinct, human};
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::path::Path;

/// qpdf reads its arguments from stdin with `@-`, one per line. Passwords go
/// this way so they are never visible in the process list.
fn run_secret(args_lines: &[String]) -> Result<()> {
    for l in args_lines {
        if l.contains('\n') || l.contains('\r') {
            bail!("passwords may not contain line breaks");
        }
    }
    let input = args_lines.join("\n") + "\n";
    engine::run_stdin(&engine::qpdf()?, &["@-".into()], &input)?;
    Ok(())
}

/// Surrounding whitespace is stripped by qpdf's argument-file reader, so
/// reject it rather than silently setting a different password.
pub fn check_password(p: &str) -> Result<()> {
    if p.is_empty() {
        bail!("password cannot be empty");
    }
    if p != p.trim() {
        bail!("password may not start or end with whitespace");
    }
    Ok(())
}

/// AES-256 encryption. Anyone needs `password` to open the file.
pub fn protect(input: &Path, password: &str, output: &Path) -> Result<()> {
    ensure_distinct(input, output)?;
    check_password(password)?;
    run_secret(&[
        "--encrypt".into(),
        password.into(),
        password.into(), // owner password: same, so restrictions cannot be lifted without it
        "256".into(),
        "--".into(),
        input.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
    ])
}

#[derive(Debug)]
pub enum UnlockError {
    NeedsPassword,
    Other(anyhow::Error),
}

pub fn unlock(input: &Path, password: Option<&str>, output: &Path) -> Result<(), UnlockError> {
    ensure_distinct(input, output).map_err(UnlockError::Other)?;
    let mut lines = Vec::new();
    if let Some(p) = password {
        check_password(p).map_err(UnlockError::Other)?;
        lines.push(format!("--password={p}"));
    }
    lines.push("--decrypt".into());
    lines.push(input.to_string_lossy().into_owned());
    lines.push(output.to_string_lossy().into_owned());
    match run_secret(&lines) {
        Ok(()) => Ok(()),
        Err(e) if format!("{e:#}").contains("invalid password") => Err(UnlockError::NeedsPassword),
        Err(e) => Err(UnlockError::Other(e)),
    }
}

/// Fetch qpdf's JSON for the named objects (`trailer` or object numbers).
fn fetch(input: &Path, objs: &[&str]) -> Result<Map<String, Value>> {
    let mut args: Vec<OsString> = vec!["--json=2".into(), "--json-key=qpdf".into()];
    for o in objs {
        args.push(format!("--json-object={o}").into());
    }
    args.push(input.into());
    let out = engine::run(&engine::qpdf()?, &args)?;
    let v: Value = serde_json::from_str(&out).context("could not parse qpdf JSON")?;
    v["qpdf"][1]
        .as_object()
        .cloned()
        .context("unexpected qpdf JSON shape")
}

fn dict_of(objs: &Map<String, Value>, key: &str) -> Result<Map<String, Value>> {
    objs.get(key)
        .and_then(|o| o["value"].as_object())
        .cloned()
        .with_context(|| format!("object `{key}` not found or not a dictionary"))
}

fn ref_num(r: &str) -> Option<&str> {
    r.split_whitespace().next()
}

#[derive(Debug, Default)]
pub struct StripReport {
    pub removed_info: bool,
    pub removed_xmp: bool,
}

/// Drop the document Info dictionary (title, author, creator, dates...) and
/// the document-level XMP metadata stream.
///
/// qpdf's JSON update replaces whole dictionaries, so we read the trailer and
/// catalog, remove the keys, and write them back.
pub fn strip_metadata(input: &Path, output: &Path) -> Result<StripReport> {
    ensure_distinct(input, output)?;

    let t = fetch(input, &["trailer"])?;
    let mut trailer = dict_of(&t, "trailer")?;
    let root_ref = trailer
        .get("/Root")
        .and_then(Value::as_str)
        .context("document has no /Root")?
        .to_string();
    let root_num = ref_num(&root_ref).context("bad /Root reference")?;

    let r = fetch(input, &[root_num])?;
    let root_key = format!("obj:{root_ref}");
    let mut root = dict_of(&r, &root_key)?;

    let report = StripReport {
        removed_info: trailer.remove("/Info").is_some(),
        removed_xmp: root.remove("/Metadata").is_some(),
    };
    root.remove("/PieceInfo");

    let update = json!({
        "qpdf": [
            { "jsonversion": 2 },
            {
                "trailer": { "value": trailer },
                root_key: { "value": root },
            }
        ]
    });

    let tmp = std::env::temp_dir().join(format!("pdfsqueeze-strip-{}.json", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec(&update)?)?;
    let mut flag = OsString::from("--update-from-json=");
    flag.push(&tmp);
    let args: Vec<OsString> = vec![input.into(), flag, output.into()];
    let res = engine::run(&engine::qpdf()?, &args);
    let _ = std::fs::remove_file(&tmp);
    res?;
    Ok(report)
}

#[derive(Debug, Default)]
pub struct InfoReport {
    pub file: String,
    pub size_bytes: u64,
    pub encrypted: bool,
    /// None when the file is locked.
    pub pages: Option<u32>,
    /// Whether document-level XMP metadata is present (None when locked).
    pub xmp: Option<bool>,
    /// Document Info dictionary entries, keys without the leading slash.
    pub info: std::collections::BTreeMap<String, String>,
}

impl InfoReport {
    pub fn to_human(&self) -> String {
        let mut out = format!(
            "File:       {}\nSize:       {}\nEncrypted:  {}\n",
            self.file,
            human(self.size_bytes),
            if self.encrypted { "yes" } else { "no" }
        );
        match self.pages {
            Some(n) => out.push_str(&format!("Pages:      {n}\n")),
            None => {
                out.push_str("Pages:      (locked: run `unlock` first)\n");
                return out;
            }
        }
        match self.xmp {
            Some(true) => out.push_str("XMP:        present\n"),
            Some(false) => out.push_str("XMP:        none\n"),
            None => {}
        }
        if self.info.is_empty() {
            out.push_str("Info:       none\n");
        } else {
            out.push_str("Info:\n");
            for (k, v) in &self.info {
                out.push_str(&format!("  {k:<14} {v}\n"));
            }
        }
        out
    }
}

/// Summary of a PDF: page count, encryption and metadata.
pub fn info(input: &Path) -> Result<InfoReport> {
    let size = std::fs::metadata(input)
        .with_context(|| format!("cannot read {}", input.display()))?
        .len();
    let mut rep = InfoReport {
        file: input.display().to_string(),
        size_bytes: size,
        ..Default::default()
    };

    let qpdf = engine::qpdf()?;
    rep.encrypted = engine::exit_code(&qpdf, &["--is-encrypted".into(), input.into()])? == 0;

    // Page count and metadata need the password for encrypted files.
    match engine::run(&qpdf, &["--show-npages".into(), input.into()]) {
        Ok(p) => rep.pages = p.trim().parse().ok(),
        Err(_) if rep.encrypted => return Ok(rep),
        Err(e) => return Err(e),
    }

    let t = fetch(input, &["trailer"])?;
    let trailer = dict_of(&t, "trailer")?;
    let root_ref = trailer.get("/Root").and_then(Value::as_str).unwrap_or("");
    if let Some(rn) = ref_num(root_ref) {
        let r = fetch(input, &[rn])?;
        if let Ok(root) = dict_of(&r, &format!("obj:{root_ref}")) {
            rep.xmp = Some(root.contains_key("/Metadata"));
        }
    }

    if let Some(info_ref) = trailer.get("/Info").and_then(Value::as_str) {
        let n = ref_num(info_ref).unwrap_or("0");
        let o = fetch(input, &[n])?;
        let d = dict_of(&o, &format!("obj:{info_ref}"))?;
        for (k, v) in &d {
            let val = match v.as_str() {
                Some(s) => s
                    .strip_prefix("u:")
                    .or_else(|| s.strip_prefix("b:"))
                    .unwrap_or(s)
                    .to_string(),
                None => v.to_string(),
            };
            rep.info.insert(k.trim_start_matches('/').to_string(), val);
        }
    }
    Ok(rep)
}
