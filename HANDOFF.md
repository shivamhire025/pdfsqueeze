# pdfsqueeze: handoff

**Read this first when picking the project up in a new chat or after context was lost.**
Everything here was checked against the repo and CI on the date below; where something was
only written and never run, it says so.

- **Last updated:** 2026-10-10.
- **Git state:** at the time of writing `origin/main` was `4eef85d` and everything after it was
  local only (the verified-status docs, the desktop window and redesigned UI, and this file).
  **Check the real state with `git log --oneline origin/main..HEAD`** rather than trusting this
  line. Pushing triggers CI, which needs the owner's go-ahead.
- **Repo:** https://github.com/shivamhire025/pdfsqueeze (public, AGPL-3.0)
- **Local path:** `~/Projects/pdfsqueeze/` (the Obsidian vault only holds notes about it:
  `Wiki/Projects/pdfsqueeze.md` and `Wiki/log.md`)
- **Update this file** whenever you finish a piece of work (status table, backlog, date, commit).

## What it is and what the owner asked for

An offline, open-source alternative to upload-based PDF websites (iLovePDF, Smallpdf). Files
never leave the machine. Written in Rust; it drives three engines (Ghostscript, qpdf,
Tesseract) and ships them bundled.

The owner (Shivam, GitHub `shivamhire025`) stated these requirements:

1. Free and open source. **Never pay for anything**: no Apple Developer account, no code
   signing certificates, no paid CI, no payment methods.
2. Downloads for: **Mac** (Apple Silicon and Intel), **Windows** (64 and 32-bit), and a
   **CLI for AI agents**. Linux was added with the owner's OK because agents run there.
3. A **real application window**, never a browser tab, and a **genuinely good design**. A first
   version that opened in the browser was rejected as "one of the ugliest UIs I have seen".
4. No surprises: say what a job consumes and ask before remote actions (see "Working rules").

## Status at a glance

| Area | State | Evidence |
|---|---|---|
| 22 commands (compress, merge, split, extract, delete, reorder, rotate, to-images, images-to-pdf, text, ocr, protect, unlock, strip-metadata, info, watermark, number-pages, repair, check, linearize, pdfa, doctor) | built | 49 unit tests, `scripts/smoke.sh` 46-47 checks |
| Agent mode: `--json` envelope, exit/error codes, `tools`, MCP server | built | tested with a scripted JSON-RPC client; **not** with Claude Desktop or Claude Code themselves |
| Packages: macOS arm64 (needs macOS 14+), macOS Intel (macOS 15+), Linux x86_64 + arm64 (glibc 2.35+), Windows x64 (with OCR), Windows x86 (no OCR) | **all six built and smoke-tested in CI** | release dry run `38021880189` |
| Desktop window (`pdfsqueeze ui`) | built, **verified on macOS only** | window list, menu bar, Quit, page loaded; see below |
| Windows window (WebView2) | written, **never compiled or run** | first CI build with `wry`/`tao` will be its first test |
| Native drag-and-drop of files from Finder/Explorer | written, **not tested** | cannot drive a native drag from the dev environment |
| `install.sh` on Linux, `install.ps1` on Windows | written, **never run** | the macOS path of `install.sh` was run |
| Published release / version tag | **none** | the one-line install commands do not work until `v0.1.0` exists |
| Code signing | none | macOS is ad-hoc signed; Windows unsigned (SmartScreen warning if downloaded in a browser) |

The detailed per-item tables are in `ROADMAP.md` (kept current; it is the source of truth for
"verified vs written").

## Resume in five minutes

```bash
cd ~/Projects/pdfsqueeze
git status -sb && git log --oneline | head          # where are we
cargo test                                          # 49 tests, no engines needed
cargo build --release
./scripts/smoke.sh target/release/pdfsqueeze        # end to end (needs gs, qpdf, tesseract installed)
./target/release/pdfsqueeze doctor                  # which engines it found
./target/release/pdfsqueeze ui                      # the app window
```

Build-time engines on a Mac: `brew install ghostscript qpdf tesseract` (qpdf must be 11+).
Toolchain: Rust 1.99 from Homebrew (no rustup, so no cross-compile targets locally; Windows and
Linux are only testable in CI).

To test a package the way a user gets it (engines out of reach):

```bash
./scripts/package-macos.sh      # -> dist/pdfsqueeze-macos-arm64.tar.gz (about 26 MB)
# then run scripts/smoke.sh against the unpacked copy under `sandbox-exec` denying /opt/homebrew
# with an empty environment (see ROADMAP "Phase 3.5")
```

## Where things are

| Path | What it is |
|---|---|
| `src/main.rs` | Slim dispatcher: parse, then `ui`, `mcp` or `commands::execute` |
| `src/cli.rs` | All clap definitions (global `--json`). The single source for CLI, schemas and MCP |
| `src/commands.rs` | Every command returns a `Done` (human text, JSON, warnings, exit code). Error classification, JSON envelope |
| `src/agent.rs` | Tool schemas generated from clap, MCP stdio server, folder allow-list (`PathPolicy`) |
| `src/ui.rs`, `src/ui.html` | The app: local server (loopback, token, Host check), window module (wry + tao), macOS menu (muda), the whole page |
| `src/engine.rs` | Finds and runs engines: bundled first, then PATH; `Os` parameter for testable Windows/Linux logic; qpdf version guard (needs 11+) |
| `src/compress.rs`, `src/batch.rs` | Ghostscript compression (dpi + JPEG quality, monotonic target-size ladder); file/folder jobs |
| `src/pages.rs`, `src/security.rs`, `src/stamp.rs`, `src/fix.rs`, `src/convert.rs`, `src/ocr.rs`, `src/imgpdf.rs`, `src/pdfout.rs` | Page ops and page geometry, passwords/metadata/info, watermark and numbering, check/repair/linearize/PDF-A, render and text via Ghostscript, OCR overlay, images to PDF, minimal PDF writer |
| `scripts/smoke.sh` | 47 end-to-end checks; runs on any OS; `SMOKE_REQUIRE_BUNDLED=1`, `SMOKE_SKIP_OCR=1` |
| `scripts/bundle-engines.sh`, `scripts/package-macos.sh` | macOS bundling (dylib relocation, ad-hoc signing) |
| `scripts/bundle-engines-linux.sh`, `scripts/package-linux.sh` | Linux bundling (ldd + patchelf; glibc floor recorded) |
| `scripts/stage-windows.ps1`, `scripts/bundle-tesseract-windows.sh` | Windows: official Ghostscript + qpdf unpacked with 7-Zip; MSYS2 only for Tesseract |
| `install.sh`, `install.ps1` | One-line installers (no admin, no password) |
| `.github/workflows/ci.yml`, `release.yml` | CI on every push; release builds by hand or on a `v*` tag |
| `README.md`, `AGENTS.md`, `ROADMAP.md`, `CLAUDE.md` | Users, agents that call the tool, status tables, rules for working in the repo |

## Decisions and why

- **Bundle engines, do not rewrite them.** Pure-Rust replacements would be months of work.
- **Three engines only.** Poppler was dropped: no current 32-bit Windows build, and Ghostscript
  can render and extract text. Ghostscript (AGPL) forces the project to be AGPL-3.0.
- **No Tauri CLI.** The window uses `wry` + `tao` (what Tauri is built on) inside the one
  binary; the page is a single embedded HTML file served from loopback.
- **Windows engines are official builds, unpacked.** MSYS2's Ghostscript has no `Resource`
  folder (its init files are compiled in), and running the installer on a runner hung a job for
  an hour. Tesseract comes from MSYS2 (no official portable build) in its own `engines/tesseract/`.
- **Windows 32-bit ships without OCR.** Tesseract has had no 32-bit build since 5.3.1.
- **Passwords never travel on a command line.** Hidden prompt, `--password-file`, or
  `PDFSQUEEZE_PASSWORD`; they reach qpdf through stdin (`qpdf @-`).
- **Agents get JSON and stable codes**, tool schemas are generated from the CLI so they cannot
  drift, and the MCP server refuses to start without `--allow-dir` or `--allow-any-path`.
- **Public repo** so CI is free and unlimited. Billing was checked read-only: Free plan, no
  payment method, Actions budget $0 with "stop usage".

## Backlog, in the order I would do it

**Next, needs the owner's go-ahead**
1. Push the local commits (triggers `ci.yml`, free). This is the first compile of the
   Windows window code; expect to fix something there.
2. Owner reviews the real window (`pdfsqueeze ui`) and gives design feedback.
3. Test `install.sh` (Linux) and `install.ps1` (Windows) in CI, then tag **`v0.1.0`**, which
   publishes the six packages and makes the one-line install commands work.

**Soon**
- Double-click launch: no arguments opens the window; Windows needs the GUI subsystem or
  `FreeConsole` so no terminal appears. App icon, `.app` bundle, Windows installer.
- Native open and Save-As dialogs; reorder documents for merge; OCR progress in the window.
- Verify native file drop and keyboard shortcuts on macOS and Windows with a person.
- Test with real documents (scans, very large files, encrypted input, CJK).
- Real MCP client test (Claude Desktop, Claude Code).
- Free Windows code signing: apply to SignPath Foundation (repo is public and AGPL).
- Linux window (needs webkit2gtk; today it opens the browser).

**Later**
- OCR languages beyond English; deskew and rotation detection; PDF/A validation with veraPDF.
- Integration tests with fixture PDFs; batch for commands other than `compress`; parallel batch.
- SPDX headers, CONTRIBUTING; Phase 5 items (Office conversion, redaction, forms, signing, compare).

## Gotchas learned the hard way

- **qpdf must be 11 or newer** (JSON v2, `--update-from-json`). Ubuntu 22.04 ships 10.6, so the
  Linux job builds a current qpdf from source. `engine::qpdf()` rejects older ones (`engine_too_old`).
- **`cfg!()` keeps the code, `#[cfg]` removes it.** A `std::os::unix` use behind `if cfg!(unix)`
  still has to compile on Windows. Use `#[cfg(unix)]`.
- **Packages inherit the build OS floor.** The Intel package needs macOS 15 because GitHub's only
  Intel runner is macOS 15; Linux needs the build runner's glibc.
- **Never run a GUI or silent installer on a runner.** Unpack with 7-Zip.
- **Every workflow job needs `timeout-minutes`**; workflows use `concurrency`.
- **`wry`/`tao` create no menu.** Without `muda`'s menu there is no Cmd+Q, Cmd+C or Cmd+V on
  macOS. A drag-drop handler that returns `true` swallows HTML5 drops (we want that: real paths).
- **A native window cannot be screenshotted** without Screen Recording permission. Evidence
  used instead: `CGWindowListCopyWindowInfo` (a 10-line Swift helper), System Events for the
  menu bar and Quit, and `lsof` for the web view's connection to the local server.
- **Ghostscript `-dSAFER` blocks file reads**; use `-dNOSAFER` only for test generators.
- **The UI server never trusts paths from the page**: it strips path-like arguments and builds
  them from registered files and its own results folder.
- Shell notes for this machine: the default shell is zsh (no word splitting of unquoted
  variables), macOS bash is 3.2, and there is no `timeout` command.

## Working rules for this project

Standing rules are in the vault `CLAUDE.md` ("Standing rules", 2026-10-09) and this repo's
`CLAUDE.md`. In short: say what a job consumes and wait for a yes; never spend or enable
spending; every long job gets a timeout and is watched; check account facts before warning;
remote side effects (push, trigger workflows, tag, publish, change repo settings) are confirmed
each time; verify locally first and run remote jobs once; log notable runs and failures in the
vault's `Wiki/log.md` the same session. Be explicit about what was run versus only written.

## Cost and account facts (checked 2026-10-09)

GitHub Free plan, no payment method, Actions budget $0 with "stop usage". The repo is public,
so standard runner minutes are free. Your own machine: Docker is installed but its daemon was
not running; Rust is from Homebrew. The owner's commit email is public in history.

## Open questions for the owner

- OK to push the local commits now (runs CI)?
- Tag `v0.1.0` and publish once the Windows window compiles and the installers are tested?
- Happy with the new design? What would they change?
- Apply for SignPath Foundation signing, or ship Windows unsigned for now?
