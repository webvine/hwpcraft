"""Render assets/icon.svg as a multi-size Windows .ico without external imaging libraries.

The SVG is four primitives (rounded square, page rectangle, the letter H as three
strokes, an underline), so they are rasterised here directly with 4x4 supersampling.
Sizes 16-64 are stored as 32-bit BMP entries, 256 as a PNG entry (Vista+ format).

usage: python -I make-icon.py <output.ico>
"""
import struct
import sys
import zlib

BACKGROUND = (0x23, 0x4C, 0x7D)
PAGE = (0xF5, 0xF8, 0xFD)
UNDERLINE = (0x7B, 0xA3, 0xD1)
SIZES = (16, 24, 32, 48, 64, 256)
SUPER = 4


def sample(x, y):
    """Colour and alpha at a point in the 128x128 SVG coordinate space."""
    r = 24.0
    cx = min(max(x, r), 128 - r)
    cy = min(max(y, r), 128 - r)
    if (x - cx) ** 2 + (y - cy) ** 2 > r * r:
        return None
    if 46 <= x <= 82 and 84.5 <= y <= 89.5:
        return UNDERLINE
    if (42 <= x <= 50 or 78 <= x <= 86) and 40 <= y <= 75:
        return BACKGROUND
    if 46 <= x <= 82 and 53 <= y <= 61:
        return BACKGROUND
    if 32 <= x <= 96 and 27 <= y <= 103:
        return PAGE
    return BACKGROUND


def render(size):
    """RGBA rows (top to bottom) for one icon size."""
    scale = 128.0 / size
    step = scale / SUPER
    rows = []
    for py in range(size):
        row = []
        for px in range(size):
            r = g = b = a = 0
            for sy in range(SUPER):
                y = (py * scale) + (sy + 0.5) * step
                for sx in range(SUPER):
                    x = (px * scale) + (sx + 0.5) * step
                    c = sample(x, y)
                    if c is not None:
                        r += c[0]
                        g += c[1]
                        b += c[2]
                        a += 1
            if a:
                row.append((r // a, g // a, b // a, (a * 255) // (SUPER * SUPER)))
            else:
                row.append((0, 0, 0, 0))
        rows.append(row)
    return rows


def bmp_entry(rows):
    size = len(rows)
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = bytearray()
    for row in reversed(rows):  # BMP stores bottom-up
        for r, g, b, a in row:
            pixels += bytes((b, g, r, a))
    mask_row = ((size + 31) // 32) * 4
    mask = bytes(mask_row * size)
    return header + bytes(pixels) + mask


def png_entry(rows):
    size = len(rows)
    raw = bytearray()
    for row in rows:
        raw.append(0)
        for r, g, b, a in row:
            raw += bytes((r, g, b, a))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def main(out):
    images = []
    for size in SIZES:
        rows = render(size)
        images.append((size, png_entry(rows) if size == 256 else bmp_entry(rows)))
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    directory = b""
    for size, data in images:
        dim = 0 if size == 256 else size
        directory += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    with open(out, "wb") as f:
        f.write(header + directory + b"".join(data for _, data in images))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: make-icon.py <output.ico>")
    main(sys.argv[1])
