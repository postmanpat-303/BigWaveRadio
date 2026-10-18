#!/usr/bin/env python3
"""Regenerate BigWave Radio branding from the wolf icon (needs Pillow).

Inputs : assets/bigwave-icon.png (256 px wolf icon, same as /workspace/loud/pack/BigWave_Radio)
         assets/bigwave-wordmark.png (green/silver "BigWave RADIO" lettering)
(assets/bigwave.ico is NOT made here: see scripts/make_ico.py, which writes BMP frames under 256 px.)
Outputs: assets/bigwave-logo.png        512x512 About-page logo (wolf icon over the wordmark)
         installer/welcome.bmp          164x314 NSIS welcome/finish page bitmap
         installer/header.bmp           150x57  NSIS header bitmap
Usage  : python3 scripts/make_branding.py
"""
import os
from PIL import Image, ImageDraw, ImageFilter

ROOT = os.path.join(os.path.dirname(__file__), "..")
P = lambda *a: os.path.join(ROOT, *a)
LIME = (68, 239, 23)
DEEP = (3, 18, 10)
FOREST = (11, 61, 20)

icon = Image.open(P("assets", "bigwave-icon.png")).convert("RGBA")
word = Image.open(P("assets", "bigwave-wordmark.png")).convert("RGBA")


def scaled(im, w=None, h=None):
    if w is None:
        w = round(im.width * h / im.height)
    if h is None:
        h = round(im.height * w / im.width)
    return im.resize((w, h), Image.LANCZOS)


def shadowed(canvas, im, xy, blur=6, alpha=150):
    sh = Image.new("RGBA", im.size, (0, 0, 0, 0))
    sh.putalpha(im.getchannel("A").point(lambda a: a * alpha // 255))
    pad = blur * 3
    big = Image.new("RGBA", (im.width + 2 * pad, im.height + 2 * pad), (0, 0, 0, 0))
    big.alpha_composite(sh, (pad, pad))
    big = big.filter(ImageFilter.GaussianBlur(blur))
    canvas.alpha_composite(big, (xy[0] - pad, xy[1] - pad + blur // 2))
    canvas.alpha_composite(im, xy)


def gradient(size, top, bottom):
    w, h = size
    g = Image.new("RGBA", size)
    d = ImageDraw.Draw(g)
    for y in range(h):
        t = y / max(1, h - 1)
        d.line([(0, y), (w, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    return g


# About logo: transparent 512x512, wolf icon on top, lettering below.
logo = Image.new("RGBA", (512, 512), (0, 0, 0, 0))
ic = scaled(icon, 272, 272)
shadowed(logo, ic, ((512 - 272) // 2, 14), blur=8, alpha=170)
wm = scaled(word, w=470)
logo.alpha_composite(wm, ((512 - wm.width) // 2, 512 - wm.height - 10))
logo.save(P("assets", "bigwave-logo.png"), optimize=True)

# NSIS welcome/finish bitmap 164x314: deep-to-forest gradient, glow, icon, wordmark.
wel = gradient((164, 314), DEEP, FOREST)
glow = Image.new("RGBA", wel.size, (0, 0, 0, 0))
ImageDraw.Draw(glow).ellipse((12, 40, 152, 180), fill=LIME + (70,))
wel.alpha_composite(glow.filter(ImageFilter.GaussianBlur(22)))
ic = scaled(icon, 124, 124)
shadowed(wel, ic, (20, 48), blur=5, alpha=180)
wm = scaled(word, w=148)
wel.alpha_composite(wm, ((164 - wm.width) // 2, 192))
ImageDraw.Draw(wel).rectangle((0, 310, 164, 314), fill=LIME + (255,))
wel.convert("RGB").save(P("installer", "welcome.bmp"))

# NSIS header bitmap 150x57 (right side of the white header strip).
hdr = Image.new("RGBA", (150, 57), (255, 255, 255, 255))
band = gradient((150, 57), FOREST, DEEP)
mask = Image.new("L", (150, 57), 0)
ImageDraw.Draw(mask).polygon([(14, 0), (150, 0), (150, 57), (0, 57)], fill=255)
hdr.paste(band, (0, 0), mask)
ic = scaled(icon, 47, 47)
hdr.alpha_composite(ic, (150 - 47 - 6, 5))
wm = scaled(word, w=80)
hdr.alpha_composite(wm, (150 - 47 - 10 - wm.width, (57 - wm.height) // 2))
hdr.convert("RGB").save(P("installer", "header.bmp"))
print("wrote assets/bigwave-logo.png installer/welcome.bmp installer/header.bmp")
