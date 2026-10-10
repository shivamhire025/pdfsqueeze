//! Command-line definitions. Kept separate so the same definitions drive the
//! CLI, the `tools` schema and the MCP server.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, ValueEnum)]
pub enum ImgFmtArg {
    Png,
    Jpg,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PageSizeArg {
    /// Page matches the image size
    Fit,
    A4,
    Letter,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PositionArg {
    BottomCenter,
    BottomLeft,
    BottomRight,
    TopCenter,
    TopLeft,
    TopRight,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum LevelArg {
    /// Light compression, best quality
    Low,
    /// Balanced; good for email and sharing
    Recommended,
    /// Smallest file, screen quality
    Extreme,
}

#[derive(Parser)]
#[command(
    name = "pdfsqueeze",
    version,
    about = "Offline PDF toolkit. Your files never leave this machine."
)]
pub struct Cli {
    /// Print one machine-readable JSON object instead of text. Never prompts.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
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
        #[arg(short, long, value_enum, conflicts_with = "target_size")]
        level: Option<LevelArg>,
        /// Downsample images to this resolution (overrides the preset's)
        #[arg(long, conflicts_with = "target_size")]
        dpi: Option<u32>,
        /// JPEG quality 1-100 for images (overrides the preset's)
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=100), conflicts_with = "target_size")]
        quality: Option<u8>,
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
    /// Encrypt with a password (AES-256). The password comes from --password-file,
    /// $PDFSQUEEZE_PASSWORD, or a hidden prompt when run interactively.
    Protect {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// File containing the password (one line)
        #[arg(long)]
        password_file: Option<PathBuf>,
    },
    /// Remove password protection. The password comes from --password-file,
    /// $PDFSQUEEZE_PASSWORD, or a hidden prompt when run interactively.
    Unlock {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// File containing the password (one line)
        #[arg(long)]
        password_file: Option<PathBuf>,
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
    /// Make a scanned PDF searchable (adds an invisible text layer)
    Ocr {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Language code(s), e.g. eng or eng+deu
        #[arg(long, default_value = "eng")]
        lang: String,
        /// Resolution pages are recognised at
        #[arg(long, default_value_t = 300)]
        dpi: u32,
        /// Pages processed at once (default: number of CPU cores)
        #[arg(long)]
        jobs: Option<usize>,
        /// Recognise every page, even ones that already have text
        #[arg(long)]
        all: bool,
        /// Folder holding extra <lang>.traineddata files
        #[arg(long)]
        tessdata: Option<PathBuf>,
    },
    /// Rebuild a damaged PDF
    Repair {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check a PDF's structure for problems
    Check { input: PathBuf },
    /// Optimise for fast web viewing (lossless)
    Linearize {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Convert to PDF/A for archiving (best effort, not certified)
    Pdfa {
        input: PathBuf,
        /// PDF/A part: 1, 2 or 3
        #[arg(long, default_value_t = 2)]
        part: u8,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check which engines are installed
    Doctor,
    /// Open a point-and-click interface in your browser (runs locally, nothing is uploaded)
    Ui {
        /// Port to listen on (default: any free port)
        #[arg(long)]
        port: Option<u16>,
        /// Only run the local server and print its address (no window)
        #[arg(long)]
        no_open: bool,
        /// Use your web browser instead of an application window
        #[arg(long)]
        browser: bool,
    },
    /// Describe every command as machine-readable tool schemas (for AI agents)
    Tools,
    /// Run as an MCP server over stdio so AI agents can call these commands as tools
    Mcp {
        /// Folder the tools may read and write (repeatable). Required unless
        /// --allow-any-path is given.
        #[arg(long = "allow-dir")]
        allow_dir: Vec<PathBuf>,
        /// Let tools touch any path the user can. Only use where the agent is trusted.
        #[arg(long)]
        allow_any_path: bool,
    },
}

