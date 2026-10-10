# pdfsqueeze

An offline, open-source PDF toolkit. Compress, merge, split and rearrange PDFs
entirely on your own machine. Nothing is uploaded anywhere.

> Working name. Early stage: CLI first, desktop app later. See [ROADMAP.md](ROADMAP.md).

## Install (macOS, no Homebrew or anything else needed)

```bash
curl -fsSL https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.sh | sh
```

This downloads one package that already contains everything (Ghostscript, qpdf and Poppler
are bundled), installs it under `~/.local/share/pdfsqueeze` and links `~/.local/bin/pdfsqueeze`.
It needs no password and writes nothing outside your home folder. Works on Apple Silicon and
Intel Macs. Check it with `pdfsqueeze doctor`.

> Releases are not published yet, so the command above will not work until the first tagged
> release exists (the repo is private for now). See "Releases" below.

### "Unidentified developer" warnings

pdfsqueeze is not signed with a paid Apple Developer certificate (it is free, open-source
software). You normally will not see a warning: macOS only blocks files that carry a download
quarantine flag, and `curl` does not set one. If you download the `.tar.gz` in a browser instead,
macOS may refuse to run it; clear the flag once:

```bash
xattr -dr com.apple.quarantine ~/Downloads/pdfsqueeze
```

The binaries are ad-hoc signed (free), which is all Apple Silicon requires to run them.

## Build from source

Requires Rust and, on the build machine only, Homebrew engines:

```bash
brew install rust ghostscript qpdf poppler
cargo build --release
./target/release/pdfsqueeze doctor     # uses the Homebrew engines it finds
```

To produce the self-contained package yourself:

```bash
./scripts/package-macos.sh             # -> dist/pdfsqueeze-macos-<arch>.tar.gz
```

## Releases and macOS versions

A package built on your own Mac only runs on that macOS version or newer, because Homebrew
compiles its libraries for the OS it runs on (a package built on macOS 27 needs macOS 27+).
Official releases are built by GitHub Actions on older runners (`.github/workflows/release.yml`,
written but not yet run) to support older Macs. Each package records its floor in
`engines/MIN_MACOS`, and `install.sh` refuses cleanly on a Mac that is too old.

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
