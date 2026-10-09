use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// "2MB", "500kb", "1.5 MB", "1048576" -> bytes.
pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim().to_ascii_lowercase();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.parse().with_context(|| format!("invalid size `{s}`"))?;
    let mult: f64 = match unit.trim() {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        other => bail!("unknown size unit `{other}` (use KB or MB)"),
    };
    Ok((n * mult) as u64)
}

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// `report.pdf` + `compressed` -> `report-compressed.pdf` next to the input.
pub fn sibling(input: &Path, suffix: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    input.with_file_name(format!("{stem}-{suffix}.pdf"))
}

/// Refuse to write over the file we are reading from.
pub fn ensure_distinct(input: &Path, output: &Path) -> Result<()> {
    if let (Ok(a), Ok(b)) = (input.canonicalize(), output.canonicalize()) {
        if a == b {
            bail!("output would overwrite the input file; choose a different --output");
        }
    }
    if input == output {
        bail!("output would overwrite the input file; choose a different --output");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("2MB").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_size("500 kb").unwrap(), 500 * 1024);
        assert_eq!(parse_size("1.5mb").unwrap(), (1.5 * 1024.0 * 1024.0) as u64);
        assert_eq!(parse_size("100").unwrap(), 100);
        assert!(parse_size("2XB").is_err());
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
    }

    #[test]
    fn sibling_name() {
        assert_eq!(
            sibling(Path::new("/a/report.pdf"), "compressed"),
            PathBuf::from("/a/report-compressed.pdf")
        );
    }
}
