#!/usr/bin/env python3
"""Writes the tone-map chart (a ramp to 1000 nits, colour patches, a sky to the peak) as a linear BT.709 PFM, 1.0 = SDR white: chart.py OUT.pfm."""
import struct
import sys

W, H = 256, 144
PEAK = 1000 / 203
HUES = [(1.0, 0.25, 0.2), (1.0, 0.6, 0.2), (0.9, 0.85, 0.3), (0.3, 0.9, 0.35), (0.3, 0.8, 0.9), (0.3, 0.45, 1.0),
        (0.85, 0.35, 0.9), (0.95, 0.7, 0.55)]
LEVELS = [0.5, 1.0, 2.0, 3.5]


def pixel(x, y):
    if y < 36:
        return (PEAK * (x / (W - 1)) ** 2.2,) * 3
    if y < 108:
        hue = HUES[x * len(HUES) // W]
        level = LEVELS[(y - 36) * len(LEVELS) // 72]
        luma = 0.2126 * hue[0] + 0.7152 * hue[1] + 0.0722 * hue[2]
        return tuple(level * c / luma for c in hue)
    t = x / (W - 1)
    sky = (0.35, 0.55, 1.0)
    return tuple((1 - t) * 0.6 * c + t * 0.95 * PEAK for c in sky)


with open(sys.argv[1], "wb") as out:
    out.write(b"PF\n%d %d\n-1.0\n" % (W, H))
    for y in reversed(range(H)):
        out.write(b"".join(struct.pack("<3f", *pixel(x, y)) for x in range(W)))
