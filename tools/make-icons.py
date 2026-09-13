#!/usr/bin/env python3
"""
Generate Musika's icons.

    python tools/make-icons.py

writes

    icons/musika.ico       embedded into musika.exe by native/build.rs, and
                           used by the Start Menu and Desktop shortcuts
    icons/musika.png       the 256px version, for looking at
    icons/musika-64.rgba   raw 64x64 RGBA pixels that native/src/main.rs pulls in
                           with include_bytes! as the window icon - so the title
                           bar, Alt-Tab and the taskbar all show the same drawing

THE DESIGN

The instrument, as an object: a rounded tile of brushed aluminium, a dark
pocket routed into it, and the pads standing in the pocket in their hues - one
of them lit, as if it is being played. It is the same machined-metal language as
the app itself.

The previous icon was three flat bars on a black square. Windows rendered it
faithfully, and it still read as a test card rather than an application,
because an icon needs a silhouette: a shape with an edge that separates from
whatever taskbar or wallpaper is behind it. The rounded tile with transparent
corners is that silhouette.

Small sizes are simplified rather than shrunk. Seven keys in a 16px tile are a
pixel and a half each and grey into mush, so 16-32px draw three keys, 40-64px
draw five, and larger sizes draw all seven.

HOW IT IS DRAWN

Every shape is a rounded rectangle measured with a signed distance function:
for each pixel, how far it is from the shape's edge. Coverage is then a clamp of
that distance over one pixel, which gives smooth anti-aliased edges at any size
without supersampling - and it matters most at 16px, where a hard edge would
leave a staircase.

Stdlib only.
"""

import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "icons"
PADS = 7

# Aluminium, lit from above.
ALU_TOP = (0.87, 0.89, 0.92)
ALU_BOT = (0.55, 0.58, 0.63)
# The pocket the keys stand in: darkest at the top, where the lip shades it.
PLATE_TOP = (0.08, 0.09, 0.11)
PLATE_BOT = (0.17, 0.18, 0.21)


def hsl(h, s, l):
    """CSS-style hsl() as 0..1 floats - the same colour model the pads use."""
    l = max(0.0, min(1.0, l))
    h = (h % 360) / 360.0
    c = (1 - abs(2 * l - 1)) * s
    x = c * (1 - abs((h * 6) % 2 - 1))
    m = l - c / 2
    r, g, b = [(c, x, 0), (x, c, 0), (0, c, x),
               (0, x, c), (x, 0, c), (c, 0, x)][int(h * 6) % 6]
    return (r + m, g + m, b + m)


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def sd_round_rect(x, y, x0, y0, x1, y1, r):
    """Signed distance from (x, y) to a rounded rectangle: negative inside."""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    hw, hh = (x1 - x0) / 2, (y1 - y0) / 2
    r = min(r, hw, hh)
    qx = abs(x - cx) - (hw - r)
    qy = abs(y - cy) - (hh - r)
    return math.hypot(max(qx, 0.0), max(qy, 0.0)) + min(max(qx, qy), 0.0) - r


def coverage(d):
    """How much of a pixel a shape covers, from its distance to the edge."""
    return max(0.0, min(1.0, 0.5 - d))


def over(dst, rgb, a):
    """Paint `rgb` at opacity `a` over `dst`, straight (non-premultiplied) alpha."""
    if a <= 0:
        return dst
    dr, dg, db, da = dst
    out_a = a + da * (1 - a)
    k = da * (1 - a)
    return ((rgb[0] * a + dr * k) / out_a,
            (rgb[1] * a + dg * k) / out_a,
            (rgb[2] * a + db * k) / out_a,
            out_a)


def render(S):
    """Draw the icon at S x S. Returns RGBA bytes, top row first."""
    n = 3 if S <= 32 else (5 if S <= 64 else PADS)
    small = S <= 24

    m = S * 0.055
    tile = (m, m, S - m, S - m)
    tile_r = S * 0.225

    if small:
        plate = (m + S * 0.11, m + S * 0.15, S - m - S * 0.11, S - m - S * 0.13)
        plate_r, pad = S * 0.06, 0.0
    else:
        plate = (m + S * 0.10, m + S * 0.165, S - m - S * 0.10, S - m - S * 0.13)
        plate_r, pad = S * 0.075, S * 0.03

    kx0, ky0 = plate[0] + pad, plate[1] + pad
    kx1, ky1 = plate[2] - pad, plate[3] - pad
    gap_frac = 0.22
    kw = (kx1 - kx0) / (n + gap_frac * (n - 1))
    gap = kw * gap_frac
    key_r = kw * 0.34
    lit = {7: 4, 5: 2}.get(n)  # the V chord, being played

    keys = []
    for i in range(n):
        x0 = kx0 + i * (kw + gap)
        keys.append(((x0, ky0, x0 + kw, ky1), i * 360 / n, i == lit))

    edge = max(1.0, S / 64)        # width of the tile's lit/shaded rim
    key_edge = max(0.8, S / 96)    # width of each key's top highlight

    px = bytearray(S * S * 4)
    for y in range(S):
        fy = y + 0.5
        for x in range(S):
            fx = x + 0.5

            d = sd_round_rect(fx, fy, *tile, tile_r)
            a_tile = coverage(d)
            if a_tile <= 0:
                continue  # outside the tile: transparent

            t = (fy - tile[1]) / (tile[3] - tile[1])
            c = over((0.0, 0.0, 0.0, 0.0), mix(ALU_TOP, ALU_BOT, t), a_tile)

            # A thin rim: light catching the top edge, shade along the bottom.
            # This single rule - highlight above, shadow below - is most of
            # what makes a flat shape read as a physical object.
            rim = a_tile * (1 - coverage(d + edge))
            if rim > 0:
                c = over(c, (1.0, 1.0, 1.0), rim * 0.6 * (1 - t) ** 2)
                c = over(c, (0.0, 0.0, 0.0), rim * 0.35 * t ** 2)

            a_plate = coverage(sd_round_rect(fx, fy, *plate, plate_r))
            if a_plate > 0:
                tp = (fy - plate[1]) / (plate[3] - plate[1])
                c = over(c, mix(PLATE_TOP, PLATE_BOT, tp), a_plate)

            for (x0, y0, x1, y1), hue, is_lit in keys:
                if fx < x0 - 1 or fx > x1 + 1:
                    continue
                dk = sd_round_rect(fx, fy, x0, y0, x1, y1, key_r)
                a_key = coverage(dk)
                if a_key <= 0:
                    continue
                tk = (fy - y0) / (y1 - y0)
                sat, light = (0.95, 0.64) if is_lit else (0.70, 0.55)
                c = over(c, hsl(hue, sat, light + 0.09 - 0.17 * tk), a_key)
                if not small:
                    top = a_key * (1 - coverage(dk + key_edge))
                    c = over(c, (1.0, 1.0, 1.0), top * 0.45 * (1 - tk) ** 3)

            i = (y * S + x) * 4
            px[i:i + 4] = bytes(int(round(max(0.0, min(1.0, v)) * 255)) for v in c)
    return px


def png_bytes(S, px):
    """RGBA PNG (colour type 6)."""
    raw = bytearray()
    for y in range(S):
        raw.append(0)  # filter: none
        raw += px[y * S * 4:(y + 1) * S * 4]

    def chunk(kind, data):
        body = kind + data
        return (struct.pack(">I", len(data)) + body
                + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", S, S, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + chunk(b"IEND", b""))


def bmp_entry(S, px):
    """An icon image in the original .ico bitmap form.

    A PNG is allowed inside an .ico since Vista, but Microsoft only recommends
    it for the 256px image: parts of the shell draw small PNG entries badly or
    fall back to a generic icon. So every size below 256 goes in the classic
    form - a BITMAPINFOHEADER, 32-bit BGRA pixels bottom row first, then a
    one-bit AND mask marking the transparent pixels for anything too old to
    read the alpha channel. The header claims twice the height because the
    format counts the mask as part of the image.
    """
    header = struct.pack("<IiiHHIIiiII", 40, S, S * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    xor = bytearray()
    for y in range(S - 1, -1, -1):
        for x in range(S):
            r, g, b, a = px[(y * S + x) * 4:(y * S + x) * 4 + 4]
            xor += bytes((b, g, r, a))
    stride = ((S + 31) // 32) * 4
    mask = bytearray()
    for y in range(S - 1, -1, -1):
        row = bytearray(stride)
        for x in range(S):
            if px[(y * S + x) * 4 + 3] == 0:
                row[x // 8] |= 0x80 >> (x % 8)
        mask += row
    return bytes(header + xor + mask)


def write_ico(path, sizes):
    images = []
    for s in sizes:
        px = render(s)
        images.append(png_bytes(s, px) if s >= 256 else bmp_entry(s, px))

    header = struct.pack("<HHH", 0, 1, len(images))  # reserved, type 1 = icon
    offset = len(header) + 16 * len(images)
    entries, blob = b"", b""
    for s, data in zip(sizes, images):
        byte = s if s < 256 else 0  # 0 means 256 - the format is that old
        entries += struct.pack("<BBBBHHII", byte, byte, 0, 0, 1, 32, len(data), offset)
        blob += data
        offset += len(data)
    path.write_bytes(header + entries + blob)
    print(f"{path.name:<16} sizes {sizes}  {path.stat().st_size} bytes")


OUT.mkdir(exist_ok=True)

# Every size Windows commonly asks for at 100-200% scaling, so it never has to
# stretch one it was not given.
write_ico(OUT / "musika.ico", [16, 20, 24, 32, 40, 48, 64, 256])

big = render(256)
(OUT / "musika.png").write_bytes(png_bytes(256, big))
print(f"{'musika.png':<16} 256x256")

(OUT / "musika-64.rgba").write_bytes(bytes(render(64)))
print(f"{'musika-64.rgba':<16} 64x64 raw RGBA for the window icon")
