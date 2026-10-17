"""Draw Hatcheck's icons with Pillow, from the same drawing as the logo: a top hat with a
check cut out of its crown.

  src-tauri/icons/app-icon.png    1024 px: the indigo tile with the margin macOS leaves
                                  around app icons; `tauri icon` makes every size from it
  src-tauri/icons/tray.png        the macOS menu-bar icon, black on transparent, which macOS
                                  tints as a template image
  src-tauri/icons/tray-color.png  the tray icon on Windows and Linux, which show an icon as
                                  it is: the tile, readable on light and dark taskbars

The shapes are in the logo's 64-unit space and the mark sits in the tile as in
site/favicon.svg; change them there and here together."""
import os

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
ICONS = os.path.join(os.path.dirname(HERE), "src-tauri", "icons")

BRIM, BRIM_RADIUS = (10, 44, 54, 52), 4  # <rect x="10" y="44" width="44" height="8" rx="4"/>
CHECK, CHECK_WIDTH = [(23, 29), (30, 36), (42, 21)], 6.5  # M23 29l7 7 12-15, the bolder icon check
IN_TILE = 0.83, 5.5  # the mark's scale and offset in a 64-unit tile: translate(5.5 5.5) scale(0.83)
TOP, BOTTOM = (63, 55, 201), (124, 58, 237)  # the tile's gradient, #3f37c9 to #7c3aed


def quad(p0, c, p1, steps=24):
    """Points along a quadratic Bézier curve, after p0."""
    return [tuple((1 - t) ** 2 * a + 2 * (1 - t) * t * b + t ** 2 * d for a, b, d in zip(p0, c, p1))
            for t in (i / steps for i in range(1, steps + 1))]


def crown():
    """The crown, a little wider at the top, its top corners rounded:
    M20 10h24q3 0 3 3l-2 34H19l-2-34q0-3 3-3z"""
    return ([(20, 10), (44, 10)] + quad((44, 10), (47, 10), (47, 13)) + [(45, 47), (19, 47)]
            + [(17, 13)] + quad((17, 13), (17, 10), (20, 10))[:-1])


def mark_mask(size, scale, dx, dy):
    """The mark as a size×size mask: crown and brim filled, the check cut out, with the
    mark's (x, y) at (dx + scale·x, dy + scale·y)."""
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)

    def at(x, y):
        return dx + scale * x, dy + scale * y

    draw.polygon([at(x, y) for x, y in crown()], fill=255)
    draw.rounded_rectangle([*at(*BRIM[:2]), *at(*BRIM[2:])], radius=scale * BRIM_RADIUS, fill=255)
    width, points = scale * CHECK_WIDTH, [at(x, y) for x, y in CHECK]
    draw.line(points, fill=0, width=round(width), joint="curve")
    for x, y in (points[0], points[-1]):  # round caps
        draw.ellipse([x - width / 2, y - width / 2, x + width / 2, y + width / 2], fill=0)
    return mask


def tile(size, margin, radius, supersample=4, top=TOP, bottom=BOTTOM):
    """The tile, top to bottom from top to bottom colour, with the mark in white and the
    tile showing through its check. margin and radius are fractions of size."""
    s = size * supersample
    m = s * margin
    gradient = Image.new("RGBA", (s, s))
    draw = ImageDraw.Draw(gradient)
    for y in range(s):
        t = min(max((y - m) / (s - 2 * m - 1), 0), 1)
        draw.line([(0, y), (s, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    body = Image.new("L", (s, s), 0)
    ImageDraw.Draw(body).rounded_rectangle([m, m, s - m, s - m], radius=s * radius, fill=255)
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    img.paste(gradient, (0, 0), body)
    unit = (s - 2 * m) / 64
    scale, offset = IN_TILE
    img.paste((255, 255, 255, 255), (0, 0, s, s), mark_mask(s, scale * unit, m + offset * unit, m + offset * unit))
    return img.resize((size, size), Image.LANCZOS)


def app_icon(size=1024):
    return tile(size, margin=0.09, radius=0.19)  # the space and corners macOS gives an icon's body


def tray_icon(size=64, supersample=8):
    """Black on transparent, the hat 88% of the width, for macOS to tint."""
    s = size * supersample
    scale = s * 0.88 / (BRIM[2] - BRIM[0])
    dx = (s - (BRIM[2] - BRIM[0]) * scale) / 2 - BRIM[0] * scale
    dy = (s - (BRIM[3] - 10) * scale) / 2 - 10 * scale  # the mark runs from y 10 to the brim's bottom
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    img.paste((0, 0, 0, 255), (0, 0, s, s), mark_mask(s, scale, dx, dy))
    return img.resize((size, size), Image.LANCZOS)


def tray_color(size=32):
    return tile(size, margin=0, radius=14 / 64, supersample=8)  # the favicon's tile, rx="14"


if __name__ == "__main__":
    os.makedirs(ICONS, exist_ok=True)
    app_icon().save(os.path.join(ICONS, "app-icon.png"))
    tray_icon().save(os.path.join(ICONS, "tray.png"))
    tray_color().save(os.path.join(ICONS, "tray-color.png"))
    print("wrote app-icon.png, tray.png and tray-color.png in", ICONS)
