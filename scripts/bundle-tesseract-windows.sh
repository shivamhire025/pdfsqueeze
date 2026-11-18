#!/bin/bash
# Add Tesseract (OCR) to a staged Windows package, from an MSYS2 MinGW install.
#
# Run from an MSYS2 "MINGW64" shell, after scripts/stage-windows.ps1:
#   pacman -S --needed mingw-w64-x86_64-tesseract-ocr
#   scripts/bundle-tesseract-windows.sh dist/pdfsqueeze-windows-x64/pdfsqueeze/engines
#
# English data (eng.traineddata) is downloaded, not taken from MSYS2: MSYS2 dropped its
# mingw64 tesseract-data packages on 2026-10-17. The file is the "fast" model from
# tesseract-ocr/tessdata_fast, pinned by commit (tag 4.1.0) and by sha256; the build stops
# if the checksum does not match.
#
# Tesseract goes in its own engines/tesseract/ folder with its own DLLs: Windows
# loads DLLs from the executable's folder, and MSYS2's builds of libraries such
# as zlib must not replace the ones qpdf ships with.
#
# We use the msvcrt-based mingw64 environment on purpose: its programs run on
# any Windows 7+ machine without installing a runtime.
set -euo pipefail
# Never fail silently: say which line stopped the script.
trap 'echo "error: bundle-tesseract-windows.sh stopped at line $LINENO: $BASH_COMMAND" >&2' ERR

ENGINES="${1:?usage: scripts/bundle-tesseract-windows.sh <path to staged engines folder>}"
[ -d "$ENGINES/bin" ] || { echo "error: $ENGINES/bin not found; run stage-windows.ps1 first" >&2; exit 1; }
ENGINES="$(cd "$ENGINES" && pwd)"
PREFIX="${MINGW_PREFIX:-/mingw64}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

TESS="$PREFIX/bin/tesseract.exe"
[ -f "$TESS" ] || { echo "error: $TESS not found (pacman -S mingw-w64-x86_64-tesseract-ocr)" >&2; exit 1; }

DEST="$ENGINES/tesseract"
rm -rf "$DEST"; mkdir -p "$DEST"
cp "$TESS" "$DEST/"

# ldd lists the transitive DLL closure; keep only MinGW ones (Windows' own
# system DLLs live under /c/Windows and must not be copied).
ldd "$TESS" | awk '/=>/ {print $3}' | grep -i "^$PREFIX/" | while read -r dll; do
  [ -f "$DEST/$(basename "$dll")" ] || cp "$dll" "$DEST/"
done

# Licence texts and versions for every package that supplied a DLL (not just Tesseract itself).
mkdir -p "$ENGINES/licenses"
for dll in "$DEST"/*.dll "$TESS"; do
  owner="$(pacman -Qo "$PREFIX/bin/$(basename "$dll")" 2>/dev/null | sed -n 's/.*is owned by //p' || true)"
  [ -n "$owner" ] || continue
  pkgfull="$(echo "$owner" | awk '{print $1}')"; ver="$(echo "$owner" | awk '{print $2}')"
  short="${pkgfull#mingw-w64-x86_64-}"
  if ! grep -q "^$pkgfull " "$ENGINES/MANIFEST.txt" 2>/dev/null; then
    echo "$pkgfull $ver" >> "$ENGINES/MANIFEST.txt"
    src="$PREFIX/share/licenses/$short"
    if [ -d "$src" ]; then
      mkdir -p "$ENGINES/licenses/$short"; cp -R "$src"/. "$ENGINES/licenses/$short/"
    elif [ -d "$HERE/licenses/$short" ]; then
      # MSYS2's package installs no licence file (giflib, libidn2, lz4): use the texts kept in
      # scripts/licenses/<name>, taken from the same upstream version (see SOURCE.txt there).
      mkdir -p "$ENGINES/licenses/$short"; cp -R "$HERE/licenses/$short"/. "$ENGINES/licenses/$short/"
    else
      # No folder under that name: take any licence-looking file the package installed,
      # wherever it put it.
      mkdir -p "$ENGINES/licenses/$short"
      # `|| true`: grep exits 1 when nothing matches, and under pipefail that would stop the whole
      # script silently before the warning below; a missing licence file must be a warning only.
      { pacman -Ql "$pkgfull" 2>/dev/null | awk '{print $2}' \
        | grep -iE '/(COPYING|LICEN[SC]E|NOTICE|COPYRIGHT)[^/]*$' | while read -r lf; do
          [ -f "$lf" ] && cp "$lf" "$ENGINES/licenses/$short/" || true
        done; } || true
      [ -n "$(ls -A "$ENGINES/licenses/$short")" ] || echo "warning: no licence file found for $pkgfull" >&2
    fi
  fi
done

# English data and the helper files Tesseract needs to write PDFs
TD="$PREFIX/share/tessdata"
mkdir -p "$ENGINES/share/tessdata"
ENG_COMMIT="65727574dfcd264acbb0c3e07860e4e9e9b22185"   # tessdata_fast tag 4.1.0
ENG_SHA256="7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2"
ENG_URL="https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/$ENG_COMMIT/eng.traineddata"
curl -fsSL --retry 3 --max-time 120 -o "$ENGINES/share/tessdata/eng.traineddata" "$ENG_URL" \
  || { echo "error: could not download eng.traineddata from $ENG_URL" >&2; exit 1; }
echo "$ENG_SHA256  $ENGINES/share/tessdata/eng.traineddata" | sha256sum -c - >/dev/null \
  || { echo "error: eng.traineddata does not match the pinned sha256" >&2; rm -f "$ENGINES/share/tessdata/eng.traineddata"; exit 1; }
echo "tessdata_fast eng.traineddata $ENG_COMMIT" >> "$ENGINES/MANIFEST.txt"
mkdir -p "$ENGINES/licenses/tessdata_fast"
curl -fsSL --retry 3 --max-time 60 -o "$ENGINES/licenses/tessdata_fast/LICENSE" \
  "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/$ENG_COMMIT/LICENSE" \
  || { echo "error: could not download the tessdata_fast LICENSE" >&2; exit 1; }
for f in pdf.ttf configs tessconfigs; do
  [ -e "$TD/$f" ] && cp -RL "$TD/$f" "$ENGINES/share/tessdata/" || true
done

# Licences and manifest
mkdir -p "$ENGINES/licenses"
for pkg in tesseract-ocr leptonica; do
  src="$PREFIX/share/licenses/$pkg"
  if [ -d "$src" ]; then mkdir -p "$ENGINES/licenses/$pkg"; cp -R "$src"/. "$ENGINES/licenses/$pkg/"; fi
  : # versions were recorded above, together with every other bundled DLL's package
done

echo "added tesseract with $(ls "$DEST"/*.dll 2>/dev/null | wc -l | tr -d ' ') DLLs to $ENGINES"
