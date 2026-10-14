# Changelog

Plain-language record of what pdfsqueeze can do and how well that has been checked. Newest first.
"Verified" says how: by automated test, by CI on a real runner, or by hand.

## Unreleased (work since the 2026-10-10 CI run; not yet on GitHub)

### Added (icons)
- **Taskbar and Dock icon.** Windows: the icon is embedded in the .exe (`build.rs`, `assets/icon.ico`)
  and set as the window's taskbar icon, so Explorer, the taskbar and pinned shortcuts show it. macOS: a
  bare binary now sets the Dock icon too (the .app bundle already had one). *Verified:* builds and
  55 unit tests pass on macOS. *Not verified:* what the Dock or the Windows taskbar actually show;
  look at them by hand. The Windows part compiles only in CI.

### Security (2026-10-13 review, all fixes below)
- **Windows "Open" no longer goes through `cmd`**, so a file named like `a&b.pdf` cannot run a
  command; result names are sanitized too. *Written, not run on Windows.*
- **Only the results folder can be downloaded or saved** from the window server (not uploads,
  previews or the temporary password file); the page can no longer pass an `image` path.
  *Verified by unit test and live requests.*
- **Ghostscript always runs with `-dSAFER`** (explicit, not just the default); `doctor` warns when
  Ghostscript is older than 10.05. *Verified: 53 of 53 smoke checks pass with the flag.*
- **Window server hardening:** Content-Security-Policy on the page, session folder mode 0700, token
  from the OS random source, and browser mode opens a private local page so the token is not in the
  `open` argument list. *Verified on macOS.*
- **Installers verify the download's SHA-256** and refuse to delete a folder that is not a previous
  pdfsqueeze install. *The guard was tested with a local package; checksum checking needs a real
  release to run against.*
- **Release builds:** Windows Ghostscript and qpdf and the Linux qpdf source are pinned to exact
  versions with SHA-256 checks; actions are pinned to commit SHAs; build jobs get a read-only
  token and a separate `publish` job holds write access. *Written, not run in CI yet.*
- **Dependency audit** (`cargo audit`, 322 crates): no known vulnerabilities; four warnings, all in
  the Linux-only GTK stack that is not compiled on Mac or Windows.

### Added
- **Double-click to open**: started from the desktop with no arguments (Finder or Explorer), the
  program opens the window; from a terminal or script it still prints help
  (`PDFSQUEEZE_NO_GUI=1` forces that). On Windows the console that Explorer opens is closed.
  Windows and Linux behaviour was compiled and unit-tested in CI only, not used by a person.
- **A real Mac app**: `scripts/package-macos-app.sh` builds `pdfsqueeze.app` (zipped) with its
  tools inside and an app icon. *Verified on macOS (Apple Silicon):* the unzipped app passes the
  full smoke test (53 of 53) with Homebrew hidden, launches with `open`, shows a 1180x832
  window and a menu bar, and quits cleanly without leaving files behind. Ad-hoc signed, not
  notarized, so the first launch needs one confirmation (see README).
- **Landing page** (live at https://shivamhire025.github.io/pdfsqueeze/, source `docs/index.html`): redesigned in a dark
  forest-green editorial style: a framed hero over the leaf photograph, a wide heavy headline with
  one green accent word ("Stop uploading your PDFs to the internet"), numbered monospace section
  labels, a side index, switchable screenshots built from real page previews (organize, merge,
  sign, save all), per-platform download columns with file lists, one-line installers with copy
  buttons, the agent/MCP section and a progress log with what is and is not checked yet.
  No external requests, no analytics. *Verified:* desktop and phone widths in a browser tab, no
  horizontal overflow. Download buttons are held on the Releases page until a release exists.
- **Save all for many results**: PDF to images and Split now show a small preview strip and one
  "Save all N images" button that puts everything into a single new folder in Downloads
  (never overwriting), then "Open folder". The per-file buttons are gone for multi-file results.
- **Opening splash**: on launch the window shows a close-up leaf photograph at full strength with
  a white logo and the tagline in the centre for a moment, then fades into the app (click or
  press a key to skip). The same photograph, softened, is the background behind the app.
  The photograph's licence has to be confirmed before publishing.
- **Sign a PDF**: draw your signature, type it, or use an image (white background removed),
  then drag and resize it on a real preview of the page and apply it to one page or every page.
  Also a command: `pdfsqueeze sign contract.pdf --image signature.png --page z --x 60 --y 78 --width 25`.
  This is a visual signature (a picture of your signature), not a certificate-based digital
  signature. *Verified:* the signature lands within 0.2% of where it was placed, with no white
  box, on normal and rotated pages (90, 180, 270), other pages untouched; 3 smoke checks and 1
  unit test. Drawing and dragging were exercised in a browser tab with scripted pointer events,
  not by hand in the real window.
- **One white card for the work**: the tool tabs, the tool links and the work area now sit on a
  single white card instead of floating over the photograph.
- **Better legibility**: secondary text and links were darkened to at least 4.5:1 contrast
  against the lightest and darkest parts of the background (measured, not guessed), a soft
  frosted backing sits behind working screens, borders and selection states are firmer, and
  disabled controls are easier to read. The first screen keeps the photograph clear.
- **Simpler layout**: one slim toolbar (icon and label tabs with an underline for the active
  one, tools of the group as plain text links) replaces the rows of pills. The big title,
  subtitle and instruction paragraph are gone (one quiet line inside the page toolbar says how
  to use it). Flat surfaces without blur or shadows, a plain document list, and a full-bleed
  window. The action button carries the tool's name ("Merge", "Compress").
- **New look**: a light design in nature green: a close-up fern photograph behind frosted-glass
  panels (licence of the photo to be confirmed before publishing), a leaf-green accent (deeper green for text), large light headlines, pill buttons and a numbered first screen. The image is
  original artwork generated by `scripts/make-hero.py` (no licence to carry); replace
  `assets/hero.jpg` (or run `scripts/set-hero.py photo.jpg`) to use a real photograph. *Verified:* screenshots of the first screen, the
  page grid and a finished result in a browser tab; not yet seen in the real app window.
- **Merge shows thumbnails**: each document appears as a card with its first page, page count and
  size, numbered in merge order, with a move grip at the bottom (a grab hand on hover): drag a
  card to reorder, or focus the grip and use the arrow keys. Each card has a cross to remove that
  document without going back to the list. A running total of pages is shown. A dashed glass "Add a PDF" card the same size sits after the last document and opens
  the file picker. Other multi-file tools show the same cards. *Verified:* moving a card changed the merge
  order and the produced file followed it.
- **Organize pages**: one screen for reordering, rotating and deleting pages. Real page
  previews; click pages to select them (Shift-click for a range), then use icon buttons to turn
  or remove the selection; drag a page or a whole selection to move it. Keyboard: arrows to
  move, R to turn, Delete to remove, Space to select. Also a command: `pdfsqueeze organize book.pdf --order 3,1,2 --rotate 1:90`.
  *Verified:* output order and rotations checked against the produced file; 2 new smoke checks
  and 1 unit test. Drag and buttons exercised in a browser tab, **not** yet by hand in the app window.
- **Real progress bar**: shows pages processed, compression attempts and files finished, with a
  percentage, for compress, OCR, PDF to images, text extraction and batch compress. Other tasks
  are near-instant and show an animated bar without a number. *Verified:* a 40-page file reported
  22%, 52%, 82%, 100% against real page numbers.
- **Grouped tools**: the window now groups related tools under five icons (Shrink and fix,
  Pages, Convert, Protect and clean, Stamp) with the tools of the active group shown beside each
  other, instead of a menu of separate pages. *Verified:* in a browser tab only.
- Page previews for **Reorder pages** use real renders of every page (up to 200 pages).

### Checked
- 51 unit tests and 50 end-to-end smoke checks pass on macOS (Apple Silicon).

## 2026-10-10: app window and downloads built

### Added
- A real application window (not a browser tab) with a redesigned interface, a macOS menu bar,
  light and dark themes, drag-and-drop of files, and Save to Downloads that never overwrites.
- Packages for macOS (Apple Silicon and Intel), Windows (64-bit and 32-bit) and Linux (x86_64
  and arm64), each with the engines bundled so nothing else needs installing.
  *Verified in CI on real runners:* all six build and pass the end-to-end smoke test (Windows
  32-bit without OCR, because the OCR engine has no 32-bit build).
- Agent mode: every command can print one JSON object, never prompts, uses stable exit and error
  codes, and `pdfsqueeze mcp` serves the tools to AI assistants over MCP (folder allow-list required).

### Not yet checked
- The Windows window has compiled in CI but nobody has opened it on a Windows machine.
- The one-line installers for Linux and Windows have not been run; no release exists yet.
- Dropping a file from Finder or Explorer onto the window, and the keyboard shortcuts, were not
  tested by hand.
- No code signing: macOS packages are ad-hoc signed; Windows packages are unsigned.

## 2026-10-09: engine and command line

- 22 commands: compress (presets, DPI, size target, batch), merge, split, extract, delete,
  reorder, rotate, PDF to images, images to PDF, text extraction, OCR, password protect and
  unlock, metadata removal, info, watermark, page numbers, repair, check, fast web view, PDF/A.
- Runs entirely on the user's machine; engines are Ghostscript, qpdf and Tesseract.
- Licence AGPL-3.0 (Ghostscript requires it). Free and open source, nothing paid.
