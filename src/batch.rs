//! Turning "what the user typed" (files and/or folders) into a list of
//! input -> output jobs for batch compression.

use crate::util::sibling;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub struct Job {
    pub input: PathBuf,
    pub output: PathBuf,
}

/// Marker our own default output name carries; such files are skipped when
/// scanning a folder so a second run does not re-compress earlier results.
const OUTPUT_SUFFIX: &str = "-compressed";

fn is_pdf(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

fn is_own_output(p: &Path) -> bool {
    p.file_stem()
        .is_some_and(|s| s.to_string_lossy().ends_with(OUTPUT_SUFFIX))
}

/// All PDFs under `dir`, sorted. Skips hidden files, our own outputs, symlinked
/// directories, and anything under `exclude`.
fn collect_pdfs(
    dir: &Path,
    recursive: bool,
    exclude: Option<&Path>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read folder {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for e in entries {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !recursive {
                continue;
            }
            if exclude.is_some_and(|x| path.canonicalize().is_ok_and(|c| c == x)) {
                continue;
            }
            collect_pdfs(&path, recursive, exclude, out)?;
        } else if ft.is_file() && is_pdf(&path) && !is_own_output(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Build the job list.
///
/// * `output`: explicit output file; only valid for a single file input.
/// * `out_dir`: write results here, keeping original file names (and the
///   folder structure below each input folder).
/// * otherwise results go next to each input as `<name>-compressed.pdf`.
pub fn plan(
    inputs: &[PathBuf],
    output: Option<&Path>,
    out_dir: Option<&Path>,
    recursive: bool,
) -> Result<Vec<Job>> {
    if output.is_some() && out_dir.is_some() {
        bail!("use either --output or --out-dir, not both");
    }
    if output.is_some() && (inputs.len() != 1 || inputs[0].is_dir()) {
        bail!("--output works with exactly one input file; use --out-dir for several files or a folder");
    }

    let exclude = out_dir.and_then(|d| d.canonicalize().ok());
    let mut jobs: Vec<Job> = Vec::new();

    for input in inputs {
        if input.is_dir() {
            let mut found = Vec::new();
            collect_pdfs(input, recursive, exclude.as_deref(), &mut found)?;
            if found.is_empty() {
                bail!(
                    "no PDFs found in {}{}",
                    input.display(),
                    if recursive { "" } else { " (use --recursive to include subfolders)" }
                );
            }
            for f in found {
                let output = match out_dir {
                    Some(d) => {
                        let rel = f.strip_prefix(input).unwrap_or(&f);
                        d.join(rel)
                    }
                    None => sibling(&f, "compressed"),
                };
                jobs.push(Job { input: f, output });
            }
        } else if input.is_file() {
            let output = match (output, out_dir) {
                (Some(o), _) => o.to_path_buf(),
                (None, Some(d)) => d.join(
                    input
                        .file_name()
                        .with_context(|| format!("bad file name {}", input.display()))?,
                ),
                (None, None) => sibling(input, "compressed"),
            };
            jobs.push(Job {
                input: input.clone(),
                output,
            });
        } else {
            bail!("{} does not exist", input.display());
        }
    }

    // Catch problems before any work starts.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for j in &jobs {
        if j.input == j.output
            || j.input
                .canonicalize()
                .ok()
                .zip(j.output.canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
        {
            bail!(
                "{} would be overwritten by its own output; pick a different --out-dir",
                j.input.display()
            );
        }
        if !seen.insert(j.output.clone()) {
            bail!(
                "two inputs would write to the same file: {}",
                j.output.display()
            );
        }
    }
    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pdfsqueeze-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        for f in ["a.pdf", "B.PDF", "notes.txt", ".hidden.pdf", "done-compressed.pdf", "sub/c.pdf"] {
            fs::write(root.join(f), b"x").unwrap();
        }
        root
    }

    #[test]
    fn folder_non_recursive_skips_noise() {
        let root = tree("flat");
        let jobs = plan(&[root.clone()], None, None, false).unwrap();
        let names: Vec<_> = jobs
            .iter()
            .map(|j| j.input.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["B.PDF", "a.pdf"]); // sorted, no sub/, hidden, txt or own output
        assert_eq!(jobs[1].output, root.join("a-compressed.pdf"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recursive_keeps_structure_in_out_dir() {
        let root = tree("rec");
        let out = root.join("out");
        let jobs = plan(&[root.clone()], None, Some(&out), true).unwrap();
        assert!(jobs.iter().any(|j| j.output == out.join("sub").join("c.pdf")));
        assert!(jobs.iter().any(|j| j.output == out.join("a.pdf")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_bad_combinations() {
        let root = tree("bad");
        assert!(plan(&[root.clone()], Some(&root.join("x.pdf")), None, false).is_err());
        assert!(plan(&[root.join("a.pdf")], Some(&root.join("x.pdf")), Some(&root), false).is_err());
        // out-dir equal to the input folder would overwrite originals
        assert!(plan(&[root.clone()], None, Some(&root), false).is_err());
        // same file name from two folders into one out-dir
        let r = plan(&[root.join("a.pdf"), root.join("sub/c.pdf"), root.join("sub/c.pdf")], None, Some(&root.join("o")), false);
        assert!(r.is_err());
        assert!(plan(&[root.join("missing.pdf")], None, None, false).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
