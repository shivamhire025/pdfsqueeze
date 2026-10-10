# pdfsqueeze

An offline, open-source PDF toolkit. Compress, merge, split and rearrange PDFs
entirely on your own machine. Nothing is uploaded anywhere.

> Working name. Early stage: CLI first, desktop app later. See [ROADMAP.md](ROADMAP.md).

## For AI agents

pdfsqueeze can be called by agents either as an MCP server or as a CLI with `--json`:

```bash
pdfsqueeze mcp --allow-dir ~/Documents          # MCP server over stdio
pdfsqueeze --json compress report.pdf           # one JSON object, never prompts
pdfsqueeze tools --json                         # every command with a JSON Schema
```

Stable exit codes (0 ok, 1 failure, 2 usage, 3 completed with problems) and error codes.
See [AGENTS.md](AGENTS.md) for the full contract, MCP client configuration and recipes.

## Install (nothing else needs to be installed)

| Platform | Command | Status |
|---|---|---|
| macOS, Apple Silicon | `curl -fsSL https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.sh \| sh` | built and smoke-tested in CI; needs macOS 14+ |
| macOS, Intel | same command | built and smoke-tested in CI; needs macOS 15+ |
| Linux x86_64 / arm64 | same command | built and smoke-tested in CI in a bare container; needs glibc 2.35+ (Ubuntu 22.04+, Debian 12+) |
| Windows 64-bit | `irm https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.ps1 \| iex` | built and smoke-tested in CI, including OCR |
| Windows 32-bit | same command | built and smoke-tested in CI; no OCR (Tesseract has no 32-bit build) |

Each package already contains everything (Ghostscript, qpdf and Tesseract are bundled). The
installers need no password or administrator rights: they install under your home folder
(`~/.local/share/pdfsqueeze`, or `%LOCALAPPDATA%\pdfsqueeze` on Windows) and add the command to
your PATH. Check with `pdfsqueeze doctor`.

> Releases are not published yet, so the commands above will not work until the first tagged
> release exists. See "Releases" below. Until then, build from source (next section).

### Security warnings

pdfsqueeze is not signed with a paid certificate (it is free, open-source software).

- **macOS**: you normally will not see a warning: Gatekeeper only blocks files that carry a
  download-quarantine flag, and `curl` does not set one. If you download the archive in a
  browser, clear the flag once: `xattr -dr com.apple.quarantine ~/Downloads/pdfsqueeze`.
- **Windows**: `install.ps1` clears the "downloaded from the internet" mark. If you download the
  zip in a browser instead, Windows SmartScreen may say "Windows protected your PC": choose
  **More info, then Run anyway**, or right-click the zip, Properties, tick **Unblock** before
  extracting. Free code signing for open-source projects (SignPath Foundation) is planned.
- **Linux**: no warnings.

## Build from source

Requires Rust and, on the build machine only, the three engines (Ghostscript, qpdf, Tesseract):

```bash
brew install rust ghostscript qpdf tesseract   # qpdf must be 11 or newer
cargo build --release
./target/release/pdfsqueeze doctor     # uses the Homebrew engines it finds
```

To produce the self-contained package yourself:

```bash
./scripts/package-macos.sh             # -> dist/pdfsqueeze-macos-<arch>.tar.gz
```

## Releases and OS versions

A package only runs on the OS version it was built on, or newer: Homebrew compiles its
libraries for the macOS it runs on, and Linux packages need at least the build machine's glibc.
Official releases are built by GitHub Actions on older runners (`.github/workflows/release.yml`,
written but never run) to support older systems. Each package records its floor
(`engines/MIN_MACOS` or `MIN_GLIBC`) and the installer refuses cleanly on a machine that is too
old. Every release job smoke-tests its own package (`scripts/smoke.sh`, 47 checks) with the
system's engines out of reach before it is published.

Build a package on your own machine with `scripts/package-macos.sh`,
`scripts/package-linux.sh`, or on Windows `scripts/stage-windows.ps1` (after
`cargo build --release`) followed by `scripts/bundle-tesseract-windows.sh` in an MSYS2 MINGW64 shell.

## Usage

```bash
# compress
pdfsqueeze compress report.pdf                       # recommended preset
pdfsqueeze compress report.pdf --level extreme
pdfsqueeze compress report.pdf --dpi 110 --quality 60   # explicit image resolution and JPEG quality
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

# scans, repair and archiving
pdfsqueeze ocr scan.pdf                              # searchable PDF (English bundled)
pdfsqueeze ocr scan.pdf --lang deu --tessdata ~/tessdata
pdfsqueeze check report.pdf
pdfsqueeze repair broken.pdf
pdfsqueeze linearize report.pdf                      # fast web view
pdfsqueeze pdfa report.pdf --part 2                  # archival; not certified

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
