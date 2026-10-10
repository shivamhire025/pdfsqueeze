#!/bin/bash
# Build a self-contained release tarball for the machine we are running on:
#   dist/pdfsqueeze-macos-<arch>.tar.gz   (arch = arm64 or x86_64)
#
# Needs Homebrew + `brew install ghostscript qpdf poppler` on the BUILD
# machine only. The tarball runs on a Mac with nothing installed, down to the
# macOS version recorded in engines/MIN_MACOS (set by the build machine's
# OS, so release builds should come from an old runner; see release.yml).
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH="$(uname -m)"                      # arm64 | x86_64
NAME="pdfsqueeze-macos-$ARCH"
STAGE="dist/$NAME/pdfsqueeze"

cargo build --release

rm -rf "dist/$NAME" "dist/$NAME.tar.gz"
mkdir -p "$STAGE"
cp target/release/pdfsqueeze "$STAGE/pdfsqueeze"
./scripts/bundle-engines.sh "$STAGE/engines"
cp LICENSE README.md "$STAGE/"

cat > "$STAGE/SOURCE.txt" <<EOF
pdfsqueeze is licensed under the GNU AGPL v3 (see LICENSE).
Source code: https://github.com/shivamhire025/pdfsqueeze

This package also bundles unmodified builds of third-party programs and
libraries (Ghostscript, qpdf, Poppler and their dependencies) from Homebrew.
engines/MANIFEST.txt lists each with its version, engines/licenses/ holds their
licence texts, and their source is available from the Homebrew formulae of the
same names (https://github.com/Homebrew/homebrew-core).
EOF

codesign --force --sign - "$STAGE/pdfsqueeze"   # free ad-hoc signature

tar -czf "dist/$NAME.tar.gz" -C "dist/$NAME" pdfsqueeze
( cd dist && shasum -a 256 "$NAME.tar.gz" > "$NAME.tar.gz.sha256" )

echo "built dist/$NAME.tar.gz ($(du -h "dist/$NAME.tar.gz" | cut -f1)), needs macOS $(cat "$STAGE/engines/MIN_MACOS")+"
