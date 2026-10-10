#!/bin/bash
# Build a self-contained Linux tarball for the machine we are running on:
#   dist/pdfsqueeze-linux-<arch>.tar.gz   (arch = x86_64 or arm64)
#
# STATUS: written but NOT YET RUN; see bundle-engines-linux.sh.
# Build on the OLDEST distribution you want to support: the package needs
# at least the glibc of the machine it was built on (recorded in MIN_GLIBC).
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -m)" in
  x86_64)          ARCH=x86_64 ;;
  aarch64|arm64)   ARCH=arm64 ;;
  *) echo "unsupported processor: $(uname -m)" >&2; exit 1 ;;
esac
NAME="pdfsqueeze-linux-$ARCH"
STAGE="dist/$NAME/pdfsqueeze"

cargo build --release

rm -rf "dist/$NAME" "dist/$NAME.tar.gz"
mkdir -p "$STAGE"
cp target/release/pdfsqueeze "$STAGE/pdfsqueeze"
./scripts/bundle-engines-linux.sh "$STAGE/engines"
cp LICENSE README.md AGENTS.md "$STAGE/"

cat > "$STAGE/SOURCE.txt" <<EOF
pdfsqueeze is licensed under the GNU AGPL v3 (see LICENSE).
Source code: https://github.com/shivamhire025/pdfsqueeze

This package also bundles unmodified builds of third-party programs and
libraries (Ghostscript, qpdf, Tesseract and their dependencies) from the
distribution's packages. engines/MANIFEST.txt lists each with its version,
engines/licenses/ holds their licence texts, and their source is available
from the distribution that provided them.
EOF

tar -czf "dist/$NAME.tar.gz" -C "dist/$NAME" pdfsqueeze
( cd dist && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256" )

echo "built dist/$NAME.tar.gz ($(du -h "dist/$NAME.tar.gz" | cut -f1)), needs glibc $(cat "$STAGE/engines/MIN_GLIBC")+"
