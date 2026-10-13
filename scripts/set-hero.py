#!/usr/bin/env python3
"""Use your own photograph as the window background.

    python3 scripts/set-hero.py path/to/photo.jpg

Resizes to at most 2400 px wide, writes assets/hero.jpg (kept under about 400 KB), then
rebuild with `cargo build --release`. Only use a photograph you have the right to ship:
the file is embedded in the app and published with the source. Needs Pillow.
"""
import sys, os
from PIL import Image

if len(sys.argv) != 2:
    sys.exit(__doc__)
src = sys.argv[1]
img = Image.open(src).convert("RGB")
if img.width > 2400:
    img = img.resize((2400, round(img.height * 2400 / img.width)), Image.LANCZOS)
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "hero.jpg")
for q in (78, 70, 62, 54, 46):
    img.save(out, quality=q, optimize=True, progressive=True, subsampling="4:2:0")
    if os.path.getsize(out) <= 400_000:
        break
print(f"wrote assets/hero.jpg  {img.width}x{img.height}  quality {q}  {os.path.getsize(out)//1024} KB")
