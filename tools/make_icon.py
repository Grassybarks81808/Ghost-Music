#!/usr/bin/env python3
"""Build the Ghost Music Windows icon from the brand artwork.

`assets/ghost-logo.svg` is the source of truth for the brand mark.  This script
redraws the very same geometry (no SVG renderer needed) and writes:

    assets/ghost.ico          multi-size Windows icon (16 → 256 px)
    assets/ghost-logo-256.png preview used by the README

The .ico is assembled by hand so that the small sizes are stored as classic
32-bit BMP entries (best compatibility with the Windows shell/taskbar) and the
large sizes as PNG entries.

Usage:
    pip install pillow
    python tools/make_icon.py
"""

from __future__ import annotations

import re
import struct
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"

SIZES = [16, 24, 32, 48, 64, 128, 256]
VIEWBOX = 256.0
SUPERSAMPLE = 8
CANVAS = int(VIEWBOX * SUPERSAMPLE)

BG = (14, 15, 20, 255)  # #0e0f14
RED = (244, 43, 65, 255)  # #f42b41
INK = (247, 247, 251, 255)  # #f7f7fb

# Geometry copied from assets/ghost-logo.svg ----------------------------------
BODY = (
    "M48 137V105C48 59 82 29 128 29s80 30 80 76v70c0 12-10 18-20 11l-18-13"
    "-20 16-22-16-20 16-20-16-18 13c-10 7-20 1-20-11v-38Z"
)
HEADBAND = "M48 113c0-46 34-76 80-76s80 30 80 76"
MOUTH = "M118 139q10 10 20 0"
EAR_CUPS = [(36, 104, 30, 58), (190, 104, 30, 58)]
EYES = [(101, 112, 8, 12), (155, 112, 8, 12)]

TOKEN_RE = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]|-?\d*\.?\d+(?:[eE]-?\d+)?")
CURVE_STEPS = 40


def _flatten_cubic(p0, p1, p2, p3):
    pts = []
    for i in range(1, CURVE_STEPS + 1):
        t = i / CURVE_STEPS
        u = 1.0 - t
        x = (u**3) * p0[0] + 3 * (u**2) * t * p1[0] + 3 * u * (t**2) * p2[0] + (t**3) * p3[0]
        y = (u**3) * p0[1] + 3 * (u**2) * t * p1[1] + 3 * u * (t**2) * p2[1] + (t**3) * p3[1]
        pts.append((x, y))
    return pts


def _flatten_quad(p0, p1, p2):
    pts = []
    for i in range(1, CURVE_STEPS + 1):
        t = i / CURVE_STEPS
        u = 1.0 - t
        x = (u**2) * p0[0] + 2 * u * t * p1[0] + (t**2) * p2[0]
        y = (u**2) * p0[1] + 2 * u * t * p1[1] + (t**2) * p2[1]
        pts.append((x, y))
    return pts


def parse_path(d: str):
    """Flatten an SVG path into a list of polylines (absolute coordinates)."""
    tokens = TOKEN_RE.findall(d)
    subpaths: list[list[tuple[float, float]]] = []
    current: list[tuple[float, float]] = []
    pos = (0.0, 0.0)
    start = (0.0, 0.0)
    prev_cubic_ctrl = None
    prev_quad_ctrl = None
    command = ""
    index = 0

    def num() -> float:
        nonlocal index
        value = float(tokens[index])
        index += 1
        return value

    def point(relative: bool):
        nonlocal index
        x, y = float(tokens[index]), float(tokens[index + 1])
        index += 2
        if relative:
            return (pos[0] + x, pos[1] + y)
        return (x, y)

    while index < len(tokens):
        token = tokens[index]
        if re.match(r"[A-Za-z]", token):
            command = token
            index += 1
        elif command == "":
            raise ValueError(f"path starts with a number: {d!r}")
        # Implicit repetition of the previous command (M/m repeat as L/l).
        elif command in "Mm":
            command = "l" if command == "m" else "L"
        elif command in "Zz":
            command = "L"

        relative = command.islower()
        letter = command.upper()

        if letter == "M":
            pos = point(relative)
            start = pos
            if current:
                subpaths.append(current)
            current = [pos]
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif letter == "L":
            pos = point(relative)
            current.append(pos)
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif letter == "H":
            x = num()
            pos = (pos[0] + x if relative else x, pos[1])
            current.append(pos)
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif letter == "V":
            y = num()
            pos = (pos[0], pos[1] + y if relative else y)
            current.append(pos)
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif letter == "C":
            c1 = point(relative)
            c2 = point(relative)
            end = point(relative)
            current.extend(_flatten_cubic(pos, c1, c2, end))
            pos, prev_cubic_ctrl, prev_quad_ctrl = end, c2, None
        elif letter == "S":
            c1 = (
                (2 * pos[0] - prev_cubic_ctrl[0], 2 * pos[1] - prev_cubic_ctrl[1])
                if prev_cubic_ctrl
                else pos
            )
            c2 = point(relative)
            end = point(relative)
            current.extend(_flatten_cubic(pos, c1, c2, end))
            pos, prev_cubic_ctrl, prev_quad_ctrl = end, c2, None
        elif letter == "Q":
            c1 = point(relative)
            end = point(relative)
            current.extend(_flatten_quad(pos, c1, end))
            pos, prev_cubic_ctrl, prev_quad_ctrl = end, None, c1
        elif letter == "T":
            c1 = (
                (2 * pos[0] - prev_quad_ctrl[0], 2 * pos[1] - prev_quad_ctrl[1])
                if prev_quad_ctrl
                else pos
            )
            end = point(relative)
            current.extend(_flatten_quad(pos, c1, end))
            pos, prev_cubic_ctrl, prev_quad_ctrl = end, None, c1
        elif letter == "Z":
            current.append(start)
            subpaths.append(current)
            current = []
            pos = start
            prev_cubic_ctrl = prev_quad_ctrl = None
        else:
            raise ValueError(f"unsupported SVG command {command!r}")

    if current:
        subpaths.append(current)

    scale = SUPERSAMPLE
    return [[(x * scale, y * scale) for x, y in line] for line in subpaths]


def render_master() -> Image.Image:
    """Draw the brand mark at high resolution with alpha."""
    image = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    # Rounded dark tile.
    draw.rounded_rectangle(
        (0, 0, CANVAS - 1, CANVAS - 1), radius=56 * SUPERSAMPLE, fill=BG
    )

    # Ghost body.
    for subpath in parse_path(BODY):
        draw.polygon(subpath, fill=RED)

    # Headband.
    band = parse_path(HEADBAND)[0]
    width = 13 * SUPERSAMPLE
    draw.line(band, fill=INK, width=int(width), joint="curve")
    for end in (band[0], band[-1]):
        r = width / 2
        draw.ellipse((end[0] - r, end[1] - r, end[0] + r, end[1] + r), fill=INK)

    # Ear cups.
    for x, y, w, h in EAR_CUPS:
        draw.rounded_rectangle(
            (
                x * SUPERSAMPLE,
                y * SUPERSAMPLE,
                (x + w) * SUPERSAMPLE,
                (y + h) * SUPERSAMPLE,
            ),
            radius=12 * SUPERSAMPLE,
            fill=INK,
        )

    # Eyes.
    for cx, cy, rx, ry in EYES:
        draw.ellipse(
            (
                (cx - rx) * SUPERSAMPLE,
                (cy - ry) * SUPERSAMPLE,
                (cx + rx) * SUPERSAMPLE,
                (cy + ry) * SUPERSAMPLE,
            ),
            fill=BG,
        )

    # Smile.
    mouth = parse_path(MOUTH)[0]
    width = 7 * SUPERSAMPLE
    draw.line(mouth, fill=BG, width=int(width), joint="curve")
    for end in (mouth[0], mouth[-1]):
        r = width / 2
        draw.ellipse((end[0] - r, end[1] - r, end[0] + r, end[1] + r), fill=BG)

    return image


def bmp_entry(image: Image.Image) -> bytes:
    """ICO "BMP" payload: BITMAPINFOHEADER + BGRA pixels + 1bpp AND mask."""
    width, height = image.size
    header = struct.pack(
        "<IiiHHIIiiII",
        40,  # biSize
        width,
        height * 2,  # XOR + AND mask
        1,  # biPlanes
        32,  # biBitCount
        0,  # biCompression
        0,  # biSizeImage
        0,
        0,
        0,
        0,
    )
    rgba = image.convert("RGBA").load()
    xor = bytearray()
    for y in range(height - 1, -1, -1):  # bottom-up
        for x in range(width):
            r, g, b, a = rgba[x, y]
            xor += bytes((b, g, r, a))
    mask_row = ((width + 31) // 32) * 4
    mask = bytearray()
    for y in range(height - 1, -1, -1):
        row = bytearray(mask_row)
        for x in range(width):
            if rgba[x, y][3] < 128:
                row[x // 8] |= 0x80 >> (x % 8)
        mask += row
    return header + bytes(xor) + bytes(mask)


def png_entry(image: Image.Image) -> bytes:
    import io

    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=True)
    return buffer.getvalue()


def write_ico(path: Path, frames: list[Image.Image]) -> None:
    entries = []
    for frame in frames:
        width, height = frame.size
        payload = png_entry(frame) if max(width, height) >= 128 else bmp_entry(frame)
        entries.append((width, height, payload))

    out = bytearray(struct.pack("<HHH", 0, 1, len(entries)))
    offset = 6 + 16 * len(entries)
    for width, height, payload in entries:
        out += struct.pack(
            "<BBBBHHII",
            0 if width >= 256 else width,
            0 if height >= 256 else height,
            0,  # palette colours
            0,  # reserved
            1,  # colour planes
            32,  # bits per pixel
            len(payload),
            offset,
        )
        offset += len(payload)
    for _, _, payload in entries:
        out += payload
    path.write_bytes(out)


def main() -> None:
    master = render_master()
    frames = [master.resize((size, size), Image.LANCZOS) for size in SIZES]
    write_ico(ASSETS / "ghost.ico", frames)
    master.resize((256, 256), Image.LANCZOS).save(ASSETS / "ghost-logo-256.png")
    print(f"wrote {ASSETS / 'ghost.ico'} ({len(SIZES)} sizes)")
    print(f"wrote {ASSETS / 'ghost-logo-256.png'}")


if __name__ == "__main__":
    main()
