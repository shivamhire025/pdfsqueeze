#!/bin/bash
# Assemble a self-contained engines/ folder (Ghostscript, qpdf, Tesseract plus
# the DLLs they need) from an MSYS2 MinGW installation.
#
# STATUS: written but NOT YET RUN on Windows. The first CI run is the first
# real test; expect to adjust package contents and file names.
#
# Run from an MSYS2 "MINGW64" shell with:
#   pacman -S --needed mingw-w64-x86_64-ghostscript mingw-w64-x86_64-qpdf \
#                      mingw-w64-x86_64-tesseract-ocr mingw-w64-x86_64-tesseract-data-eng
#
# We use the msvcrt-based mingw64 environment on purpose: its programs run on
# any Windows 7+ machine without installing a runtime.
#
# Windows loads DLLs from the folder the .exe is in, so everything goes in bin/.
#
# usage: scripts/bundle-engines-windows.sh <output-dir>
set -euo pipefail

OUT="${1:?usage: scripts/bundle-engines-windows.sh <output-dir>}"
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
PREFIX="${MINGW_PREFIX:-/mingw64}"

rm -rf "$OUT"; mkdir -p "$OUT/bin" "$OUT/share" "$OUT/licenses"

pick() { for n in "$@"; do [ -f "$PREFIX/bin/$n" ] && { echo "$PREFIX/bin/$n"; return; }; done; return 1; }

GS="$(pick gswin64c.exe gswin32c.exe gs.exe)" || { echo "error: Ghostscript not found in $PREFIX/bin" >&2; exit 1; }
QPDF="$(pick qpdf.exe)" || { echo "error: qpdf not found in $PREFIX/bin" >&2; exit 1; }
TESS="$(pick tesseract.exe)" || { echo "warning: tesseract not found; building without OCR" >&2; TESS=""; }

MANIFEST="$OUT/MANIFEST.txt"; : > "$MANIFEST"

copy_with_dlls() {
  local exe="$1"
  cp "$exe" "$OUT/bin/"
  # ldd lists the transitive DLL closure; keep only the ones from MinGW
  # (Windows' own system DLLs live under /c/Windows and must not be copied).
  ldd "$exe" | awk '/=>/ {print $3}' | grep -i "^$PREFIX/" | while read -r dll; do
    [ -f "$OUT/bin/$(basename "$dll")" ] || cp "$dll" "$OUT/bin/"
  done
}

copy_with_dlls "$GS"
copy_with_dlls "$QPDF"
[ -n "$TESS" ] && copy_with_dlls "$TESS"

# Ghostscript resources. MSYS2 uses share/ghostscript/<version>/
GSSHARE="$(ls -d "$PREFIX"/share/ghostscript/*/ 2>/dev/null | sort | tail -1 || true)"
if [ -n "$GSSHARE" ] && [ -d "$GSSHARE/Resource" ]; then
  mkdir -p "$OUT/share/ghostscript"
  cp -RL "$GSSHARE"/. "$OUT/share/ghostscript/"
elif [ -d "$PREFIX/share/ghostscript/Resource" ]; then
  mkdir -p "$OUT/share/ghostscript"
  cp -RL "$PREFIX/share/ghostscript"/. "$OUT/share/ghostscript/"
else
  echo "error: Ghostscript resources not found under $PREFIX/share/ghostscript" >&2; exit 1
fi
# Substitute fonts, if the package ships them separately
if [ -d "$PREFIX/share/fonts/urw-base35" ]; then
  mkdir -p "$OUT/share/ghostscript/fonts"
  cp -L "$PREFIX"/share/fonts/urw-base35/* "$OUT/share/ghostscript/fonts/" 2>/dev/null || true
fi

# Tesseract English data
if [ -n "$TESS" ]; then
  TD="$PREFIX/share/tessdata"
  if [ -f "$TD/eng.traineddata" ]; then
    mkdir -p "$OUT/share/tessdata"
    cp "$TD/eng.traineddata" "$OUT/share/tessdata/"
    for f in pdf.ttf configs tessconfigs; do
      [ -e "$TD/$f" ] && cp -RL "$TD/$f" "$OUT/share/tessdata/" || true
    done
  else
    echo "warning: eng.traineddata not found; OCR will not work" >&2
  fi
fi

# Licences and manifest
for pkg in ghostscript qpdf tesseract-ocr; do
  src="$PREFIX/share/licenses/$pkg"
  if [ -d "$src" ]; then
    mkdir -p "$OUT/licenses/$pkg"; cp -R "$src"/. "$OUT/licenses/$pkg/"
  fi
  v="$(pacman -Q "mingw-w64-x86_64-$pkg" 2>/dev/null || true)"
  [ -n "$v" ] && echo "$v" >> "$MANIFEST"
done

echo "bundled $(ls "$OUT/bin"/*.exe | wc -l | tr -d ' ') tools and $(ls "$OUT/bin"/*.dll 2>/dev/null | wc -l | tr -d ' ') DLLs into $OUT ($(du -sh "$OUT" | cut -f1))"
