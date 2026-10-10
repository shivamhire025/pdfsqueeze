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
| PDF to images (PNG/JPG, chosen DPI, page range) | `to-images` | Ghostscript (was Poppler) | done |
| Images to PDF (JPEG embedded losslessly, A4/Letter/fit) | `images-to-pdf` | pure Rust | done |
| PDF to text | `text` | Ghostscript `txtwrite` (was Poppler) | done |
| Password protect (AES-256) and unlock | `protect`, `unlock` | qpdf | done |
| Strip metadata (Info dict + document XMP) | `strip-metadata` | qpdf JSON | done |
| Inspect a PDF (pages, encryption, metadata) | `info` | qpdf | done |
| Watermark (text, opacity, angle) | `watermark` | pure Rust overlay + qpdf | done |
| Page numbers (position, `{n}`/`{total}` template) | `number-pages` | pure Rust overlay + qpdf | done |
| Image watermark / stamp | n/a | n/a | todo |
| Non-ASCII text in watermarks (font embedding) | n/a | n/a | todo |
| Strip per-page metadata and annotation authors | n/a | n/a | todo |

## Phase 3: Smart features (done)

| Feature | Command | Engine | Status |
|---|---|---|---|
| OCR: scanned PDF to searchable PDF (parallel, skips pages that already have text) | `ocr` | Tesseract + qpdf overlay | done |
| Explicit JPEG quality, monotonic target-size ladder | `compress --quality` | Ghostscript | done |
| Check structure | `check` | qpdf | done |
| Repair damaged PDFs (honest about lost data) | `repair` | qpdf, Ghostscript fallback | done |
| Linearize (fast web view) | `linearize` | qpdf | done |
| PDF/A-1, -2, -3 conversion (declared, not certified) | `pdfa` | Ghostscript | done |
| OCR languages beyond English bundled / downloadable | n/a | n/a | todo |
| OCR page deskew and rotation detection (needs `osd` data) | n/a | n/a | todo |
| PDF/A validation (veraPDF) | n/a | n/a | todo |
| Skip JPEG re-encoding for flat graphics / screenshots | n/a | n/a | todo |

## Phase 3.7: Agent-mode CLI (done)

| Item | Status |
|---|---|
| Global `--json`: one JSON object per command, `{ok, command, result\|error, warnings, exit_code}` | done |
| Never prompts: `--password-file` or `$PDFSQUEEZE_PASSWORD`, else `password_required` immediately | done |
| Stable exit codes (0/1/2/3) and error codes, classified centrally | done |
| `tools --json`: schemas generated from the CLI definitions | done |
| `mcp`: MCP server over stdio reusing the same commands, with a folder allow-list | done, tested with a scripted client |
| AGENTS.md: contract, client setup, recipes | done |
| Tested against a real MCP client (Claude Desktop / Claude Code) | todo |
| MCP progress notifications for long jobs (OCR) | todo |
| Packaging for `npx` / `pip` installs | todo |

## Phase 3.9: Windows and Linux (packages verified in CI)

**Dry run #5 (run 38021880189, 2026-10-09): all six packages built and passed their smoke
tests, and CI was green on every job.** Smoke test results: Windows x64 47/47 (including OCR),
Windows x86 45/45 (no OCR), Linux x86_64 and arm64 47/47 each inside a bare `ubuntu:22.04`
container, macOS arm64 and Intel 46/46. Minimum systems, read from the builds: macOS arm64 14+,
**macOS Intel 15+** (GitHub's only Intel runner is macOS 15), Linux glibc 2.35+. Windows was
only run on Windows Server 2022; older Windows versions are untested. Still never run: the
installer scripts on Linux and Windows (`install.sh` on Linux, `install.ps1`), and Authenticode
signing does not exist yet.

Neither platform can be run from the Mac this was developed on, so these were developed
against GitHub's runners. The table below keeps the history of what each run showed.

| Item | Status |
|---|---|
| Engine lookup is OS-aware: `.exe` names, `gswin64c.exe`, Windows install dirs, per-OS install hints | done, unit-tested on every platform via an `Os` parameter |
| Ghostscript resources found in Homebrew, Debian/MSYS2 (versioned) and Windows-installer layouts | done, unit-tested |
| `\\?\` path prefix stripped (Ghostscript cannot read it); `qpdf.exe` exit-code 3 handling | done |
| `GS_LIB` built with `join_paths` (`;` on Windows); static CRT for MSVC builds | done |
| `scripts/smoke.sh`: 47 end-to-end checks, runs on any OS, can require bundled engines | done, passes on macOS |
| `ci.yml`: tests on Linux/macOS/Windows, 32-bit Windows tests, end-to-end with apt engines | passed (run 38015649219 and the fixes run); now with timeouts and concurrency |
| `bundle-engines-linux.sh` (ldd + patchelf, glibc floor), `package-linux.sh` | written, unverified |
| `stage-windows.ps1`: official Ghostscript + qpdf unpacked with 7-Zip (x86) | **verified**: 44/45 smoke checks on the real 32-bit package (dry run #3); the one failure was a bug in the smoke check, fixed |
| `stage-windows.ps1` (x64) | **verified** (dry run #5): 47/47 smoke checks |
| `bundle-tesseract-windows.sh`: MSYS2 Tesseract in its own `engines/tesseract/` folder | **verified** (dry run #5): OCR passes in the Windows x64 package |
| `install.ps1` | unverified |
| `release.yml`: macOS arm64 and Intel packages built and smoke-tested with system engines out of reach | verified (dry run #1) |
| Linux x86_64 and arm64 packages: build, then pass the full smoke test inside a bare `ubuntu:22.04` container with no engines installed | **verified** (dry run #4, run 38021396870). Dry run #3 had failed 11 of 47 because 22.04's qpdf 10.6 lacks JSON v2; fixed by building a current qpdf in CI |
| `engine_too_old` guard: qpdf below 11 is rejected with a clear message | done, unit-tested and tried end to end |
| `release.yml`: macOS arm64 + Intel | **verified** three times (dry runs #1, #3, #4) |
| Windows jobs in dry run #4 | failed at the unit-test compile step: a test used `std::os::unix` behind a runtime `cfg!`; fixed with `#[cfg(unix)]`; both Windows packages then passed in dry run #5 |
| Linux packages tested inside a bare `ubuntu:22.04` container | in release.yml, unverified |
| musl / Alpine Linux | not supported (needs a static or musl build) |
| Windows 32-bit OCR | not possible with current Tesseract builds |
| Windows ARM64 | not planned yet |
| Authenticode signing via SignPath Foundation (free for open source; the repo is now public, so this can be applied for) | todo |

## Phase 3.8: Three engines only (done)

Poppler is no longer used. Ghostscript renders pages and extracts text, qpdf reads page
geometry, Tesseract does OCR. This is what makes a 32-bit Windows build possible (Poppler has
no current 32-bit Windows build) and shrinks the bundle.

| Item | Status |
|---|---|
| `to-images` via Ghostscript (`png16m`/`jpeg`, source page numbers preserved in file names) | done |
| `text` via Ghostscript `txtwrite`, per page, dedented; invisible OCR text is extracted | done |
| Page sizes via qpdf JSON with MediaBox/CropBox/Rotate inheritance (watermark, page numbers) | done |
| Bundle: 3 tools + 23 libraries (was 5 + 35), 25 MB download (was 28 MB) | done |
| Compare text quality against real-world documents (multi-column, CJK, RTL) | todo |

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
| Repo made public (CI is free, `install.sh` can download) | done 2026-10-09 |
| First tagged release | todo |
| Homebrew tap (`brew install`) for users who have brew | todo |
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
- pdfsqueeze needs qpdf 11 or newer (JSON output v2, `--update-from-json`). Packages bundle
  one; building from source against an older system qpdf fails with `engine_too_old`.
- Engines are bundled in release packages (see Phase 3.5). Building from source still needs
  Homebrew's Ghostscript, qpdf and Tesseract on the build machine.
- Bundle size is about 60 MB installed (26 MB download); it includes Tesseract libraries that
  Ghostscript links against.
- Text extraction uses Ghostscript's `txtwrite`. It preserves columns and tables on the test
  documents but has not been compared against real-world PDFs; reading order for complex
  multi-column layouts may differ from dedicated tools.
- Compress, merge, split and the page commands do not accept encrypted input yet; run
  `unlock` first.
- Watermark and page-number text is limited to printable ASCII (standard Helvetica).
- `compress` now forces JPEG for colour and grey images so quality is predictable. Flat
  graphics, screenshots and diagrams can pick up JPEG artefacts at low quality settings; the
  original is kept whenever the result would not be smaller.
- OCR: only English is bundled (others need `--tessdata`). There is no deskew or automatic
  rotation, so skewed or sideways scans recognise poorly. OCR adds a text layer but does not
  clean up the image.
- `pdfa` output declares the PDF/A part and conformance B and embeds fonts and an sRGB output
  intent, but has not been run through a validator such as veraPDF.
- `repair` cannot bring back data that is missing from the file (for example an interrupted
  download); it reports that instead of claiming success.
- `strip-metadata` removes the document Info dictionary and document-level XMP stream. It does
  not touch per-page metadata, annotation authors, or embedded files.
- Images with EXIF rotation are placed as stored (orientation tag is ignored), and CMYK JPEGs
  are decoded to RGB rather than embedded as-is.
- Passwords are read from a hidden prompt or `$PDFSQUEEZE_PASSWORD`, never from a flag, and are
  passed to qpdf over stdin so they do not appear in the process list. They may not start or
  end with whitespace.
