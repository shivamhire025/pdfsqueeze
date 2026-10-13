# pdfsqueeze: handoff

**Read this first when picking the project up in a new chat or after context was lost.**
Everything here was checked against the repo, CI and the machine on the date below. Where
something was only written and never run, it says so.

- **Last updated:** 2026-10-13 (end of day).
- **Git state:** do not trust a number written here. Run `git status -sb` and
  `git log --oneline origin/main..HEAD`. On 2026-10-13 the tree was clean with **42 commits, 2 not
  pushed**: `19a945c` (sentence-case headline) and `6053d1b` (double-click launch and the Mac app).
- **Repo:** https://github.com/shivamhire025/pdfsqueeze (public, AGPL-3.0)
- **Live landing page:** https://shivamhire025.github.io/pdfsqueeze/ (GitHub Pages, branch `main`,
  folder `/docs`). It shows the last *pushed* version.
- **Local path:** `~/Projects/pdfsqueeze/`
- **Private record (owner's Obsidian vault, not in this repo):**
  `Wiki/Projects/pdfsqueeze - Log.md` (status snapshot, decisions, commit-by-commit timeline,
  verification ledger) and `Wiki/Projects/pdfsqueeze - Session history.md` (how the owner likes to work).
- **Public record:** `CHANGELOG.md`, `CREDITS.md`, `ROADMAP.md` (verified-versus-written tables).
- **Keep this file current:** update status, backlog, date and commit when you finish work.

## What it is and what the owner asked for

An offline, open-source alternative to upload-based PDF websites (iLovePDF, Smallpdf). Files never
leave the machine. Written in Rust; it drives three engines (Ghostscript, qpdf, Tesseract) and
ships them bundled. One binary is a command line, an app window and an MCP server.

The owner (GitHub `shivamhire025`) stated these requirements:

1. Free and open source. **Never pay for anything**: no Apple Developer account, no code-signing
   certificates, no paid CI, no payment methods.
2. Downloads for **Mac** (Apple Silicon and Intel), **Windows** (64 and 32-bit) and a **CLI for AI
   agents**. Linux was added with the owner's OK because agents run there.
3. A **real application window**, never a browser tab, with a genuinely good design. Direction so
   far: light theme, nature green, flat and quiet, icons over text, one white card for the work,
   real thumbnails everywhere, drag to reorder, one "Save all" for many results.
4. No surprises: say what a job consumes and ask before any remote action.
5. A landing page: dark forest-green editorial style modelled on getartcraft.com/apps/photocraft,
   hero copy "Stop uploading your PDFs to the internet".

## Status at a glance

| Area | State | Evidence |
|---|---|---|
| 23 commands: compress, organize, merge, split, extract, delete, reorder, rotate, sign, to-images, images-to-pdf, text, ocr, protect, unlock, strip-metadata, info, watermark, number-pages, repair, check, linearize, pdfa, doctor | built | 53 unit tests and `scripts/smoke.sh` 53 checks on macOS; CI on macOS, Windows, Linux |
| Agent mode: `--json`, exit and error codes, `tools`, MCP server | built | scripted JSON-RPC client; **not** a real MCP client |
| Window UI: organize pages (select, turn, remove, drag), merge cards (thumbnails, drag, remove, add), sign (draw, type or image, place on a page), real progress bar, Save all, splash, tool groups | built | driven in a browser tab with scripted pointer events and measured outputs; macOS window launched and inspected. **Never driven by a hand.** |
| macOS app `pdfsqueeze.app` (zipped) | built locally | unzipped copy passes smoke 53/53 with Homebrew hidden, launches with `open`, 1180x832 window, menu bar, clean quit. **Not built by CI yet; Intel not built; not run from a quarantined download.** |
| Double-click launch (no args from the desktop opens the window) | built | unit-tested; works for the Mac app. **Windows and Linux: compiled in CI only.** |
| Six packages (mac arm64/Intel, Linux x86_64/arm64, Windows x64/x86) | built and smoke-tested in CI **on 2026-10-10** (run `38021880189`) | **Predates the window, signing and the app. Must be re-run.** |
| Windows window (WebView2) | compiles in CI (`38480625744`) | **never run by a person** |
| Native file drag from Finder or Explorer onto the window | written | not tested |
| `install.sh` (Linux), `install.ps1` (Windows) | written | **never run** (the macOS path of `install.sh` was run) |
| Published release / tag | **none** | installers and download buttons do not work until `v0.1.0` exists |
| Code signing | none | Mac ad-hoc only; Windows unsigned (SmartScreen warning). Free option: SignPath Foundation, not applied for |
| Landing page | live | desktop, wide and phone widths checked in a browser tab; download buttons point at the Releases page (`RELEASE_READY=false`) |
| Certificate (digital) signatures | **not built** | `sign` places a picture of a signature only |

## Resume in five minutes

```bash
cd ~/Projects/pdfsqueeze
git status -sb && git log --oneline origin/main..HEAD   # where are we, what is unpushed
cargo test                                              # 53 tests, no engines needed
cargo build --release
./scripts/smoke.sh target/release/pdfsqueeze            # needs gs, qpdf 11+, tesseract installed
./target/release/pdfsqueeze doctor                      # which engines it found
./target/release/pdfsqueeze ui                          # the app window
bash scripts/package-macos-app.sh                       # -> dist/pdfsqueeze-macos-<arch>.app.zip
```

Build-time engines on a Mac: `brew install ghostscript qpdf tesseract` (qpdf must be 11+).
Toolchain: Rust 1.99 from Homebrew (no rustup, so no cross-compile targets: Windows and Linux are
only testable in CI). Python 3 with Pillow and numpy is used by `scripts/make-hero.py`.

Test the window without a person: `pdfsqueeze ui --no-open --port 47399` prints a URL with a token.
Upload files with `curl -X POST -H "X-Token: <t>" --data-binary @file.pdf
"http://127.0.0.1:47399/api/upload?name=file.pdf"`, open the URL in a browser pane and drive it with
JavaScript. Add `&splash=hold` to keep the opening splash on screen.

## What to do next (each step needs the owner's go-ahead; see "Working rules")

1. **Push** the 2 local commits (runs the free CI once, about 5 minutes).
2. **Release dry run**: `gh workflow run release.yml` (manual dispatch, publishes nothing). It now
   builds the six packages plus `pdfsqueeze-macos-<arch>.app.zip` for both Macs and smoke-tests
   the app. Free on a public repo; budget 20 to 40 minutes; watch it and cancel past about twice
   that. Fix whatever it finds (the app path has never run in CI).
3. **Installers**: test `install.sh` on Linux and `install.ps1` on Windows (a CI job per OS, free).
4. **Tag**: `git tag v0.1.0 && git push origin v0.1.0` publishes the files to a GitHub Release.
5. **Page**: set `RELEASE_READY = true` near the bottom of `docs/index.html` and push (Pages
   redeploys; GitHub caches about 10 minutes, so add a `?v=` query when checking).
6. Then the backlog below.

## Where things are

| Path | What it is |
|---|---|
| `src/main.rs` | Dispatcher: a bare desktop launch opens the window (`opens_window_on_launch`), else parse, then `ui`, `mcp` or `commands::execute`. Windows: `release_own_console` |
| `src/cli.rs` | All clap definitions (global `--json`). Single source for CLI, schemas and MCP |
| `src/commands.rs` | Every command returns a `Done` (text, JSON, warnings, exit code); error classification; JSON envelope |
| `src/agent.rs` | Tool schemas from clap, MCP stdio server, folder allow-list (`PathPolicy`, `PATH_ARGS`) |
| `src/ui.rs`, `src/ui.html` | The app: loopback server (per-run token, Host check; endpoints `/api/upload`, `files`, `run`, `progress`, `pages`, `pagethumb`, `pagepreview`, `thumb`, `download`, `save`, `save_all`, `reveal`, `open`, `remove`, `tools` and `/hero.jpg`), window (wry + tao), Mac menu (muda), the whole page in one HTML file |
| `src/engine.rs` | Finds and runs engines: bundled first (`engines/`, `../engines`, `../Resources/engines` inside the .app), then PATH; `Os` parameter; qpdf 11+ guard; `run_lines` streams Ghostscript page lines |
| `src/progress.rs` | Real progress of the running job, polled by the window |
| `src/compress.rs`, `src/batch.rs` | Ghostscript compression (dpi, JPEG quality, monotonic target-size ladder); folder jobs |
| `src/pages.rs` | Page ops: select, extract, delete, reorder, **organize**, merge, split, rotate; page geometry |
| `src/stamp.rs` | Watermark, page numbers, **sign** (overlay PDF with a soft-masked image placed by percent) |
| `src/security.rs`, `fix.rs`, `convert.rs`, `ocr.rs`, `imgpdf.rs`, `pdfout.rs` | Passwords, metadata and info; check, repair, linearize, PDF/A; render and text via Ghostscript; OCR overlay; images to PDF (incl. `load_rgba`); minimal PDF writer |
| `scripts/smoke.sh` | 53 end-to-end checks, any OS; `SMOKE_REQUIRE_BUNDLED=1`, `SMOKE_SKIP_OCR=1` |
| `scripts/bundle-engines.sh`, `package-macos.sh`, `package-macos-app.sh` | macOS bundling (dylib relocation, ad-hoc signing), tarball, and the `.app` |
| `scripts/bundle-engines-linux.sh`, `package-linux.sh` | Linux (ldd + patchelf; glibc floor recorded) |
| `scripts/stage-windows.ps1`, `bundle-tesseract-windows.sh` | Windows: official Ghostscript + qpdf unpacked with 7-Zip; MSYS2 only for Tesseract |
| `scripts/make-hero.py`, `set-hero.py` | Generated licence-free artwork; prepare a photograph as the background |
| `assets/` | `hero.jpg` (leaf photo, see CREDITS), `icon*.png`, `icon.ico` |
| `docs/` | Landing page (`index.html`, `leaf.jpg`, `hero.jpg`, `img/`), served by Pages |
| `install.sh`, `install.ps1` | One-line installers (no admin, no password) |
| `.github/workflows/ci.yml`, `release.yml` | CI on every push; release by hand or on a `v*` tag |

## Decisions and why

- **Bundle engines, do not rewrite them.** Ghostscript is AGPL, so the project is AGPL-3.0.
- **Three engines only.** Poppler dropped: no current 32-bit Windows build; Ghostscript renders and
  extracts text.
- **Window = wry + tao inside the one binary**, the page is one embedded HTML file on loopback; no Tauri CLI.
- **Windows engines are official builds, unpacked.** Never run a GUI or silent installer on a
  runner (it hung a job for an hour). Tesseract comes from MSYS2 in `engines/tesseract/`.
- **Windows 32-bit ships without OCR** (no Tesseract 32-bit build since 5.3.1).
- **Passwords never travel on a command line** (hidden prompt, `--password-file`, env var; qpdf `@-`).
- **Agents get JSON and stable codes**; schemas are generated from the CLI; MCP needs `--allow-dir`.
- **Public repo**, so CI and Pages are free. Private would take Pages offline, break the installers
  (raw URLs) and downloads, and cap CI minutes. The owner chose to keep it public (2026-10-13).
- **Signing is visual**, not certificate-based.
- **The UI server never trusts paths from the page**: it builds them from registered uploads and
  its own results folder; the signature picture is uploaded as `__signature.png` and hidden.
- **The Mac download is a `.app`**; a bare launch from the desktop opens the window, from a
  terminal it prints help; `PDFSQUEEZE_NO_GUI=1` forces help (agents, CI).

## Backlog

**Soon**
- Run the window by hand on Mac and Windows: drag, draw a signature, Save all, drop files from Finder.
- Native open and Save-As dialogs; finer OCR progress.
- Windows exe icon (needs a resource compile step); a Linux `.desktop` file.
- Test with real documents (scans, large files, encrypted, CJK). Real MCP client test.
- Free Windows signing (SignPath Foundation); Mac notarization is paid and not planned.
- Photographers' names in `CREDITS.md` (Magnific free licence needs attribution).
- Linux window (needs webkit2gtk; today it opens the browser).

**Later**
- OCR languages beyond English; deskew; PDF/A validation (veraPDF).
- Integration tests with fixture PDFs; batch for more commands; parallel batch.
- Certificate-based digital signatures; redaction, forms, compare, Office conversion.

## Gotchas learned the hard way

- **qpdf must be 11 or newer** (JSON v2). Ubuntu 22.04 ships 10.6, so the Linux job builds qpdf from
  source; `engine::qpdf()` rejects older ones (`engine_too_old`).
- **`cfg!()` keeps the code, `#[cfg]` removes it**; a unix-only API behind `cfg!(unix)` still has to
  compile on Windows.
- **Packages inherit the build OS floor** (the Intel Mac needs macOS 15, the only Intel runner;
  Linux needs the runner's glibc). A package built locally says this Mac's version; ignore that.
- **Every workflow job needs `timeout-minutes`**; workflows use `concurrency`.
- **`wry`/`tao` create no menu**; `muda` supplies Cmd+Q, Cmd+C, Cmd+V. A drag-drop handler that
  returns `true` swallows HTML5 drops (we want real paths), so reordering uses pointer events.
- **A drag can end before its last move event**: use the release position on pointerup.
- **A native window cannot be screenshotted** without Screen Recording permission; use
  `CGWindowListCopyWindowInfo` (a Swift one-liner), System Events (menu bar), `lsof`.
- **Browser-pane test drags**: the drag tool's end coordinate is not scaled like the start one; use
  scripted pointer events or a 1:1 viewport.
- **Default `<button>` padding** pushes icons off-centre; set `padding: 0`.
- **Pages cache** about 10 minutes; check the live site with a `?fresh=` query.
- **The sandbox refuses `rm` on `$(...)` results**; print the path first, then remove the literal path.
- **Ghostscript `-dSAFER` blocks file reads**; `-dNOSAFER` only for test generators.
- Shell: zsh (no word splitting), macOS bash 3.2, no `timeout` command.

## Working rules for this project

Standing rules are in the owner's vault `CLAUDE.md` and this repo's `CLAUDE.md`. In short: say what
a job consumes and wait for a yes; never spend or enable spending; every long job gets a timeout
and is watched; check account facts before warning; **remote side effects (push, trigger a
workflow, tag, publish, change repo settings) are confirmed each time**; verify locally first and
run remote jobs once; log notable runs the same session in the project log; say what was run
versus only written.

## Cost and account facts (checked 2026-10-09, still true 2026-10-13)

GitHub Free plan, no payment method, Actions budget $0 with "stop usage". Public repo, so standard
runner minutes and Pages are free. Do **not** click GitHub's "Enterprise trial" buttons. The
owner's commit email is public in the history.

## Open questions for the owner

- Push the 2 local commits and start the release dry run?
- Apply for SignPath Foundation (free Windows signing), or ship Windows unsigned for now?
- Wording of the first-launch notes (Gatekeeper, SmartScreen) on the page.
- Add the photographers' names to `CREDITS.md`.
