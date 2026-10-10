//! Page-level operations (merge, split, extract, delete, reorder, rotate)
//! implemented on top of qpdf, which rewrites structure without re-rendering
//! content, so quality is never lost.

use crate::engine;
use crate::util::ensure_distinct;
use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn qpdf() -> Result<PathBuf> {
    engine::require("qpdf", "qpdf")
}

pub fn page_count(input: &Path) -> Result<u32> {
    let out = engine::run(&qpdf()?, &["--show-npages".into(), input.into()])?;
    out.trim()
        .parse()
        .with_context(|| format!("could not read page count of {}", input.display()))
}


/// Displayed (width, height) in points of every page, in order.
///
/// Read from the page tree with inheritance resolved: MediaBox, CropBox and
/// Rotate may sit on a parent /Pages node. The visible area is the CropBox
/// when present, otherwise the MediaBox; 90/270 degree rotation swaps the two.
pub fn page_sizes(input: &Path) -> Result<Vec<(f64, f64)>> {
    let out = engine::run(
        &qpdf()?,
        &["--json=2".into(), "--json-key=qpdf".into(), input.into()],
    )?;
    let v: Value = serde_json::from_str(&out).context("could not parse qpdf JSON")?;
    let objs = v["qpdf"][1]
        .as_object()
        .context("unexpected qpdf JSON shape")?;
    sizes_from_objects(objs)
}

#[derive(Clone, Default)]
struct Inherited {
    media: Option<[f64; 4]>,
    crop: Option<[f64; 4]>,
    rotate: Option<i64>,
}

fn obj_value<'a>(objs: &'a Map<String, Value>, r: &str) -> Option<&'a Value> {
    objs.get(&format!("obj:{r}")).map(|o| &o["value"])
}

fn as_number(objs: &Map<String, Value>, v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if s.ends_with(" R") => obj_value(objs, s).and_then(|x| as_number(objs, x)),
        _ => None,
    }
}

fn as_box(objs: &Map<String, Value>, v: &Value) -> Option<[f64; 4]> {
    let arr = match v {
        Value::Array(a) => a,
        Value::String(s) if s.ends_with(" R") => obj_value(objs, s)?.as_array()?,
        _ => return None,
    };
    if arr.len() != 4 {
        return None;
    }
    let mut b = [0.0; 4];
    for (i, x) in arr.iter().enumerate() {
        b[i] = as_number(objs, x)?;
    }
    Some(b)
}

pub fn sizes_from_objects(objs: &Map<String, Value>) -> Result<Vec<(f64, f64)>> {
    let root_ref = objs
        .get("trailer")
        .and_then(|t| t["value"]["/Root"].as_str())
        .context("document has no /Root")?;
    let catalog = obj_value(objs, root_ref).context("catalog not found")?;
    let pages_ref = catalog["/Pages"].as_str().context("document has no page tree")?;

    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    walk(objs, pages_ref, Inherited::default(), &mut seen, &mut out, 0)?;
    Ok(out)
}

fn walk(
    objs: &Map<String, Value>,
    node_ref: &str,
    mut inh: Inherited,
    seen: &mut HashSet<String>,
    out: &mut Vec<(f64, f64)>,
    depth: usize,
) -> Result<()> {
    if depth > 64 || !seen.insert(node_ref.to_string()) {
        bail!("page tree is malformed (loop or too deep)");
    }
    let node = obj_value(objs, node_ref).context("page tree node not found")?;
    if let Some(m) = node.get("/MediaBox").and_then(|v| as_box(objs, v)) {
        inh.media = Some(m);
    }
    if let Some(c) = node.get("/CropBox").and_then(|v| as_box(objs, v)) {
        inh.crop = Some(c);
    }
    if let Some(r) = node.get("/Rotate").and_then(|v| as_number(objs, v)) {
        inh.rotate = Some(r as i64);
    }

    if let Some(kids) = node.get("/Kids").and_then(Value::as_array) {
        for k in kids {
            if let Some(r) = k.as_str() {
                walk(objs, r, inh.clone(), seen, out, depth + 1)?;
            }
        }
        return Ok(());
    }

    // a leaf page
    let b = inh.crop.or(inh.media).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let (mut w, mut h) = ((b[2] - b[0]).abs(), (b[3] - b[1]).abs());
    let rot = inh.rotate.unwrap_or(0).rem_euclid(360);
    if rot == 90 || rot == 270 {
        std::mem::swap(&mut w, &mut h);
    }
    out.push((w, h));
    Ok(())
}

/// Parse a page spec like `1-3,7,z` (1-based, `z` = last page).
/// Order is preserved and ranges may descend (`5-3`), so the same parser
/// serves extract, delete and reorder.
pub fn parse_page_spec(spec: &str, total: u32) -> Result<Vec<u32>> {
    let mut pages = Vec::new();
    for token in spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let (a, b) = match token.split_once('-') {
            Some((a, b)) => (page_num(a, total)?, page_num(b, total)?),
            None => {
                let p = page_num(token, total)?;
                (p, p)
            }
        };
        if a <= b {
            pages.extend(a..=b);
        } else {
            pages.extend((b..=a).rev());
        }
    }
    if pages.is_empty() {
        bail!("empty page selection");
    }
    Ok(pages)
}

fn page_num(s: &str, total: u32) -> Result<u32> {
    let s = s.trim();
    let n = if s.eq_ignore_ascii_case("z") || s.eq_ignore_ascii_case("last") {
        total
    } else {
        s.parse::<u32>()
            .with_context(|| format!("invalid page `{s}`"))?
    };
    if n == 0 || n > total {
        bail!("page {n} is out of range (document has {total} pages)");
    }
    Ok(n)
}

fn page_list(pages: &[u32]) -> String {
    pages
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Write `pages` (in the given order) of `input` to `output`.
pub fn select(input: &Path, pages: &[u32], output: &Path) -> Result<()> {
    ensure_distinct(input, output)?;
    let args: Vec<OsString> = vec![
        input.into(),
        "--pages".into(),
        ".".into(),
        page_list(pages).into(),
        "--".into(),
        output.into(),
    ];
    engine::run(&qpdf()?, &args)?;
    Ok(())
}

pub fn extract(input: &Path, spec: &str, output: &Path) -> Result<usize> {
    let total = page_count(input)?;
    let pages = parse_page_spec(spec, total)?;
    select(input, &pages, output)?;
    Ok(pages.len())
}

pub fn delete(input: &Path, spec: &str, output: &Path) -> Result<usize> {
    let total = page_count(input)?;
    let drop: Vec<u32> = parse_page_spec(spec, total)?;
    let keep: Vec<u32> = (1..=total).filter(|p| !drop.contains(p)).collect();
    if keep.is_empty() {
        bail!("that would delete every page");
    }
    select(input, &keep, output)?;
    Ok(keep.len())
}

/// `order` must name every page exactly once, e.g. `3,1,2`.
pub fn reorder(input: &Path, order: &str, output: &Path) -> Result<()> {
    let total = page_count(input)?;
    let pages = parse_page_spec(order, total)?;
    let mut sorted = pages.clone();
    sorted.sort_unstable();
    let expected: Vec<u32> = (1..=total).collect();
    if sorted != expected {
        bail!("reorder must list every page 1..={total} exactly once");
    }
    select(input, &pages, output)
}

pub fn merge(inputs: &[PathBuf], output: &Path) -> Result<()> {
    if inputs.len() < 2 {
        bail!("give at least two PDFs to merge");
    }
    for i in inputs {
        ensure_distinct(i, output)?;
    }
    let mut args: Vec<OsString> = vec!["--empty".into(), "--pages".into()];
    args.extend(inputs.iter().map(OsString::from));
    args.push("--".into());
    args.push(output.into());
    engine::run(&qpdf()?, &args)?;
    Ok(())
}

pub enum SplitMode {
    /// A new file every N pages.
    Every(u32),
    /// One file per comma-separated range, e.g. `1-3,4-9`.
    Ranges(String),
}

pub fn split(input: &Path, mode: SplitMode, out_dir: &Path) -> Result<Vec<PathBuf>> {
    let total = page_count(input)?;
    let groups: Vec<Vec<u32>> = match mode {
        SplitMode::Every(n) => {
            if n == 0 {
                bail!("--every must be at least 1");
            }
            (1..=total)
                .collect::<Vec<_>>()
                .chunks(n as usize)
                .map(|c| c.to_vec())
                .collect()
        }
        SplitMode::Ranges(spec) => spec
            .split(',')
            .map(|t| parse_page_spec(t, total))
            .collect::<Result<_>>()?,
    };

    std::fs::create_dir_all(out_dir)?;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "part".into());

    let mut written = Vec::new();
    for (i, pages) in groups.iter().enumerate() {
        let out = out_dir.join(format!("{stem}-part-{:03}.pdf", i + 1));
        select(input, pages, &out)?;
        written.push(out);
    }
    Ok(written)
}

/// Rotate clockwise by 90, 180 or 270 degrees. `spec = None` rotates all pages.
pub fn rotate(input: &Path, angle: u32, spec: Option<&str>, output: &Path) -> Result<()> {
    ensure_distinct(input, output)?;
    if ![90, 180, 270].contains(&angle) {
        bail!("angle must be 90, 180 or 270");
    }
    let rotate_arg = match spec {
        None => format!("--rotate=+{angle}"),
        Some(s) => {
            let total = page_count(input)?;
            let pages = parse_page_spec(s, total)?;
            format!("--rotate=+{angle}:{}", page_list(&pages))
        }
    };
    let args: Vec<OsString> = vec![rotate_arg.into(), input.into(), output.into()];
    engine::run(&qpdf()?, &args)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        assert_eq!(parse_page_spec("1-3,7", 10).unwrap(), vec![1, 2, 3, 7]);
        assert_eq!(parse_page_spec("8-z", 10).unwrap(), vec![8, 9, 10]);
        assert_eq!(parse_page_spec("3,1,2", 3).unwrap(), vec![3, 1, 2]);
        assert_eq!(parse_page_spec("5-3", 10).unwrap(), vec![5, 4, 3]);
    }

    fn objs(j: Value) -> Map<String, Value> {
        j.as_object().unwrap().clone()
    }

    #[test]
    fn sizes_inherit_crop_and_rotate() {
        let o = objs(serde_json::json!({
            "trailer": { "value": { "/Root": "1 0 R" } },
            "obj:1 0 R": { "value": { "/Pages": "2 0 R" } },
            // MediaBox + Rotate inherited from the root of the page tree
            "obj:2 0 R": { "value": { "/Kids": ["3 0 R", "4 0 R", "5 0 R"], "/MediaBox": [0, 0, 612, 792], "/Rotate": 90 } },
            "obj:3 0 R": { "value": { "/Type": "/Page" } },
            "obj:4 0 R": { "value": { "/Type": "/Page", "/Rotate": 0, "/MediaBox": [0, 0, 595.28, 841.89] } },
            "obj:5 0 R": { "value": { "/Type": "/Page", "/Rotate": 0, "/CropBox": [10, 10, 210, 310] } },
        }));
        let s = sizes_from_objects(&o).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], (792.0, 612.0)); // inherited 612x792 rotated 90
        assert!((s[1].0 - 595.28).abs() < 1e-9 && (s[1].1 - 841.89).abs() < 1e-9);
        assert_eq!(s[2], (200.0, 300.0)); // CropBox wins
    }

    #[test]
    fn page_tree_loop_is_an_error() {
        let o = objs(serde_json::json!({
            "trailer": { "value": { "/Root": "1 0 R" } },
            "obj:1 0 R": { "value": { "/Pages": "2 0 R" } },
            "obj:2 0 R": { "value": { "/Kids": ["2 0 R"] } },
        }));
        assert!(sizes_from_objects(&o).is_err());
    }

    #[test]
    fn bad_specs() {
        assert!(parse_page_spec("0", 10).is_err());
        assert!(parse_page_spec("11", 10).is_err());
        assert!(parse_page_spec("a-b", 10).is_err());
        assert!(parse_page_spec("", 10).is_err());
    }
}
