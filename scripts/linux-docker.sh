#!/bin/bash
# Build the Linux package and run the smoke test in Docker, the same recipe as the release
# workflow's linux job (Ubuntu 22.04, pinned qpdf and Ghostscript built from source).
#
#   scripts/linux-docker.sh            # needs Docker running; ~3 minutes on an Apple Silicon Mac
#
# Builds for the container's architecture (arm64 on an Apple Silicon Mac). Output: dist-linux/
# Nothing outside this repository is touched; the repository is mounted read-only.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v docker >/dev/null || { echo "error: docker not found" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "error: Docker is not running (open Docker Desktop)" >&2; exit 1; }
OUT="$PWD/dist-linux"; rm -rf "$OUT"; mkdir -p "$OUT"
RUNNER="$(mktemp -d)"; trap 'rm -rf "$RUNNER"' EXIT
cat > "$RUNNER/run.sh" <<'INNER'
#!/bin/bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq tesseract-ocr tesseract-ocr-eng fonts-urw-base35 patchelf cmake g++ \
  zlib1g-dev libjpeg-dev libssl-dev curl ca-certificates build-essential pkg-config >/dev/null
cd /tmp
curl -fsSL -o qpdf.tar.gz https://github.com/qpdf/qpdf/releases/download/v12.4.2/qpdf-12.4.2.tar.gz
echo "8a58af5b6141319287c1883bec8bd1bd545b7567b7fc5e6ce5d25a1c85f36397  qpdf.tar.gz" | sha256sum -c -
tar xzf qpdf.tar.gz && (cd qpdf-12.4.2 && cmake -S . -B build -DCMAKE_BUILD_TYPE=Release >/dev/null \
  && cmake --build build -j"$(nproc)" >/dev/null && cmake --install build >/dev/null)
curl -fsSL -o gs.tar.gz https://github.com/ArtifexSoftware/ghostpdl-downloads/releases/download/gs10080/ghostscript-10.08.0.tar.gz
echo "caf199e3f233f1290b27d0972d636f66c303355f2353309b7bfddf1edda06b3d  gs.tar.gz" | sha256sum -c -
tar xzf gs.tar.gz && (cd ghostscript-10.08.0 && ./configure --prefix=/usr/local --disable-cups --without-x >/dev/null \
  && make -j"$(nproc)" >/dev/null && make install >/dev/null && cp -R iccprofiles /usr/local/share/ghostscript/10.08.0/)
ldconfig; hash -r
curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null 2>&1
. "$HOME/.cargo/env"
mkdir -p /work && cd /src && tar --exclude=./target --exclude=./dist --exclude=./dist-linux --exclude=./.git -cf - . | tar -xf - -C /work
cd /work && bash scripts/package-linux.sh
cp -R dist/. /out/
INNER
echo "== building the Linux package in Docker (Ubuntu 22.04) =="
docker run --rm -v "$PWD":/src:ro -v "$RUNNER":/s:ro -v "$OUT":/out ubuntu:22.04 bash /s/run.sh
PKG="$(ls "$OUT"/pdfsqueeze-linux-*.tar.gz | head -1)"
mkdir -p "$OUT/x" && tar -xzf "$PKG" -C "$OUT/x"
echo "== smoke test in a bare Ubuntu container, bundled engines only =="
docker run --rm -e SMOKE_REQUIRE_BUNDLED=1 -v "$PWD/scripts":/scripts:ro -v "$OUT/x":/pkg:ro \
  ubuntu:22.04 bash /scripts/smoke.sh /pkg/pdfsqueeze/pdfsqueeze
echo "package: $PKG"
