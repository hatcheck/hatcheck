#!/usr/bin/env python3
"""Recolor a macOS .iconset's PNGs by rotating hue (and optionally scaling saturation).

Usage:
    python3 recolor-icon.py <iconset_dir> <hue_shift> [sat_gain]

  hue_shift : integer in Pillow HSV units (0-255 == 0-360 degrees). e.g. +135 ~ orange->blue.
  sat_gain  : float multiplier for saturation (default 1.0).

Operates in place on every *.png in the iconset directory. Requires Pillow.
"""
import os
import sys
from PIL import Image

iconset = sys.argv[1]
hue_shift = int(sys.argv[2]) if len(sys.argv) > 2 else 0
sat_gain = float(sys.argv[3]) if len(sys.argv) > 3 else 1.0

for name in os.listdir(iconset):
    if not name.endswith(".png"):
        continue
    path = os.path.join(iconset, name)
    im = Image.open(path).convert("RGBA")
    r, g, b, a = im.split()
    h, s, v = Image.merge("RGB", (r, g, b)).convert("HSV").split()
    if hue_shift:
        h = h.point(lambda i: (i + hue_shift) % 256)
    if sat_gain != 1.0:
        s = s.point(lambda i: min(255, int(i * sat_gain)))
    out = Image.merge("HSV", (h, s, v)).convert("RGB")
    r2, g2, b2 = out.split()
    Image.merge("RGBA", (r2, g2, b2, a)).save(path)
    print("recolored", name)
