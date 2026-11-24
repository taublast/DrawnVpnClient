"""Net Connect repo banner, docs/banner.png: 1280x640 (also the GitHub social preview size), drawn at
2x and scaled down. Uses the Android launcher icon, docs/screenshot-android.png (only its top: the
server list stays out) and Windows' Segoe UI. Run: python dev/make_banner.py"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "docs" / "banner.png"
S = 2
W, H = 1280 * S, 640 * S
BG = (11, 18, 32)  # the app's background, #0B1220
TEXT, MUTED = (232, 238, 247), (138, 151, 173)
ACCENT = (59, 130, 246)  # the app's blue, #3B82F6
GREEN, AMBER, GRAY = (34, 197, 94), (245, 158, 11), (138, 151, 173)


def font(name, size):
    return ImageFont.truetype(rf"C:\Windows\Fonts\{name}", size * S)


img = Image.new("RGB", (W, H), BG)

# Soft glows: the connected core's green behind the phone, the app's blue and the connecting amber.
glow = Image.new("RGB", (W, H), BG)
g = ImageDraw.Draw(glow)
for (x, y, r, c) in [(960, 300, 260, GREEN), (1240, 40, 200, ACCENT), (700, 620, 200, AMBER)]:
    g.ellipse([(x - r) * S, (y - r) * S, (x + r) * S, (y + r) * S], fill=tuple(int(v * 0.45 + b * 0.55) for v, b in zip(c, BG)))
img = Image.blend(img, glow.filter(ImageFilter.GaussianBlur(120 * S)), 0.7)
d = ImageDraw.Draw(img)

# Left: icon, name, promise.
icon = Image.open(REPO / "android" / "res" / "mipmap-xxxhdpi" / "ic_launcher.png").convert("RGBA").resize((104 * S, 104 * S), Image.LANCZOS)
img.paste(icon, (72 * S, 100 * S), icon)
d.text((72 * S, 222 * S), "Net Connect", font=font("segoeuib.ttf", 64), fill=TEXT)
d.text((74 * S, 308 * S), "VLESS + REALITY client", font=font("segoeuib.ttf", 40), fill=ACCENT)
d.text((74 * S, 382 * S), "Your own server, one tap away.", font=font("segoeui.ttf", 28), fill=TEXT)
d.text((74 * S, 436 * S), "XTLS Vision + REALITY on Xray-core, the whole device tunneled.", font=font("segoeui.ttf", 20), fill=MUTED)
d.text((74 * S, 464 * S), "Paste a vless:// link and connect.", font=font("segoeui.ttf", 20), fill=MUTED)

# The platforms.
x = 76
f = font("seguisb.ttf", 18)
for label in ["Android", "iPhone", "Mac", "Windows"]:
    d.ellipse([x * S, 536 * S, (x + 12) * S, 548 * S], fill=GREEN)
    d.text(((x + 20) * S, 530 * S), label, font=f, fill=MUTED)
    x += 20 + int(d.textlength(label, font=f) / S) + 32

# Right: the real phone screen, connected, bleeding off the bottom edge.
shot = Image.open(REPO / "docs" / "screenshot-android.png").convert("RGBA")
shot = shot.crop((0, 0, shot.width, min(1290, shot.height)))
width = 500 * S
shot = shot.resize((width, int(shot.height * width / shot.width)), Image.LANCZOS)
px, py = 720 * S, H - shot.height
mask = Image.new("L", shot.size, 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, shot.width - 1, shot.height + 60 * S], radius=36 * S, fill=255)
shadow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
ImageDraw.Draw(shadow).rounded_rectangle([px, py + 14 * S, px + shot.width, py + shot.height + 60 * S], radius=40 * S, fill=(0, 0, 0, 170))
img = Image.alpha_composite(img.convert("RGBA"), shadow.filter(ImageFilter.GaussianBlur(26 * S)))
img.paste(shot, (px, py), mask)
ImageDraw.Draw(img).rounded_rectangle([px, py, px + shot.width - 1, py + shot.height + 60 * S], radius=36 * S, outline=(38, 50, 74), width=3 * S)

img.convert("RGB").resize((W // S, H // S), Image.LANCZOS).save(OUT, optimize=True)
print("saved", OUT)
