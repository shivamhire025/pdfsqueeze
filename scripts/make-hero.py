#!/usr/bin/env python3
"""Generates assets/hero.jpg: a long archive corridor of paper-filled shelves in soft forest light.

Original artwork made from code, so there is no licence to carry. To use a real
photograph instead, replace assets/hero.jpg (2400x1350 works well; keep it under
about 400 KB) and rebuild. Needs Pillow and numpy.
"""
import math, random
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

W, H = 2400, 1350
random.seed(11)
HZ = int(H * 0.56)          # horizon
VX = int(W * 0.58)          # vanishing point (corridor runs off to the right of centre)
WALL = 760.0                # half width of the corridor at z = 1
TOP = 620.0                 # wall height above the horizon at z = 1
BOT = 300.0                 # below the horizon (floor line) at z = 1

def warm(v, k=1.0):
    v = max(0.0, min(1.0, v))
    return (int((16 + 196 * v) * k), int((24 + 206 * v) * k), int((15 + 170 * v) * k))

img = Image.new("RGB", (W, H), (4, 8, 5))
d = ImageDraw.Draw(img)

def lerp(a, b, t): return a + (b - a) * t

def proj(side, z, up):
    """Screen point on a wall: side -1 left/+1 right, z depth, up in 0 (floor)..1 (top)."""
    x = VX + side * WALL / z
    y = HZ + BOT / z - up * (TOP + BOT) / z
    return (x, y)

def quad(side, z0, z1, u0, u1, fill, outline=None):
    pts = [proj(side, z0, u0), proj(side, z1, u0), proj(side, z1, u1), proj(side, z0, u1)]
    d.polygon(pts, fill=fill, outline=outline)

# ceiling and floor planes
d.polygon([(0, 0), (W, 0), (VX + 40, HZ - 20), (VX - 40, HZ - 20)], fill=warm(0.06))
d.polygon([(0, H), (W, H), (VX + 60, HZ + 20), (VX - 60, HZ + 20)], fill=warm(0.09))

zs = [1.0]
while zs[-1] < 14:
    zs.append(zs[-1] * 1.22)
rows = 8
for side in (-1, 1):
    for zi in range(len(zs) - 1, 0, -1):          # far to near
        z0, z1 = zs[zi - 1], zs[zi]
        depth = 1 - min(1, math.log(z0) / math.log(14))
        quad(side, z0, z1, 0, 1, warm(0.05 + 0.04 * depth))
        for r in range(rows):
            u0 = (r + 0.12) / rows
            u1 = (r + 0.92) / rows
            n = 5
            for k in range(n):
                za = lerp(z0, z1, k / n + 0.01)
                zb = lerp(z0, z1, (k + 1) / n - 0.01)
                shade = (0.18 + 0.5 * depth) * random.uniform(0.55, 1.0)
                quad(side, za, zb, u0, u1, warm(shade))
                # lit top edge of each ream
                a, b = proj(side, za, u1), proj(side, zb, u1)
                d.line([a, b], fill=warm(min(1, shade + 0.28)), width=max(1, int(3 / z0)))
            # shelf board
            a, b = proj(side, z0, u0 - 0.04), proj(side, z1, u0 - 0.04)
            d.line([a, b], fill=warm(0.025), width=max(2, int(9 / z0)))
        # upright post at the near end of the bay
        a, b = proj(side, z0, 0), proj(side, z0, 1)
        d.line([a, b], fill=warm(0.02), width=max(2, int(14 / z0)))

# floor reflection
arr = np.asarray(img).astype(np.float32)
h = H - HZ
refl = arr[max(0, HZ - h):HZ][::-1]
fade = np.linspace(0.45, 0.0, refl.shape[0])[:, None, None]
arr[HZ:HZ + refl.shape[0]] = arr[HZ:HZ + refl.shape[0]] * (1 - fade) + refl * fade
img = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))

base = img.filter(ImageFilter.GaussianBlur(2.4))
glow = img.filter(ImageFilter.GaussianBlur(46))
out = np.asarray(base).astype(np.float32) * 0.82 + np.asarray(glow).astype(np.float32) * 0.62

yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
# the doorway of light at the far end: a soft glow, not a shape
door = Image.new("RGB", (W, H), (0, 0, 0))
dd = ImageDraw.Draw(door)
for i in range(50):
    t = i / 49
    r = lerp(120, 8, t)
    dd.ellipse([VX - r * 0.8, HZ - r * 1.15, VX + r * 0.8, HZ + r * 1.15], fill=warm(0.10 + 0.9 * t * t))
door = door.filter(ImageFilter.GaussianBlur(34))
out += np.asarray(door).astype(np.float32) * 1.15
# dust in the light: faint bokeh discs
rng = np.random.default_rng(5)
for _ in range(46):
    cx, cy = rng.uniform(0, W), rng.uniform(0, H * 0.8)
    r = rng.uniform(10, 42)
    m = np.exp(-(((xx - cx) ** 2 + (yy - cy) ** 2) / (2 * (r * 0.55) ** 2)))
    out += m[:, :, None] * rng.uniform(4, 13) * np.array([0.92, 1.0, 0.80])[None, None, :]

# darken the left third so headlines read, vignette, warm grade, grain
left = np.clip(1.0 - 0.55 * np.clip((W * 0.55 - xx) / (W * 0.55), 0, 1), 0.4, 1.0)
vx = (xx - W / 2) / (W / 2); vy = (yy - H / 2) / (H / 2)
vig = np.clip(1.2 - 0.62 * (vx ** 2 + vy ** 2), 0.22, 1.0)
out *= (left * vig)[:, :, None]
out *= np.array([0.86, 1.0, 0.84])[None, None, :]
out += rng.normal(0, 3.0, out.shape)
Image.fromarray(np.clip(out, 0, 255).astype(np.uint8)).save(
    "assets/hero.jpg", quality=64, optimize=True, progressive=True, subsampling="4:2:0")
print("wrote assets/hero.jpg")
