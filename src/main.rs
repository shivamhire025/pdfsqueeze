mod batch;
mod compress;
mod convert;
mod engine;
mod imgpdf;
mod pages;
mod pdfout;
mod security;
mod stamp;
mod util;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand, ValueEnum};
use compress::{Goal, Level};
use pages::SplitMode;
use std::path::PathBuf;
use util::{human, parse_size, sibling};

#[derive(Clone, Copy, ValueEnum)]
enum ImgFmtArg {
    Png,
    Jpg,
}

#[derive(Clone, Copy, ValueEnum)]
enum PageSizeArg {
    /// Page matches the image size
    Fit,
    A4,
    Letter,
}

#[derive(Clone, Copy, ValueEnum)]
enum PositionArg {
    BottomCenter,
    BottomLeft,
    BottomRight,
    TopCenter,
    TopLeft,
    TopRight,
}

/// Password from $PDFSQUEEZE_PASSWORD (for scripts) or a hidden prompt.
/// Never accepted as a flag: command-line arguments are visible to every
/// process on the machine.
fn get_password(prompt: &str, confirm: bool) -> Result<String> {
    let p = match std::env::var("PDFSQUEEZE_PASSWORD") {
        Ok(p) => p,
        Err(_) => {
            let p = rpassword::prompt_password(prompt)?;
            if confirm {
                let again = rpassword::prompt_password("Repeat password: ")?;
                if again != p {
                    bail!("passwords do not match");
                }
            }
            p
        }
    };
    security::check_password(&p)?;
    Ok(p)
}

#[derive(Parser)]
#[command(
    name = "pdfsqueeze",
    version,
    about = "Offline PDF toolkit. Your files never leave this machine."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum LevelArg {
    /// Light compression, best quality
    Low,
    /// Balanced; good for email and sharing
    Recommended,
    /// Smallest file, screen quality
    Extreme,
}

#[derive(Subcommand)]
enum Cmd {
    /// Shrink PDFs: files and/or folders (presets, custom DPI, or a target size)
    Compress {
        /// PDF files and/or folders containing PDFs
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<PathBuf>,
        /// Output file, for a single input file (default: <name>-compressed.pdf)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write results to this folder, keeping the original file names
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Include PDFs in subfolders when an input is a folder
        #[arg(short, long)]
        recursive: bool,
        /// Compression preset (default: recommended)
        #[arg(short, long, value_enum, conflicts_with_all = ["dpi", "target_size"])]
        level: Option<LevelArg>,
        /// Downsample images to this resolution
        #[arg(long, conflicts_with = "target_size")]
        dpi: Option<u32>,
        /// Keep the best quality that fits, e.g. 2MB or 500KB
        #[arg(long)]
        target_size: Option<String>,
    },
    /// Join several PDFs into one, in the order given
    Merge {
        #[arg(required = true, num_args = 2..)]
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Split a PDF into several files
    Split {
        input: PathBuf,
        /// New file every N pages
        #[arg(long, conflicts_with = "ranges")]
        every: Option<u32>,
        /// One file per range, e.g. 1-3,4-9
        #[arg(long)]
        ranges: Option<String>,
        /// Directory for the parts (default: next to the input)
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
    /// Keep only the given pages, e.g. 1-3,7,z
    Extract {
        input: PathBuf,
        pages: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Remove the given pages
    Delete {
        input: PathBuf,
        pages: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Reorder pages; list every page once, e.g. 3,1,2
    Reorder {
        input: PathBuf,
        order: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Rotate pages clockwise by 90, 180 or 270 degrees
    Rotate {
        input: PathBuf,
        angle: u32,
        /// Pages to rotate (default: all)
        #[arg(long)]
        pages: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Render pages as PNG or JPG images
    ToImages {
        input: PathBuf,
        /// Directory for the images (default: <name>-images next to the input)
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "png")]
        format: ImgFmtArg,
        #[arg(long, default_value_t = 150)]
        dpi: u32,
        /// First page to render (1-based)
        #[arg(long)]
        first: Option<u32>,
        /// Last page to render
        #[arg(long)]
        last: Option<u32>,
    },
    /// Combine images (JPG, PNG, GIF, BMP, WebP, TIFF) into one PDF, one per page
    ImagesToPdf {
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value = "fit")]
        page: PageSizeArg,
        /// Resolution used to size pages in `fit` mode
        #[arg(long, default_value_t = 150.0)]
        dpi: f64,
    },
    /// Extract the text of a PDF
    Text {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Keep the original column layout
        #[arg(long)]
        layout: bool,
    },
    /// Encrypt with a password (AES-256). Prompts for the password.
    Protect {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Remove password protection. Prompts if a password is needed.
    Unlock {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Remove document metadata (author, title, creator, dates, XMP)
    StripMetadata {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Show page count, encryption and metadata
    Info { input: PathBuf },
    /// Stamp diagonal text across every page
    Watermark {
        input: PathBuf,
        /// Watermark text (ASCII), e.g. CONFIDENTIAL
        text: String,
        #[arg(long, default_value_t = 0.2)]
        opacity: f64,
        /// Font size in points (default: fit to the page)
        #[arg(long)]
        size: Option<f64>,
        /// Counter-clockwise rotation in degrees
        #[arg(long, default_value_t = 45.0)]
        angle: f64,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Add page numbers
    NumberPages {
        input: PathBuf,
        #[arg(long, value_enum, default_value = "bottom-center")]
        position: PositionArg,
        /// Label template; {n} is the page number, {total} the last number
        #[arg(long, default_value = "{n}")]
        format: String,
        #[arg(long, default_value_t = 10.0)]
        size: f64,
        /// Number given to the first page
        #[arg(long, default_value_t = 1)]
        start: i64,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check which engines are installed
    Doctor,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Compress {
            inputs,
            output,
            out_dir,
            recursive,
            level,
            dpi,
            target_size,
        } => {
            let goal = if let Some(t) = target_size {
                Goal::TargetBytes(parse_size(&t)?)
            } else if let Some(d) = dpi {
                if d < 10 {
                    bail!("--dpi {d} is too low to be useful");
                }
                Goal::Dpi(d)
            } else {
                Goal::Level(match level.unwrap_or(LevelArg::Recommended) {
                    LevelArg::Low => Level::Low,
                    LevelArg::Recommended => Level::Recommended,
                    LevelArg::Extreme => Level::Extreme,
                })
            };
            let wants_target = matches!(goal, Goal::TargetBytes(_));
            let jobs = batch::plan(&inputs, output.as_deref(), out_dir.as_deref(), recursive)?;
            let batch_mode = jobs.len() > 1;
            let total = jobs.len();

            let (mut failed, mut kept, mut missed) = (0usize, 0usize, 0usize);
            let (mut before_sum, mut after_sum) = (0u64, 0u64);

            for (i, job) in jobs.iter().enumerate() {
                if let Some(parent) = job.output.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let res = compress::compress(&job.input, &job.output, goal);
                match res {
                    Err(e) if !batch_mode => return Err(e),
                    Err(e) => {
                        failed += 1;
                        eprintln!("[{}/{total}] FAILED {}: {e:#}", i + 1, job.input.display());
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
                        let saved = 100.0 - (o.after as f64 / o.before.max(1) as f64 * 100.0);
                        if batch_mode {
                            let name = job.input.display();
                            if o.kept_original && out_dir.is_none() {
                                // Next to the original an identical copy is just
                                // clutter; with --out-dir the copy keeps the set complete.
                                let _ = std::fs::remove_file(&job.output);
                                println!(
                                    "[{}/{total}] {name}: already optimal ({}), left as is",
                                    i + 1,
                                    human(o.before)
                                );
                            } else if o.kept_original {
                                println!(
                                    "[{}/{total}] {name}: already optimal ({}), copied unchanged",
                                    i + 1,
                                    human(o.before)
                                );
                            } else {
                                println!(
                                    "[{}/{total}] {name}: {} -> {} ({saved:.0}% smaller)",
                                    i + 1,
                                    human(o.before),
                                    human(o.after)
                                );
                            }
                        } else if o.kept_original {
                            println!(
                                "Already optimal: compression would not make it smaller. Copied unchanged to {}",
                                job.output.display()
                            );
                        } else {
                            println!(
                                "{} -> {} ({saved:.0}% smaller, images at {} dpi)\nSaved to {}",
                                human(o.before),
                                human(o.after),
                                o.dpi.unwrap_or(0),
                                job.output.display()
                            );
                        }
                    }
                }
            }

            if wants_target && missed > 0 && !batch_mode {
                eprintln!("warning: could not reach the target size; this is the smallest result");
            }
            if batch_mode {
                let done = total - failed;
                let saved = 100.0 - (after_sum as f64 / before_sum.max(1) as f64 * 100.0);
                println!(
                    "\n{done}/{total} files processed: {} -> {} ({saved:.0}% smaller overall)",
                    human(before_sum),
                    human(after_sum)
                );
                if kept > 0 {
                    println!("{kept} already optimal (not made smaller)");
                }
                if missed > 0 {
                    println!("{missed} could not reach the target size (smallest result kept)");
                }
                if failed > 0 {
                    bail!("{failed} file(s) failed");
                }
            }
        }
        Cmd::Merge { inputs, output } => {
            pages::merge(&inputs, &output)?;
            println!("Merged {} files into {}", inputs.len(), output.display());
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
            for f in &files {
                println!("{}", f.display());
            }
            println!("Wrote {} files", files.len());
        }
        Cmd::Extract {
            input,
            pages: spec,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "extract"));
            let n = pages::extract(&input, &spec, &output)?;
            println!("Kept {n} pages -> {}", output.display());
        }
        Cmd::Delete {
            input,
            pages: spec,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "trimmed"));
            let n = pages::delete(&input, &spec, &output)?;
            println!("{n} pages remain -> {}", output.display());
        }
        Cmd::Reorder {
            input,
            order,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "reordered"));
            pages::reorder(&input, &order, &output)?;
            println!("Reordered -> {}", output.display());
        }
        Cmd::Rotate {
            input,
            angle,
            pages: spec,
            output,
        } => {
            let output = output.unwrap_or_else(|| sibling(&input, "rotated"));
            pages::rotate(&input, angle, spec.as_deref(), &output)?;
            println!("Rotated -> {}", output.display());
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
            println!("Wrote {} images to {}", files.len(), dir.display());
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
            println!("Created {} ({n} pages)", output.display());
        }
        Cmd::Text {
            input,
            output,
            layout,
        } => {
            let output = output.unwrap_or_else(|| input.with_extension("txt"));
            let chars = convert::to_text(&input, &output, layout)?;
            if chars == 0 {
                eprintln!("warning: no text found. This may be a scanned PDF (OCR is planned).");
            }
            println!("Wrote {chars} characters to {}", output.display());
        }
        Cmd::Protect { input, output } => {
            let output = output.unwrap_or_else(|| sibling(&input, "protected"));
            let pw = get_password("New password: ", true)?;
            security::protect(&input, &pw, &output)?;
            println!("Encrypted (AES-256) -> {}", output.display());
        }
        Cmd::Unlock { input, output } => {
            let output = output.unwrap_or_else(|| sibling(&input, "unlocked"));
            let env_pw = std::env::var("PDFSQUEEZE_PASSWORD").ok();
            match security::unlock(&input, env_pw.as_deref(), &output) {
                Ok(()) => {}
                Err(security::UnlockError::NeedsPassword) if env_pw.is_none() => {
                    let pw = get_password("Password: ", false)?;
                    match security::unlock(&input, Some(&pw), &output) {
                        Ok(()) => {}
                        Err(security::UnlockError::NeedsPassword) => bail!("wrong password"),
                        Err(security::UnlockError::Other(e)) => return Err(e),
                    }
                }
                Err(security::UnlockError::NeedsPassword) => bail!("wrong password"),
                Err(security::UnlockError::Other(e)) => return Err(e),
            }
            println!("Unlocked -> {}", output.display());
        }
        Cmd::StripMetadata { input, output } => {
            let output = output.unwrap_or_else(|| sibling(&input, "clean"));
            let r = security::strip_metadata(&input, &output)?;
            println!(
                "Info dictionary: {}\nXMP metadata:    {}\nSaved to {}",
                if r.removed_info { "removed" } else { "none found" },
                if r.removed_xmp { "removed" } else { "none found" },
                output.display()
            );
        }
        Cmd::Info { input } => {
            print!("{}", security::info(&input)?);
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
            println!("Watermarked {n} pages -> {}", output.display());
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
            println!("Numbered {n} pages -> {}", output.display());
        }
        Cmd::Doctor => {
            for (tool, pkg, needed_for) in engine::TOOLS {
                match engine::find(tool) {
                    Some(p) => println!("ok       {tool:<10} {}  ({needed_for})", p.display()),
                    None => println!("MISSING  {tool:<10} brew install {pkg}  ({needed_for})"),
                }
            }
        }
    }
    Ok(())
}
