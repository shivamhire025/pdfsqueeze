#!/bin/bash
# Stage a self-contained Windows package for the machine we are running on:
#   dist/pdfsqueeze-windows-<arch>/pdfsqueeze/   (arch = x64 or x86)
# The release workflow zips that folder afterwards.
#
# STATUS: written but NOT YET RUN on Windows.
#
# Run from an MSYS2 MINGW64 shell AFTER `cargo build --release` has produced
# target/release/pdfsqueeze.exe (built with the MSVC toolchain).
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH="${PDFSQUEEZE_WIN_ARCH:-x64}"
NAME="pdfsqueeze-windows-$ARCH"
STAGE="dist/$NAME/pdfsqueeze"
EXE="${PDFSQUEEZE_EXE:-target/release/pdfsqueeze.exe}"
[ -f "$EXE" ] || { echo "error: $EXE not found; run cargo build --release first" >&2; exit 1; }

rm -rf "dist/$NAME"
mkdir -p "$STAGE"
cp "$EXE" "$STAGE/pdfsqueeze.exe"
./scripts/bundle-engines-windows.sh "$STAGE/engines"
cp LICENSE README.md AGENTS.md "$STAGE/"

cat > "$STAGE/SOURCE.txt" <<EOF
pdfsqueeze is licensed under the GNU AGPL v3 (see LICENSE).
Source code: https://github.com/shivamhire025/pdfsqueeze

This package also bundles unmodified builds of third-party programs and
libraries (Ghostscript, qpdf, Tesseract and their dependencies) from MSYS2.
engines/MANIFEST.txt lists each with its version, engines/licenses/ holds their
licence texts, and their source is available from MSYS2
(https://github.com/msys2/MINGW-packages).
EOF

echo "staged $STAGE"
