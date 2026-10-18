#!/usr/bin/env python3
"""Build assets/bigwave.ico (exe resource + NSIS installer/uninstaller icon).

Windows Explorer, Edge's download list and the shell icon cache can fail to
render PNG-compressed frames at small sizes and fall back to the generic
exe/installer icon. So this writes a classic ICO:

  256 px          PNG (assets/bigwave-icon.png, byte-for-byte)
  128..16 px      32-bit BGRA DIB with a matching 1-bit AND mask

Inputs: assets/icons/bigwave-radio_<N>.png (the wolf pack, hand-sized frames);
24 px has no pack frame and is resampled from the 256 px icon.
Needs Pillow. Do NOT regenerate the .ico with Pillow's ICO writer
(Image.save(..., format="ICO")): it stores every size as PNG.
"""
import io
import os
import struct

from PIL import Image

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
P = lambda *a: os.path.join(ROOT, *a)

BMP_SIZES = [128, 64, 48, 32, 24, 16]


def frame_image(size):
    path = P("assets", "icons", f"bigwave-radio_{size}.png")
    if os.path.exists(path):
        im = Image.open(path).convert("RGBA")
        if im.size == (size, size):
            return im
    src = Image.open(P("assets", "bigwave-icon.png")).convert("RGBA")
    return src.resize((size, size), Image.LANCZOS)


def dib(im):
    w, h = im.size
    px = im.tobytes()  # RGBA, top-down
    xor = bytearray()
    for y in range(h - 1, -1, -1):  # DIB rows are bottom-up
        row = px[y * w * 4:(y + 1) * w * 4]
        for x in range(w):
            r, g, b, a = row[x * 4:x * 4 + 4]
            xor += bytes((b, g, r, a))
    stride = ((w + 31) // 32) * 4
    andmask = bytearray()
    for y in range(h - 1, -1, -1):
        bits = bytearray(stride)
        for x in range(w):
            if px[(y * w + x) * 4 + 3] == 0:  # 1 = transparent
                bits[x // 8] |= 0x80 >> (x % 8)
        andmask += bits
    # BITMAPINFOHEADER: height is doubled (XOR + AND), no compression
    hdr = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, len(xor) + len(andmask), 0, 0, 0, 0)
    return hdr + bytes(xor) + bytes(andmask)


def main():
    frames = [(256, open(P("assets", "bigwave-icon.png"), "rb").read())]
    for s in BMP_SIZES:
        frames.append((s, dib(frame_image(s))))
    out = io.BytesIO()
    out.write(struct.pack("<HHH", 0, 1, len(frames)))
    off = 6 + 16 * len(frames)
    for s, data in frames:
        out.write(struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), off))
        off += len(data)
    for _, data in frames:
        out.write(data)
    with open(P("assets", "bigwave.ico"), "wb") as f:
        f.write(out.getvalue())
    print("wrote assets/bigwave.ico:", ", ".join(f"{s}{'(png)' if s == 256 else ''}" for s, _ in frames))


if __name__ == "__main__":
    main()
