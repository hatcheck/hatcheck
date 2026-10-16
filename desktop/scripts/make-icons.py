"""Draw Hatcheck's icons with Pillow: src-tauri/icons/app-icon.png (1024 px, which
`tauri icon` turns into every size) and src-tauri/icons/tray.png (the menu-bar icon,
black on transparent, which macOS tints as a template image). A top hat, as in a
cloakroom's hat check."""
import os

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
ICONS = os.path.join(os.path.dirname(HERE), "src-tauri", "icons")


def hat(draw, box, color, band=None):
    """A top hat filling box (x0, y0, x1, y1): a brim, a crown and a band across it."""
    x0, y0, x1, y1 = box
    w, h = x1 - x0, y1 - y0
    brim = h * 0.17
    crown_w = w * 0.58
    cx0 = x0 + (w - crown_w) / 2
    draw.rounded_rectangle([cx0, y0, cx0 + crown_w, y1 - brim * 0.5], radius=w * 0.07, fill=color)
    draw.rounded_rectangle([x0, y1 - brim, x1, y1], radius=brim / 2, fill=color)
    if band is not None:
        top = y1 - brim * 0.5 - h * 0.30
        draw.rectangle([cx0, top, cx0 + crown_w, top + h * 0.12], fill=band)


def app_icon(size=1024, scale=2):
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    # A rounded square with a vertical indigo-to-violet gradient, as macOS icons are drawn.
    gradient = Image.new("RGBA", (s, s))
    top, bottom = (63, 55, 201), (124, 58, 237)
    gd = ImageDraw.Draw(gradient)
    for y in range(s):
        t = y / (s - 1)
        gd.line([(0, y), (s, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    mask = Image.new("L", (s, s), 0)
    margin = s * 0.09  # the space macOS leaves around an icon's body
    ImageDraw.Draw(mask).rounded_rectangle([margin, margin, s - margin, s - margin], radius=s * 0.19, fill=255)
    img.paste(gradient, (0, 0), mask)
    draw = ImageDraw.Draw(img)
    hat(draw, (s * 0.24, s * 0.26, s * 0.76, s * 0.70), (255, 255, 255, 255), band=(245, 179, 66, 255))
    return img.resize((size, size), Image.LANCZOS)


def tray_icon(size=64, scale=8):
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    hat(draw, (s * 0.06, s * 0.14, s * 0.94, s * 0.86), (0, 0, 0, 255), band=(0, 0, 0, 0))
    return img.resize((size, size), Image.LANCZOS)


if __name__ == "__main__":
    os.makedirs(ICONS, exist_ok=True)
    app_icon().save(os.path.join(ICONS, "app-icon.png"))
    tray_icon().save(os.path.join(ICONS, "tray.png"))
    print("wrote app-icon.png and tray.png in", ICONS)
