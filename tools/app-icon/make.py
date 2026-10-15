"""アプリのアイコン(icon.png / icon.ico)を自作の絵で作る(crates/pokesle-app/assets/ へ出力)

ゲームの画像は使わず、鍋と星を図形だけで描く。

    python tools/app-icon/make.py
"""
import pathlib

from PIL import Image, ImageDraw

OUT = pathlib.Path(__file__).resolve().parents[2] / "crates" / "pokesle-app" / "assets"
S = 1024  # 縮小して滑らかにするため大きく描く


def star(d, cx, cy, r, fill):
    pts = []
    import math
    for i in range(8):
        a = math.pi / 4 * i - math.pi / 2
        rr = r if i % 2 == 0 else r * 0.38
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    d.polygon(pts, fill=fill)


def draw():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # 背景: 紺の円と金色の縁
    d.ellipse((16, 16, S - 16, S - 16), fill=(255, 196, 64, 255))
    d.ellipse((60, 60, S - 60, S - 60), fill=(30, 36, 56, 255))
    # 鍋の持ち手
    d.rounded_rectangle((170, 470, 260, 540), 30, fill=(220, 226, 240, 255))
    d.rounded_rectangle((764, 470, 854, 540), 30, fill=(220, 226, 240, 255))
    # 鍋の本体
    d.rounded_rectangle((250, 430, 774, 760), 90, fill=(236, 240, 250, 255))
    d.rectangle((250, 430, 774, 560), fill=(236, 240, 250, 255))
    # ふた
    d.rounded_rectangle((230, 395, 794, 450), 27, fill=(255, 150, 80, 255))
    d.ellipse((472, 330, 552, 410), fill=(255, 150, 80, 255))
    # 鍋の中の帯
    d.rectangle((250, 590, 774, 625), fill=(255, 196, 64, 255))
    # 湯気と星
    star(d, 512, 215, 95, (255, 226, 120, 255))
    star(d, 330, 290, 48, (255, 226, 120, 255))
    star(d, 700, 300, 40, (255, 226, 120, 255))
    return img


def main():
    img = draw()
    img.resize((256, 256), Image.LANCZOS).save(OUT / "icon.png")
    img.save(OUT / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])


main()
