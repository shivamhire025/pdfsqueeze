#!/bin/bash
# Add Tesseract (OCR) to a staged Windows package, from an MSYS2 MinGW install.
#
# Run from an MSYS2 "MINGW64" shell, after scripts/stage-windows.ps1:
#   pacman -S --needed mingw-w64-x86_64-tesseract-ocr mingw-w64-x86_64-tesseract-data-eng
#   scripts/bundle-tesseract-windows.sh dist/pdfsqueeze-windows-x64/pdfsqueeze/engines
#
# Tesseract goes in its own engines/tesseract/ folder with its own DLLs: Windows
# loads DLLs from the executable's folder, and MSYS2's builds of libraries such
# as zlib must not replace the ones qpdf ships with.
#
# We use the msvcrt-based mingw64 environment on purpose: its programs run on
# any Windows 7+ machine without installing a runtime.
set -euo pipefail

ENGINES="${1:?usage: scripts/bundle-tesseract-windows.sh <path to staged engines folder>}"
[ -d "$ENGINES/bin" ] || { echo "error: $ENGINES/bin not found; run stage-windows.ps1 first" >&2; exit 1; }
ENGINES="$(cd "$ENGINES" && pwd)"
PREFIX="${MINGW_PREFIX:-/mingw64}"

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
    if [ -d "$src" ]; then mkdir -p "$ENGINES/licenses/$short"; cp -R "$src"/. "$ENGINES/licenses/$short/"; fi
  fi
done

# English data and the helper files Tesseract needs to write PDFs
TD="$PREFIX/share/tessdata"
mkdir -p "$ENGINES/share/tessdata"
if [ -f "$TD/eng.traineddata" ]; then
  cp "$TD/eng.traineddata" "$ENGINES/share/tessdata/"
else
  echo "warning: eng.traineddata not found under $TD; OCR will not work" >&2
fi
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
