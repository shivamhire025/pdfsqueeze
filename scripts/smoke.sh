#!/bin/bash
# End-to-end smoke test of a pdfsqueeze executable, using only its own engines.
# Runs on macOS, Linux and Windows (Git Bash / MSYS2). Exits non-zero on the
# first failure.
#
#   scripts/smoke.sh /path/to/pdfsqueeze[.exe]
#
# Set SMOKE_SKIP_OCR=1 for builds that ship without Tesseract (32-bit Windows).
set -uo pipefail

BIN="${1:?usage: smoke.sh <pdfsqueeze executable>}"
[ -x "$BIN" ] || [ -f "$BIN" ] || { echo "no such executable: $BIN" >&2; exit 2; }
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

pass=0; fail=0
ok()   { echo "  ok    $1"; pass=$((pass + 1)); }
bad()  { echo "  FAIL  $1"; [ -n "${2:-}" ] && echo "        $2" | head -5; fail=$((fail + 1)); }

# run NAME EXPECTED_EXIT -- args...   EXPECTED_EXIT may be "0|3"; stdout goes to $OUT
run() {
  local name="$1" want="$2"; shift 3
  OUT="$("$BIN" "$@" 2>"$WORK/stderr.txt")"; local got=$?
  case "|$want|" in
    *"|$got|"*) ok "$name" ;;
    *) bad "$name (exit $got, wanted $want)" "$OUT $(cat "$WORK/stderr.txt")" ;;
  esac
}
has() { # has NAME PATTERN  (checks $OUT)
  if printf '%s' "$OUT" | grep -q -- "$2"; then ok "$1"; else bad "$1 (missing: $2)" "$OUT"; fi
}
exists() { if [ -s "$1" ]; then ok "file $1"; else bad "file $1 was not created"; fi; }

echo "pdfsqueeze smoke test: $("$BIN" --version)"

echo "== engines"
run "doctor" 0 -- --json doctor
has "gs found" '"name": "gs"'
has "qpdf found" '"name": "qpdf"'
echo "$OUT" | grep -q '"found": false' && echo "  note  some engines are missing (see doctor)"
if [ "${SMOKE_REQUIRE_BUNDLED:-0}" = "1" ]; then
  # packaged builds must use their own engines, never ones that happen to be installed
  # Keys are sorted, so a found-but-not-bundled engine reads "bundled":false,"found":true.
  # (An engine that is simply absent, like Tesseract in the 32-bit build, is not a failure.)
  if echo "$OUT" | tr -d ' \n' | grep -q '"bundled":false,"found":true'; then
    bad "all found engines are bundled" "$(echo "$OUT" | grep -B2 -A2 '"found": true')"
  else
    ok "all found engines are bundled"
  fi
fi

echo "== build a 3 page PDF from images"
# 1x1 PNG
printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -d > px.png 2>/dev/null \
  || printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -D > px.png
run "images-to-pdf" 0 -- images-to-pdf px.png px.png px.png -o three.pdf --page a4
exists three.pdf
run "info --json" 0 -- --json info three.pdf
has "3 pages" '"pages": 3'

echo "== page operations (qpdf)"
run "merge" 0 -- merge three.pdf three.pdf -o six.pdf
run "split" 0 -- split three.pdf --every 1 --out-dir parts
exists parts/three-part-001.pdf
run "extract" 0 -- extract three.pdf 1,z -o ex.pdf
run "delete" 0 -- delete three.pdf 2 -o del.pdf
run "reorder" 0 -- reorder three.pdf 3,1,2 -o re.pdf
run "rotate" 0 -- rotate three.pdf 90 --pages 1 -o rot.pdf
run "organize (reorder + rotate + delete)" 0 -- --json organize three.pdf --order 3,1 --rotate 1:90 -o org.pdf
has "organize reports pages" '"pages_kept": 2'
run "organize refuses an angle that is not 90/180/270" 1 -- --json organize three.pdf --order 1 --rotate 1:45 -o bad.pdf
has "organize bad angle is invalid_argument" '"code": "invalid_argument"'
run "linearize" 0 -- linearize three.pdf -o lin.pdf
run "check" 0 -- check three.pdf

echo "== stamping (qpdf + generated overlay)"
run "watermark" 0 -- watermark three.pdf DRAFT -o wm.pdf
run "number-pages" 0 -- number-pages three.pdf --format "Page {n} of {total}" -o num.pdf
run "number-pages on rotated page" 0 -- number-pages rot.pdf -o numrot.pdf
exists numrot.pdf

echo "== Ghostscript"
run "compress" 0 -- compress three.pdf -o small.pdf
run "to-images" 0 -- to-images three.pdf --out-dir imgs --dpi 30
exists imgs/three-1.png
run "text" 0 -- text num.pdf -o num.txt
if grep -q "Page 1 of 3" num.txt 2>/dev/null; then ok "text content"; else bad "text content" "$(cat num.txt 2>/dev/null)"; fi
run "pdfa" 0 -- --json pdfa three.pdf -o a.pdf
has "pdfa declared" '"declares_pdfa": true'

echo "== security and metadata"
printf 'smoke test pw 1\n' > pw.txt
run "protect" 0 -- protect three.pdf -o prot.pdf --password-file pw.txt
run "info on locked file" 0 -- --json info prot.pdf
has "reports locked" '"locked": true'
run "unlock" 0 -- unlock prot.pdf -o unl.pdf --password-file pw.txt
run "strip-metadata" 0 -- strip-metadata three.pdf -o clean.pdf
run "wrong password rejected" 1 -- --json unlock prot.pdf -o bad.pdf --password-file pw.txt.missing
has "error is classified" '"code"'

echo "== repair"
# cut a bigger file in half so the end (and cross-reference table) is gone
run "make a bigger file" 0 -- merge six.pdf six.pdf six.pdf six.pdf -o big.pdf
size=$(wc -c < big.pdf); head -c $((size / 2)) big.pdf > cut.pdf
run "check sees the damage" "1|3" -- --json check cut.pdf
run "repair damaged file" "0|3" -- --json repair cut.pdf -o fixed.pdf
exists fixed.pdf

echo "== agent mode"
run "never prompts" 1 -- --json protect three.pdf -o np.pdf
has "password_required" 'password_required'
run "tools schema" 0 -- tools --json
has "tool list" '"compress"'
OUT="$(printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | "$BIN" mcp --allow-dir "$WORK" 2>/dev/null)"
has "mcp initialize" '"serverInfo"'
has "mcp tools/list" '"inputSchema"'

if [ "${SMOKE_SKIP_OCR:-0}" = "1" ]; then
  echo "== OCR skipped (SMOKE_SKIP_OCR=1)"
else
  echo "== OCR (Tesseract)"
  run "ocr" 0 -- ocr three.pdf -o ocr.pdf
  exists ocr.pdf
fi

echo
echo "passed: $pass   failed: $fail"
[ "$fail" = 0 ]
