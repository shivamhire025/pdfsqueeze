#!/bin/bash
# Assemble a self-contained engines/ folder (Ghostscript, qpdf, Poppler tools
# plus every non-system library they need) from a Homebrew install.
#
# Homebrew is needed on the BUILD machine only. The result runs on a Mac with
# nothing installed: all library references are rewritten to be relative.
#
# usage: scripts/bundle-engines.sh <output-dir>      e.g. dist/pdfsqueeze/engines
set -euo pipefail

OUT="${1:?usage: scripts/bundle-engines.sh <output-dir>}"
OUT="$(mkdir -p "$OUT" && cd "$OUT" && pwd)"
TOOLS="gs qpdf pdftoppm pdftotext pdfinfo"

command -v brew >/dev/null || { echo "error: Homebrew is required on the build machine" >&2; exit 1; }
PREFIX="$(brew --prefix)"
for t in $TOOLS; do
  [ -x "$PREFIX/bin/$t" ] || { echo "error: $t not found; run: brew install ghostscript qpdf poppler" >&2; exit 1; }
done

rm -rf "$OUT"; mkdir -p "$OUT/bin" "$OUT/lib" "$OUT/licenses" "$OUT/etc"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
QUEUE="$WORK/queue"; : > "$QUEUE"
KEGS="$WORK/kegs"; : > "$KEGS"

real() { readlink -f "$1"; }

# install_name_tool warns that it invalidates signatures; we re-sign at the end.
int() { install_name_tool "$@" 2> >(grep -v 'invalidate the code signature' >&2 || true); }

# Print the LC_RPATH entries of a Mach-O file.
rpaths() { otool -l "$1" | awk '/cmd LC_RPATH/{f=1} f && $1=="path"{print $2; f=0}'; }

# Where does install name $2 (referenced from original file $1) live on disk?
# Prints a real path, or nothing for system libraries / unresolvable names.
locate() {
  local from="$1" name="$2" base rp d
  case "$name" in
    /usr/lib/*|/System/*) return 0 ;;
    @loader_path/*)
      [ -e "$(dirname "$from")/${name#@loader_path/}" ] && real "$(dirname "$from")/${name#@loader_path/}"
      return 0 ;;
    @rpath/*)
      base="${name#@rpath/}"
      while IFS= read -r rp; do
        rp="${rp//@loader_path/$(dirname "$from")}"
        rp="${rp//@executable_path/$(dirname "$from")}"
        if [ -e "$rp/$base" ]; then real "$rp/$base"; return 0; fi
      done < <(rpaths "$from")
      for d in "$PREFIX/lib" "$PREFIX"/opt/*/lib; do
        if [ -e "$d/$base" ]; then real "$d/$base"; return 0; fi
      done
      return 0 ;;
    /*)
      [ -e "$name" ] && real "$name"
      return 0 ;;
  esac
}

note_keg() { # remember the Homebrew keg a file came from, for license files
  case "$1" in
    */Cellar/*/*/*) echo "${1%%/Cellar/*}/Cellar/$(echo "${1#*/Cellar/}" | cut -d/ -f1-2)" >> "$KEGS" ;;
  esac
}

# 1. the tools themselves
for t in $TOOLS; do
  src="$(real "$PREFIX/bin/$t")"
  cp "$src" "$OUT/bin/$t"; chmod u+w "$OUT/bin/$t"
  printf '%s\t%s\n' "$src" "$OUT/bin/$t" >> "$QUEUE"
  note_keg "$src"
done

# 2. walk the dependency graph, copying and re-pointing as we go
i=1
while :; do
  line="$(sed -n "${i}p" "$QUEUE")"
  [ -n "$line" ] || break
  i=$((i + 1))
  orig="${line%%$'\t'*}"; copy="${line#*$'\t'}"
  case "$copy" in "$OUT"/bin/*) mode=bin ;; *) mode=lib ;; esac
  own="$(basename "$copy")"

  if [ "$mode" = lib ]; then int -id "@loader_path/$own" "$copy"; fi

  while IFS= read -r dep; do
    [ -n "$dep" ] || continue
    r="$(locate "$orig" "$dep")"
    [ -n "$r" ] || continue                 # system library: leave alone
    base="$(basename "$dep")"
    [ "$base" != "$own" ] || continue       # a dylib's own id line
    if [ ! -e "$OUT/lib/$base" ]; then
      cp "$r" "$OUT/lib/$base"; chmod u+w "$OUT/lib/$base"
      printf '%s\t%s\n' "$r" "$OUT/lib/$base" >> "$QUEUE"
      note_keg "$r"
    fi
    if [ "$mode" = bin ]; then new="@executable_path/../lib/$base"; else new="@loader_path/$base"; fi
    int -change "$dep" "$new" "$copy"
  done < <(otool -L "$orig" | tail -n +2 | awk '{print $1}')

  # rpaths pointed into Homebrew; nothing needs them any more
  while IFS= read -r rp; do
    [ -n "$rp" ] && int -delete_rpath "$rp" "$copy"
  done < <(rpaths "$copy")
done

# 3. Ghostscript's resources (init files, ICC profiles, fonts)
GSROOT="$(dirname "$(dirname "$(real "$PREFIX/bin/gs")")")"
mkdir -p "$OUT/share"
cp -RL "$GSROOT/share/ghostscript" "$OUT/share/ghostscript"
if [ -d "$GSROOT/share/fonts" ]; then cp -RL "$GSROOT/share/fonts" "$OUT/share/fonts"; fi

# 4. a minimal fontconfig setup that uses the system's fonts
cat > "$OUT/etc/fonts.conf" <<'XML'
<?xml version="1.0"?>
<fontconfig>
  <dir>/System/Library/Fonts</dir>
  <dir>/Library/Fonts</dir>
  <dir>~/Library/Fonts</dir>
  <cachedir>~/Library/Caches/pdfsqueeze/fontconfig</cachedir>
</fontconfig>
XML

# 5. licence texts (AGPL/LGPL/etc. require shipping them) and a manifest
sort -u "$KEGS" | while IFS= read -r keg; do
  name="$(echo "${keg#*/Cellar/}" | cut -d/ -f1)"
  ver="$(echo "${keg#*/Cellar/}" | cut -d/ -f2)"
  echo "$name $ver" >> "$OUT/MANIFEST.txt"
  mkdir -p "$OUT/licenses/$name"
  for f in "$keg"/COPYING* "$keg"/LICENSE* "$keg"/LICENCE* "$keg"/COPYRIGHT* "$keg"/NOTICE*; do
    [ -f "$f" ] && cp "$f" "$OUT/licenses/$name/" || true
  done
done

# 6. ad-hoc sign (free; Apple Silicon refuses unsigned or modified binaries)
for f in "$OUT"/bin/* "$OUT"/lib/*; do codesign --force --sign - "$f" 2>/dev/null; done

# 7. fail loudly if anything still points outside the bundle
bad=0
for f in "$OUT"/bin/* "$OUT"/lib/*; do
  if otool -L "$f" | tail -n +2 | awk '{print $1}' | grep -E '^/opt/homebrew|^/usr/local|^@rpath' >/dev/null; then
    echo "error: $f still references an external library:" >&2
    otool -L "$f" | grep -E '/opt/homebrew|/usr/local|@rpath' >&2
    bad=1
  fi
done
[ "$bad" = 0 ] || exit 1

# 8. record the oldest macOS this bundle can run on (highest "minos" wins), so
#    the installer can refuse cleanly instead of crashing on an older Mac
max=0
for f in "$OUT"/bin/* "$OUT"/lib/*; do
  v="$(otool -l "$f" | awk '/LC_BUILD_VERSION/{f=1} f && $1=="minos"{print $2; exit}')"
  major="${v%%.*}"
  [ -n "$major" ] && [ "$major" -gt "$max" ] && max="$major"
done
echo "$max" > "$OUT/MIN_MACOS"

echo "bundled $(ls "$OUT/bin" | wc -l | tr -d ' ') tools and $(ls "$OUT/lib" | wc -l | tr -d ' ') libraries into $OUT ($(du -sh "$OUT" | cut -f1))"
