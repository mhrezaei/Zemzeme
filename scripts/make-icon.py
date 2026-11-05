"""Draw the app icon (1024x1024 PNG) with Pillow.

    python3 scripts/make-icon.py src-tauri/icons/source.png
    npm run tauri icon src-tauri/icons/source.png

A microphone next to a text cursor: speech that ends up where you type.
"""
import sys

from PIL import Image, ImageDraw, ImageFilter

S = 1024
SS = 4  # supersampling for smooth edges
W = S * SS


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def main(out):
    top, bottom = (124, 108, 255), (78, 58, 214)

    # Background gradient.
    grad = Image.new("RGB", (W, W))
    gd = ImageDraw.Draw(grad)
    for y in range(W):
        gd.line([(0, y), (W, y)], fill=lerp(top, bottom, y / W))

    # Rounded-square mask with a margin (macOS-style safe area).
    margin, radius = 100 * SS, 200 * SS
    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).rounded_rectangle([margin, margin, W - margin, W - margin], radius, fill=255)

    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    img.paste(grad, (0, 0), mask)

    # Soft drop shadow under the glyphs.
    glyph = Image.new("L", (W, W), 0)
    g = ImageDraw.Draw(glyph)
    u = SS
    cx = 455 * u
    # Microphone capsule.
    g.rounded_rectangle([cx - 105 * u, 270 * u, cx + 105 * u, 580 * u], 105 * u, fill=255)
    # Holder arc.
    g.arc([cx - 175 * u, 370 * u, cx + 175 * u, 690 * u], 0, 180, fill=255, width=42 * u)
    # Stem and base.
    g.rectangle([cx - 21 * u, 685 * u, cx + 21 * u, 770 * u], fill=255)
    g.rounded_rectangle([cx - 105 * u, 750 * u, cx + 105 * u, 792 * u], 21 * u, fill=255)
    # Text cursor (I-beam) to the right.
    tx = 720 * u
    g.rounded_rectangle([tx - 18 * u, 300 * u, tx + 18 * u, 760 * u], 18 * u, fill=255)
    g.rounded_rectangle([tx - 70 * u, 290 * u, tx + 70 * u, 326 * u], 18 * u, fill=255)
    g.rounded_rectangle([tx - 70 * u, 734 * u, tx + 70 * u, 770 * u], 18 * u, fill=255)

    shadow = glyph.filter(ImageFilter.GaussianBlur(14 * u)).point(lambda v: int(v * 0.35))
    dark = Image.new("RGBA", (W, W), (30, 20, 90, 255))
    img.paste(dark, (0, 10 * u), shadow)
    white = Image.new("RGBA", (W, W), (255, 255, 255, 255))
    img.paste(white, (0, 0), glyph)

    img.resize((S, S), Image.LANCZOS).save(out)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "icon.png")
