# Roadmap

Goal: a fully offline, open-source alternative to iLovePDF/Smallpdf-style sites.
Files never leave the user's machine. Every feature is available from the CLI first,
then surfaced in a desktop app.

Priority is by how often people actually need the feature, so Phase 1 covers most
real-world use.

## Phase 1: Core toolkit (CLI)  (done)

| Feature | Command | Engine | Status |
|---|---|---|---|
| Compress with presets | `compress --level` | Ghostscript | done |
| Compress to custom DPI | `compress --dpi` | Ghostscript | done |
| Compress to a target size | `compress --target-size 2MB` | Ghostscript | done (see limitations) |
| Merge | `merge a.pdf b.pdf -o out.pdf` | qpdf | done |
| Split every N / by ranges | `split --every / --ranges` | qpdf | done |
| Extract / delete / reorder pages | `extract`, `delete`, `reorder` | qpdf | done |
| Rotate (all or selected pages) | `rotate` | qpdf | done |
| Engine health check | `doctor` | n/a | done |
| Batch: files and folders at once (recursive, `--out-dir`, per-file report) | `compress dir/ -r` | n/a | done |
| Integration tests with fixture PDFs | `cargo test` | n/a | todo |
| Batch for the other commands (to-images, strip-metadata, watermark, ...) | n/a | n/a | todo |
| Parallel batch processing | n/a | n/a | todo |

## Phase 2: Conversion and privacy (done)

| Feature | Command | Engine | Status |
|---|---|---|---|
| PDF to images (PNG/JPG, chosen DPI, page range) | `to-images` | Poppler `pdftoppm` | done |
| Images to PDF (JPEG embedded losslessly, A4/Letter/fit) | `images-to-pdf` | pure Rust | done |
| PDF to text | `text` | Poppler `pdftotext` | done |
| Password protect (AES-256) and unlock | `protect`, `unlock` | qpdf | done |
| Strip metadata (Info dict + document XMP) | `strip-metadata` | qpdf JSON | done |
| Inspect a PDF (pages, encryption, metadata) | `info` | qpdf | done |
| Watermark (text, opacity, angle) | `watermark` | pure Rust overlay + qpdf | done |
| Page numbers (position, `{n}`/`{total}` template) | `number-pages` | pure Rust overlay + qpdf | done |
| Image watermark / stamp | n/a | n/a | todo |
| Non-ASCII text in watermarks (font embedding) | n/a | n/a | todo |
| Strip per-page metadata and annotation authors | n/a | n/a | todo |

## Phase 3: Smart features

| Feature | Engine | Status |
|---|---|---|
| OCR: scanned PDF to searchable PDF | Tesseract | todo |
| Repair corrupted PDFs | qpdf / MuPDF | todo |
| Linearize (fast web view) | qpdf | todo |
| PDF/A conversion | Ghostscript | todo |
| Predictable JPEG quality control in compress | Ghostscript | todo |

## Phase 3.5: Distribution without Homebrew  (built, partly verified)

| Item | Status |
|---|---|
| `scripts/bundle-engines.sh`: copy engines + all libraries, make paths relative, ad-hoc sign, record min macOS | done, verified on this Mac |
| Engine lookup prefers bundled engines; sets `GS_LIB` and fontconfig for them | done |
| Tested with Homebrew blocked by a macOS sandbox and an empty environment (all commands, non-embedded fonts) | done |
| `scripts/package-macos.sh`: tarball + sha256 + licences + AGPL source notice | done |
| `install.sh`: no-password user install, quarantine-safe, refuses too-old macOS | done, tested with a local tarball |
| GitHub Actions release for arm64 + Intel on older runners | written, **not run** |
| Intel (x86_64) build | **untested** (needs the CI runner) |
| macOS older than the CI runner (e.g. 11-13) | not supported; needs building every dependency from source |
| First tagged release + making the repo public so `install.sh` works | todo |
| Homebrew tap (`brew install`) for users who have brew | todo |
| Poppler-data (CJK text encodings) bundled | todo |
| Windows and Linux packages | todo |

## Phase 4: Desktop app

- Tauri shell (Rust backend reuses the CLI as a library)
- Drag-and-drop, batch queue, progress
- Before/after preview with a quality slider
- Watch folder: auto-compress anything dropped in
- Finder / Explorer right-click integration
- Bundle engines as sidecars so users install nothing extra

## Phase 5: Advanced

- Office to PDF and back (LibreOffice headless)
- True redaction (MuPDF)
- Form fill and flatten
- Visual and digital signatures
- Compare two PDFs
- Pipelines: chain steps ("strip metadata, compress, watermark")

## Phase 6: Release

- LICENSE file: AGPL-3.0 added (required while Ghostscript is linked or bundled). Still to do: SPDX headers, a CONTRIBUTING note
- CI builds and installers for macOS, Windows, Linux
- Optional: replace Ghostscript with a pure-Rust image recompression path for a
  permissive license and smaller binary

## Known limitations

- `--target-size` assumes lower DPI means smaller files, which does not always hold
  because Ghostscript chooses lossless vs JPEG per image automatically. Results are
  always valid and under the target when reachable, but not always the highest quality
  that fits. Phase 3 adds explicit JPEG quality control.
- Engines are bundled in release packages (see Phase 3.5). Building from source still needs
  Homebrew's Ghostscript, qpdf and Poppler on the build machine.
- Bundle size is about 60 MB installed (26 MB download); it includes Tesseract libraries that
  Ghostscript links against.
- CJK and other special text encodings in PDFs may not render or extract, because Poppler's
  data package is not bundled yet.
- Compress, merge, split and the page commands do not accept encrypted input yet; run
  `unlock` first.
- Watermark and page-number text is limited to printable ASCII (standard Helvetica).
- `strip-metadata` removes the document Info dictionary and document-level XMP stream. It does
  not touch per-page metadata, annotation authors, or embedded files.
- Images with EXIF rotation are placed as stored (orientation tag is ignored), and CMYK JPEGs
  are decoded to RGB rather than embedded as-is.
- Passwords are read from a hidden prompt or `$PDFSQUEEZE_PASSWORD`, never from a flag, and are
  passed to qpdf over stdin so they do not appear in the process list. They may not start or
  end with whitespace.
