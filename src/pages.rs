//! Page-level operations (merge, split, extract, delete, reorder, rotate)
//! implemented on top of qpdf, which rewrites structure without re-rendering
//! content, so quality is never lost.

use crate::engine;
use crate::util::ensure_distinct;
use anyhow::{bail, Context, Result};
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

    #[test]
    fn bad_specs() {
        assert!(parse_page_spec("0", 10).is_err());
        assert!(parse_page_spec("11", 10).is_err());
        assert!(parse_page_spec("a-b", 10).is_err());
        assert!(parse_page_spec("", 10).is_err());
    }
}
