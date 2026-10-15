#!/bin/bash
# Assemble a self-contained engines/ folder (Ghostscript, qpdf, Tesseract plus
# every non-system shared library they need) from distro packages.
#
# STATUS: written for Debian/Ubuntu but NOT YET RUN. The first CI run is the
# first real test; expect to adjust package paths.
#
# Build machine needs:
#   sudo apt install ghostscript qpdf tesseract-ocr tesseract-ocr-eng \
#                    fonts-urw-base35 patchelf
# End users need nothing but glibc (the build runner's version or newer).
#
# usage: scripts/bundle-engines-linux.sh <output-dir>
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:?usage: scripts/bundle-engines-linux.sh <output-dir>}"
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
TOOLS="gs qpdf tesseract"

for t in $TOOLS patchelf ldd; do
  command -v "$t" >/dev/null || { echo "error: $t not found (see header for packages)" >&2; exit 1; }
done

rm -rf "$OUT"; mkdir -p "$OUT/bin" "$OUT/lib" "$OUT/share" "$OUT/licenses"

# Libraries that must come from the target system (tied to its kernel/glibc).
EXCLUDE='^(linux-vdso|ld-linux|libc\.so|libm\.so|libdl\.so|libpthread\.so|librt\.so|libutil\.so|libresolv\.so|libnsl\.so|libcrypt\.so|libanl\.so|libBrokenLocale|libgcc_s)'

PKGS="$(mktemp)"; trap 'rm -f "$PKGS"' EXIT

for t in $TOOLS; do
  src="$(readlink -f "$(command -v "$t")")"
  cp "$src" "$OUT/bin/$t"
  echo "$src" >> "$PKGS"
  # ldd already lists the full transitive closure
  ldd "$src" | awk '/=>/ && $3 ~ /^\// {print $1, $3}' | while read -r name path; do
    if echo "$name" | grep -Eq "$EXCLUDE"; then continue; fi
    [ -e "$OUT/lib/$name" ] || cp -L "$path" "$OUT/lib/$name"
    echo "$path" >> "$PKGS"
  done
done

# Make everything find its libraries relative to itself.
for f in "$OUT"/bin/*; do patchelf --set-rpath '$ORIGIN/../lib' "$f"; done
for f in "$OUT"/lib/*;  do patchelf --set-rpath '$ORIGIN' "$f" 2>/dev/null || true; done

# Ghostscript resources: find the folder containing Resource/Init wherever the
# distribution put it (Debian: /usr/share/ghostscript/<version>/).
GSVER="$("$OUT/bin/gs" --version 2>/dev/null || gs --version)"
# Prefer the resources of the version we actually bundle (a source-built gs lives under /usr/local).
INIT="$(find /usr/local/share /usr/share /usr/lib -type d -path "*/ghostscript/$GSVER/Resource/Init" 2>/dev/null | head -1 || true)"
[ -n "$INIT" ] || INIT="$(find /usr/share /usr/lib /usr/local/share -type d -path '*/Resource/Init' 2>/dev/null | sort | tail -1 || true)"
if [ -n "$INIT" ]; then
  GSROOT="$(dirname "$(dirname "$INIT")")"
elif [ -d "/usr/local/share/ghostscript/$GSVER/lib" ]; then
  # A Ghostscript built from source embeds Resource/Init and the ICC profiles in the binary;
  # only its lib/ folder is installed.
  GSROOT="/usr/local/share/ghostscript/$GSVER"
else
  echo "error: Ghostscript resources not found (looked for */ghostscript/$GSVER)" >&2
  exit 1
fi
echo "ghostscript resources: $GSROOT"
mkdir -p "$OUT/share/ghostscript"
cp -RL "$GSROOT"/. "$OUT/share/ghostscript/"
for d in lib iccprofiles; do
  if [ ! -d "$OUT/share/ghostscript/$d" ]; then
    other="$(find /usr/share/ghostscript -maxdepth 2 -type d -name "$d" 2>/dev/null | sort | tail -1 || true)"
    [ -n "$other" ] && cp -RL "$other" "$OUT/share/ghostscript/$d"
  fi
done
# Standard-14 substitute fonts (urw-base35), used for PDFs that do not embed fonts.
for d in /usr/share/fonts/opentype/urw-base35 /usr/share/fonts/type1/urw-base35 /usr/share/fonts/urw-base35; do
  if [ -d "$d" ]; then
    mkdir -p "$OUT/share/ghostscript/fonts"
    cp -L "$d"/* "$OUT/share/ghostscript/fonts/" 2>/dev/null || true
    break
  fi
done

# Tesseract English data and its PDF helper files.
TD="$(dirname "$(find /usr/share -name eng.traineddata 2>/dev/null | head -1)")"
if [ -f "$TD/eng.traineddata" ]; then
  mkdir -p "$OUT/share/tessdata"
  cp "$TD/eng.traineddata" "$OUT/share/tessdata/"
  for f in pdf.ttf configs tessconfigs; do
    [ -e "$TD/$f" ] && cp -RL "$TD/$f" "$OUT/share/tessdata/" || true
  done
else
  echo "warning: no eng.traineddata found; OCR will not work in this bundle" >&2
fi

# Licence texts and manifest (best effort via dpkg).
if command -v dpkg >/dev/null; then
  sort -u "$PKGS" | while read -r f; do
    # dpkg records /lib/... or /usr/lib/... depending on the package; try every spelling.
    real="$(readlink -f "$f")"
    pkg=""
    for cand in "$f" "$real" "${f#/usr}" "/usr${f#/usr}" "${real#/usr}" "/usr${real#/usr}"; do
      pkg="$(dpkg -S "$cand" 2>/dev/null | head -1 | cut -d: -f1 || true)"
      [ -n "$pkg" ] && break
    done
    if [ -z "$pkg" ]; then
      case "$f" in
        # qpdf is built from source (Ubuntu 22.04 ships a qpdf too old for JSON v2): its licence
        # texts are kept in scripts/licenses/qpdf, because the install tree has none.
        */bin/gs|*/libgs.so*)
          if [ ! -d "$OUT/licenses/ghostscript-source" ]; then
            mkdir -p "$OUT/licenses/ghostscript-source"
            cp "$HERE"/licenses/ghostscript/* "$OUT/licenses/ghostscript-source/"
            echo "ghostscript $("$OUT/bin/gs" --version) (built from source, https://ghostscript.com/releases/)" >> "$OUT/MANIFEST.txt"
          fi ;;
        */qpdf|*/libqpdf.so*)
          if [ ! -d "$OUT/licenses/qpdf" ]; then
            mkdir -p "$OUT/licenses/qpdf"
            cp "$HERE"/licenses/qpdf/* "$OUT/licenses/qpdf/"
            echo "qpdf $("$OUT/bin/qpdf" --version | head -1 | awk '{print $3}') (built from source, https://github.com/qpdf/qpdf)" >> "$OUT/MANIFEST.txt"
          fi ;;
        *) echo "warning: no package owns $f (licence not recorded)" >&2 ;;
      esac
      continue
    fi
    if [ ! -d "$OUT/licenses/$pkg" ]; then
      mkdir -p "$OUT/licenses/$pkg"
      cp "/usr/share/doc/$pkg/copyright" "$OUT/licenses/$pkg/" 2>/dev/null || true
      pkgname="${pkg%%:*}"
      echo "$pkgname $(dpkg-query -W -f='${Version}' "$pkg" 2>/dev/null)" >> "$OUT/MANIFEST.txt"
    fi
  done
fi

# Oldest glibc this bundle can run on = the one it was built against.
glibc="$(ldd --version | head -1 | grep -oE '[0-9]+\.[0-9]+$' || echo 0)"
echo "$glibc" > "$OUT/MIN_GLIBC"

# Fail loudly if something still points outside the bundle.
bad=0
for f in "$OUT"/bin/*; do
  if ldd "$f" | grep -q "not found"; then echo "error: $f has unresolved libraries:" >&2; ldd "$f" | grep "not found" >&2; bad=1; fi
done
[ "$bad" = 0 ] || exit 1

echo "bundled $(ls "$OUT/bin" | wc -l | tr -d ' ') tools and $(ls "$OUT/lib" | wc -l | tr -d ' ') libraries into $OUT ($(du -sh "$OUT" | cut -f1)), needs glibc $glibc+"
