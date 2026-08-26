#!/usr/bin/env python3
"""
Generate Heptad's app icons.

The icon is just the instrument: seven bars in the seven pad hues, on the same
background colour as the page. Those hues are computed the same way app.js
computes them - degree * 360 / 7 - so if the palette ever changes, rerun this
rather than hand-editing a PNG.

    python tools/make-icons.py

One-shot; the output is committed. No dependencies (stdlib zlib writes the PNG),
because adding Pillow to a project with no build step to draw seven rectangles
would be silly.
"""

import struct
import zlib
from pathlib import Path

BG = (33, 36, 41)          # the plate: the pocket the pads sit in
PADS = 7
SUPERSAMPLE = 2            # render double size, box-filter down: cheap anti-aliasing

OUT = Path(__file__).resolve().parent.parent / "icons"


def hsl_to_rgb(h, s, l):
    """CSS hsl() -> 8-bit RGB, so the icon matches the stylesheet exactly."""
    h = (h % 360) / 360.0
    c = (1 - abs(2 * l - 1)) * s
    x = c * (1 - abs((h * 6) % 2 - 1))
    m = l - c / 2
    r, g, b = [(c, x, 0), (x, c, 0), (0, c, x),
               (0, x, c), (x, 0, c), (c, 0, x)][int(h * 6) % 6]
    return tuple(int(round((v + m) * 255)) for v in (r, g, b))


def render(size, inset_frac):
    """Draw the icon at `size`, with `inset_frac` of empty margin all round."""
    inset = size * inset_frac
    span = size - 2 * inset
    gap = span / PADS * 0.16
    bar_w = (span - gap * (PADS - 1)) / PADS
    radius = bar_w * 0.28

    # Precompute each bar's horizontal extent and colour.
    bars = []
    for degree in range(PADS):
        x0 = inset + degree * (bar_w + gap)
        hue = round(degree * 360 / PADS)
        # Between the pads at rest and the pads lit - an icon wants to read at
        # 32px, so it borrows the brightness of a pressed cap.
        bars.append((x0, x0 + bar_w, hsl_to_rgb(hue, 0.72, 0.55)))

    y0, y1 = inset, size - inset
    rows = []
    for y in range(size):
        row = [BG] * size
        for x0, x1, colour in bars:
            for x in range(int(x0), min(int(x1) + 1, size)):
                if not (x0 <= x < x1 and y0 <= y < y1):
                    continue
                # Rounded corners: only the corner quadrants need a distance test.
                cx = x0 + radius if x < x0 + radius else (x1 - radius if x > x1 - radius else x)
                cy = y0 + radius if y < y0 + radius else (y1 - radius if y > y1 - radius else y)
                if (x - cx) ** 2 + (y - cy) ** 2 > radius ** 2:
                    continue
                row[x] = colour
        rows.append(row)
    return rows


def downsample(rows, factor):
    """Box filter - this is what turns the jagged edges smooth."""
    size = len(rows) // factor
    out = []
    for y in range(size):
        row = []
        for x in range(size):
            acc = [0, 0, 0]
            for dy in range(factor):
                src = rows[y * factor + dy]
                for dx in range(factor):
                    px = src[x * factor + dx]
                    acc[0] += px[0]; acc[1] += px[1]; acc[2] += px[2]
            n = factor * factor
            row.append((acc[0] // n, acc[1] // n, acc[2] // n))
        out.append(row)
    return out


def write_png(path, rows):
    size = len(rows)
    raw = b"".join(b"\x00" + bytes(v for px in row for v in px) for row in rows)

    def chunk(kind, data):
        body = kind + data
        return (struct.pack(">I", len(data)) + body
                + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF))

    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    print(f"{path.relative_to(path.parent.parent)}  {size}x{size}  {len(path.read_bytes())} bytes")


# `inset` differs per icon: a maskable icon may be cropped to a circle by the OS,
# so its artwork has to stay inside the middle ~80%. The others can run closer
# to the edge and look less like a stamp.
TARGETS = [
    ("icon-192.png", 192, 0.10),
    ("icon-512.png", 512, 0.10),
    ("icon-maskable-512.png", 512, 0.20),
    ("apple-touch-icon.png", 180, 0.10),  # iOS Add to Home Screen uses this one
]

OUT.mkdir(exist_ok=True)
for name, size, inset in TARGETS:
    write_png(OUT / name, downsample(render(size * SUPERSAMPLE, inset), SUPERSAMPLE))
