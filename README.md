# pdfsqueeze

An offline, open-source PDF toolkit. Compress, merge, split and rearrange PDFs
entirely on your own machine. Nothing is uploaded anywhere.

> Working name. Early stage: CLI first, desktop app later. See [ROADMAP.md](ROADMAP.md).

## Requirements

- Rust (`brew install rust`)
- Engines: `brew install ghostscript qpdf poppler tesseract`

Run `pdfsqueeze doctor` to see what is installed.

## Build

```bash
cargo build --release
./target/release/pdfsqueeze --help
```

## Usage

```bash
# compress
pdfsqueeze compress report.pdf                       # recommended preset
pdfsqueeze compress report.pdf --level extreme
pdfsqueeze compress report.pdf --dpi 110
pdfsqueeze compress report.pdf --target-size 2MB     # highest quality that fits

# batch: several files and/or folders in one go
pdfsqueeze compress inbox/                           # writes <name>-compressed.pdf next to each
pdfsqueeze compress inbox/ -r --out-dir smaller/     # include subfolders, keep structure and names
pdfsqueeze compress a.pdf b.pdf --level extreme --out-dir smaller/

# pages
pdfsqueeze merge a.pdf b.pdf c.pdf -o all.pdf
pdfsqueeze split book.pdf --every 10
pdfsqueeze split book.pdf --ranges 1-3,4-9
pdfsqueeze extract book.pdf 1-3,z -o excerpt.pdf     # z = last page
pdfsqueeze delete book.pdf 2,5-7
pdfsqueeze reorder book.pdf 3,1,2
pdfsqueeze rotate scan.pdf 90 --pages 1-2

# convert
pdfsqueeze to-images report.pdf --dpi 150 --format jpg
pdfsqueeze images-to-pdf a.jpg b.png -o album.pdf --page a4
pdfsqueeze text report.pdf --layout

# privacy and security
pdfsqueeze protect report.pdf          # asks for a password (hidden)
pdfsqueeze unlock report-protected.pdf
pdfsqueeze strip-metadata report.pdf
pdfsqueeze info report.pdf

# stamping
pdfsqueeze watermark report.pdf "CONFIDENTIAL" --opacity 0.2
pdfsqueeze number-pages report.pdf --format "Page {n} of {total}" --position bottom-right
```

Passwords are never taken from flags (they would be visible to other processes); use the
prompt or the `PDFSQUEEZE_PASSWORD` environment variable.

Originals are never modified. If compression would not make a file smaller, the original is
copied unchanged (for a single file or with `--out-dir`) or simply left alone (batch mode
writing next to the originals). In batch mode a failing file is reported and the rest continue;
the exit code is 1 if anything failed. Hidden files and earlier `-compressed` results are skipped.

## License

AGPL-3.0-or-later, see [LICENSE](LICENSE). Ghostscript is AGPL, so this project is too.
