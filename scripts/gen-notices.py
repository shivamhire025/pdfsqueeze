#!/usr/bin/env python3
"""Generate THIRD-PARTY-NOTICES.md and THIRD-PARTY-LICENSES-RUST.txt.

Run from the repository root after `cargo fetch --locked` (needs the crate sources in
~/.cargo/registry). The two files are copied into every package by the package scripts.

    python3 scripts/gen-notices.py

Bundled programs and libraries (tables below) must be kept in step with what the packaging
scripts actually bundle; each package's engines/MANIFEST.txt is the authority for versions.
"""
import glob, json, os, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGETS = ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc",
           "i686-pc-windows-msvc", "x86_64-unknown-linux-gnu"]

# name, SPDX licence (as declared by the Homebrew formula), upstream
MAC = [
    ("ghostscript", "AGPL-3.0-or-later", "https://ghostscript.com/releases/"),
    ("qpdf", "Apache-2.0", "https://github.com/qpdf/qpdf"),
    ("tesseract", "Apache-2.0", "https://github.com/tesseract-ocr/tesseract"),
    ("leptonica", "BSD-2-Clause", "https://github.com/DanBloomberg/leptonica"),
    ("jbig2dec", "AGPL-3.0-or-later", "https://github.com/ArtifexSoftware/jbig2dec"),
    ("openssl@3", "Apache-2.0", "https://www.openssl.org/source/"),
    ("freetype", "FTL (or GPL-2.0-or-later)", "https://freetype.org"),
    ("fontconfig", "HPND-sell-variant AND Unicode-3.0 AND MIT", "https://www.fontconfig.org"),
    ("gettext (libintl)", "LGPL-2.1-or-later", "https://www.gnu.org/software/gettext/"),
    ("libidn", "GPL-2.0-or-later OR LGPL-3.0-or-later", "https://www.gnu.org/software/libidn/"),
    ("xz (liblzma)", "0BSD", "https://tukaani.org/xz/"),
    ("libarchive", "BSD-2-Clause", "https://www.libarchive.org"),
    ("zstd", "BSD-3-Clause OR GPL-2.0-only", "https://github.com/facebook/zstd"),
    ("lz4", "BSD-2-Clause", "https://github.com/lz4/lz4"),
    ("libb2", "CC0-1.0", "https://github.com/BLAKE2/libb2"),
    ("libpng", "libpng-2.0", "http://www.libpng.org"),
    ("libtiff", "libtiff (BSD-like)", "https://libtiff.gitlab.io/libtiff/"),
    ("jpeg-turbo", "IJG AND Zlib AND BSD-3-Clause", "https://libjpeg-turbo.org"),
    ("giflib", "MIT", "https://giflib.sourceforge.net"),
    ("webp", "BSD-3-Clause", "https://developers.google.com/speed/webp"),
    ("openjpeg", "BSD-2-Clause", "https://www.openjpeg.org"),
    ("little-cms2", "MIT", "https://www.littlecms.com"),
    ("Tesseract English language data (eng.traineddata)", "Apache-2.0", "https://github.com/tesseract-ocr/tessdata"),
    ("Ghostscript support files (fonts, colour profiles, PostScript resources in share/ghostscript)", "distributed with Ghostscript under its licence; see the files' own headers and engines/licenses/ghostscript", "https://ghostscript.com/releases/"),
]
WIN = [
    ("Ghostscript (gswin64c.exe, gsdll64.dll)", "AGPL-3.0-or-later", "https://ghostscript.com/releases/"),
    ("qpdf (qpdf.exe, qpdf30.dll)", "Apache-2.0", "https://github.com/qpdf/qpdf"),
    ("Tesseract and Leptonica (MSYS2)", "Apache-2.0; BSD-2-Clause", "https://packages.msys2.org"),
    ("Microsoft Visual C++ runtime (vcruntime140, msvcp140, concrt140)", "Microsoft redistributable licence", "https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files"),
    ("OpenSSL (libcrypto, libssl)", "Apache-2.0", "https://www.openssl.org"),
    ("libarchive, libb2, libbz2, liblzma, libzstd, liblz4, zlib, libdeflate, brotli", "BSD / permissive / public domain", "https://packages.msys2.org"),
    ("libpng, libtiff, libjpeg-turbo, libwebp, giflib, openjpeg, libjbig, LERC", "permissive (BSD / MIT / libpng-like)", "https://packages.msys2.org"),
    ("libcurl, libssh2, nghttp2, nghttp3, ngtcp2, libpsl, libexpat", "curl / BSD / MIT", "https://packages.msys2.org"),
    ("libidn2, libunistring, libiconv, libintl", "LGPL-3.0-or-later (or GPL)", "https://packages.msys2.org"),
    ("libgcc_s, libstdc++, libgomp", "GPL-3.0-or-later with GCC Runtime Library Exception", "https://gcc.gnu.org"),
    ("libwinpthread", "MIT / ZPL-2.1", "https://www.mingw-w64.org"),
    ("Tesseract English language data (eng.traineddata)", "Apache-2.0", "https://github.com/tesseract-ocr/tessdata"),
]


MIT_TEXT = """Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE."""


def crates():
    out = {}
    for tgt in TARGETS:
        m = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--filter-platform", tgt, "--locked"],
            cwd=ROOT, stderr=subprocess.DEVNULL))
        pk = {p["id"]: p for p in m["packages"]}
        nodes = {n["id"]: n for n in m["resolve"]["nodes"]}
        seen, st = set(), [m["resolve"]["root"]]
        while st:
            i = st.pop()
            if i in seen:
                continue
            seen.add(i)
            for d in nodes[i]["deps"]:
                if any(k["kind"] in (None, "build") for k in d["dep_kinds"]):
                    st.append(d["pkg"])
        for i in seen:
            p = pk[i]
            if p["name"] == "pdfsqueeze":
                continue
            out[(p["name"], p["version"])] = p
    return out


def main():
    cs = crates()
    reg = os.path.expanduser("~/.cargo/registry/src")
    lines = []
    texts = []
    missing = []
    for (name, ver) in sorted(cs):
        p = cs[(name, ver)]
        lic = p.get("license") or "see license file"
        lines.append(f"| {name} | {ver} | {lic} | {p.get('repository') or ''} |")
        d = os.path.dirname(p["manifest_path"])
        if not os.path.isdir(d):
            found = glob.glob(os.path.join(reg, "*", f"{name}-{ver}"))
            d = found[0] if found else d
        files = sorted(f for f in glob.glob(os.path.join(d, "*"))
                       if os.path.isfile(f) and os.path.basename(f).upper().startswith(
                           ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "COPYRIGHT")))
        if not files:
            missing.append(f"{name} {ver}")
        texts.append("=" * 78 + f"\n{name} {ver}  ({lic})\n{p.get('repository') or ''}\n" + "=" * 78 + "\n")
        if not files:
            authors = "; ".join(p.get("authors") or []) or "the crate authors"
            texts.append(f"(The published crate contains no licence file. Its manifest declares the licence "
                         f"expression \"{lic}\"; authors: {authors}.)\n")
            if "MIT" in lic:
                texts.append(f"MIT License. Copyright (c) {authors}\n\n{MIT_TEXT}\n\n")
        for f in files:
            try:
                texts.append(f"--- {os.path.basename(f)} ---\n" + open(f, encoding="utf-8", errors="replace").read().rstrip() + "\n\n")
            except OSError:
                pass

    md = ["# Third-party notices", "",
          "pdfsqueeze itself is free software under the GNU Affero General Public License, version 3 or",
          "later (see `LICENSE`). It is built from, and ships together with, the third-party software",
          "listed here. Each component remains under its own licence; the notices below are provided as",
          "those licences require. The exact versions in a given download are listed in its",
          "`engines/MANIFEST.txt`, and licence texts for bundled programs are in `engines/licenses/`.", "",
          "## Written offer of source code", "",
          "For every program or library listed below that is licensed under the GNU AGPL, GPL or LGPL, the",
          "corresponding source code for the exact version in your download is available from the upstream",
          "project (links below). If you cannot obtain it there, open an issue at",
          "https://github.com/shivamhire025/pdfsqueeze/issues and it will be provided, for at least three",
          "years from the date of your download. pdfsqueeze does not modify these programs: they are",
          "unmodified builds from the distributor named in `engines/MANIFEST.txt` (Homebrew on macOS,",
          "Ubuntu packages on Linux, official Ghostscript and qpdf releases plus MSYS2 on Windows).", "",
          "Libraries under the LGPL are shipped as separate shared libraries inside `engines/`, so you can",
          "replace them with your own build of the same library.", "",
          "## Programs and libraries bundled with the macOS download", "",
          "| Component | Licence | Upstream |", "|---|---|---|"]
    md += [f"| {n} | {l} | {u} |" for n, l, u in MAC]
    md += ["", "## Programs and libraries bundled with the Windows downloads", "",
           "| Component | Licence | Upstream |", "|---|---|---|"]
    md += [f"| {n} | {l} | {u} |" for n, l, u in WIN]
    md += ["", "## Programs and libraries bundled with the Linux downloads", "",
           "Taken from the Ubuntu 22.04 packages named in `engines/MANIFEST.txt`; each package's",
           "licence file is in `engines/licenses/<package>/copyright`. This set includes Ghostscript and",
           "libgs (AGPL), Tesseract and Leptonica, qpdf, and the system libraries they need (several are",
           "LGPL or GPL with exceptions, for example GnuTLS, GMP, Nettle, libgcrypt, Kerberos, systemd,",
           "ICU, libstdc++).", "",
           "## Rust libraries compiled into pdfsqueeze", "",
           f"{len(lines)} crates across all platforms. Full licence and copyright texts are in",
           "`THIRD-PARTY-LICENSES-RUST.txt`.", "",
           "| Crate | Version | Licence | Repository |", "|---|---|---|---|"] + lines
    md += ["", "## Images", "",
           "The background photograph is from Magnific's free photo library under its free licence, which",
           "requires attribution; see `CREDITS.md`.", ""]
    open(os.path.join(ROOT, "THIRD-PARTY-NOTICES.md"), "w").write("\n".join(md))
    open(os.path.join(ROOT, "THIRD-PARTY-LICENSES-RUST.txt"), "w").write(
        "Licence and copyright texts of the Rust crates compiled into pdfsqueeze.\n"
        "Generated by scripts/gen-notices.py.\n\n" + "".join(texts))
    print(f"{len(cs)} crates; {len(missing)} had no licence file in the crate (fallback text used)")


if __name__ == "__main__":
    sys.exit(main())
