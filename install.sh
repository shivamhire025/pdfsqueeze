#!/bin/sh
# pdfsqueeze installer for macOS. No Homebrew or other software required.
#
#   curl -fsSL https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.sh | sh
#
# Installs to ~/.local/share/pdfsqueeze and links ~/.local/bin/pdfsqueeze.
# Nothing is written outside your home folder and no password is needed.
#
# Environment (all optional):
#   PDFSQUEEZE_TARBALL  install from this local .tar.gz instead of downloading
#   PDFSQUEEZE_HOME     install location (default ~/.local/share/pdfsqueeze)
#   PDFSQUEEZE_BIN      where to put the command (default ~/.local/bin)
set -eu

REPO="shivamhire025/pdfsqueeze"
DEST="${PDFSQUEEZE_HOME:-$HOME/.local/share/pdfsqueeze}"
BIN="${PDFSQUEEZE_BIN:-$HOME/.local/bin}"

fail() { echo "error: $*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || fail "this installer is for macOS only"

case "$(uname -m)" in
  arm64|x86_64) ARCH="$(uname -m)" ;;
  *) fail "unsupported processor: $(uname -m)" ;;
esac

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ -n "${PDFSQUEEZE_TARBALL:-}" ]; then
  cp "$PDFSQUEEZE_TARBALL" "$TMP/pkg.tar.gz"
else
  URL="https://github.com/$REPO/releases/latest/download/pdfsqueeze-macos-$ARCH.tar.gz"
  echo "Downloading $URL"
  curl -fSL --progress-bar "$URL" -o "$TMP/pkg.tar.gz" \
    || fail "download failed. Is there a release for $ARCH yet?"
fi

tar -xzf "$TMP/pkg.tar.gz" -C "$TMP"
[ -x "$TMP/pdfsqueeze/pdfsqueeze" ] || fail "package looks wrong (no pdfsqueeze inside)"

# Refuse cleanly on a Mac that is too old for the bundled engines.
if [ -f "$TMP/pdfsqueeze/engines/MIN_MACOS" ]; then
  need="$(cat "$TMP/pdfsqueeze/engines/MIN_MACOS")"
  have="$(sw_vers -productVersion | cut -d. -f1)"
  if [ "$have" -lt "$need" ]; then
    fail "this build needs macOS $need or newer; you have $(sw_vers -productVersion)"
  fi
fi

mkdir -p "$(dirname "$DEST")" "$BIN"
rm -rf "$DEST"
mv "$TMP/pdfsqueeze" "$DEST"
ln -sf "$DEST/pdfsqueeze" "$BIN/pdfsqueeze"

# Files fetched with curl are not quarantined, but be safe if a browser was used.
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true

echo "Installed: $BIN/pdfsqueeze"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo
     echo "Add this to your shell profile (~/.zshrc) so the command is found:"
     echo "  export PATH=\"$BIN:\$PATH\"" ;;
esac
echo
"$BIN/pdfsqueeze" --version
"$BIN/pdfsqueeze" doctor | grep -v tesseract
