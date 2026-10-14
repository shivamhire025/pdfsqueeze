#!/bin/sh
# pdfsqueeze installer for macOS and Linux. Nothing else needs to be installed.
#
#   curl -fsSL https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.sh | sh
#
# Installs to ~/.local/share/pdfsqueeze and links ~/.local/bin/pdfsqueeze.
# Nothing is written outside your home folder and no password is needed.
# (Windows: use install.ps1.)
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

case "$(uname -s)" in
  Darwin) PLATFORM=macos ;;
  Linux)  PLATFORM=linux ;;
  *) fail "this installer is for macOS and Linux. On Windows use install.ps1" ;;
esac

case "$(uname -m)" in
  arm64|aarch64) ARCH=arm64 ;;
  x86_64|amd64)  ARCH=x86_64 ;;
  *) fail "unsupported processor: $(uname -m)" ;;
esac

# The Linux package is built against glibc; Alpine and other musl systems differ.
if [ "$PLATFORM" = linux ] && (ldd --version 2>&1 | grep -qi musl); then
  fail "this build needs glibc; musl-based systems such as Alpine are not supported yet"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ -n "${PDFSQUEEZE_TARBALL:-}" ]; then
  cp "$PDFSQUEEZE_TARBALL" "$TMP/pkg.tar.gz"
else
  URL="https://github.com/$REPO/releases/latest/download/pdfsqueeze-$PLATFORM-$ARCH.tar.gz"
  echo "Downloading $URL"
  curl -fSL --progress-bar "$URL" -o "$TMP/pkg.tar.gz" \
    || fail "download failed. Is there a release for $PLATFORM/$ARCH yet?"
  # Every release file has a .sha256 beside it; refuse a download that does not match.
  curl -fsSL "$URL.sha256" -o "$TMP/pkg.sha256" \
    || fail "could not fetch the checksum file ($URL.sha256)"
  want="$(awk '{print $1}' "$TMP/pkg.sha256")"
  if command -v sha256sum >/dev/null 2>&1; then
    got="$(sha256sum "$TMP/pkg.tar.gz" | awk '{print $1}')"
  else
    got="$(shasum -a 256 "$TMP/pkg.tar.gz" | awk '{print $1}')"
  fi
  [ -n "$want" ] && [ "$want" = "$got" ] \
    || fail "checksum mismatch: the download is damaged or not the published file"
  echo "Checksum verified."
fi

tar -xzf "$TMP/pkg.tar.gz" -C "$TMP"
[ -x "$TMP/pdfsqueeze/pdfsqueeze" ] || fail "package looks wrong (no pdfsqueeze inside)"

# Refuse cleanly on a machine that is too old for the bundled engines.
if [ -f "$TMP/pdfsqueeze/engines/MIN_MACOS" ] && [ "$PLATFORM" = macos ]; then
  need="$(cat "$TMP/pdfsqueeze/engines/MIN_MACOS")"
  have="$(sw_vers -productVersion | cut -d. -f1)"
  if [ "$have" -lt "$need" ]; then
    fail "this build needs macOS $need or newer; you have $(sw_vers -productVersion)"
  fi
fi
if [ -f "$TMP/pdfsqueeze/engines/MIN_GLIBC" ] && [ "$PLATFORM" = linux ]; then
  need="$(cat "$TMP/pdfsqueeze/engines/MIN_GLIBC")"
  have="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')"
  if [ -n "$have" ] && [ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -1)" != "$need" ]; then
    fail "this build needs glibc $need or newer; you have $have"
  fi
fi

mkdir -p "$(dirname "$DEST")" "$BIN"
# Only replace a folder that is a previous pdfsqueeze install (or is empty or absent),
# so a mistyped PDFSQUEEZE_HOME can never wipe something else.
if [ -e "$DEST" ]; then
  if [ ! -d "$DEST" ] || { [ -n "$(ls -A "$DEST" 2>/dev/null)" ] && [ ! -x "$DEST/pdfsqueeze" ]; }; then
    fail "$DEST exists and is not a pdfsqueeze install; choose another PDFSQUEEZE_HOME or remove it yourself"
  fi
fi
rm -rf "$DEST"
mv "$TMP/pdfsqueeze" "$DEST"
ln -sf "$DEST/pdfsqueeze" "$BIN/pdfsqueeze"

# Files fetched with curl are not quarantined, but be safe if a browser was used.
if [ "$PLATFORM" = macos ]; then
  xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true
fi

echo "Installed: $BIN/pdfsqueeze"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo
     echo "Add this to your shell profile (~/.zshrc or ~/.bashrc) so the command is found:"
     echo "  export PATH=\"$BIN:\$PATH\"" ;;
esac
echo
"$BIN/pdfsqueeze" --version
"$BIN/pdfsqueeze" doctor
