"""食材アイコンを作る(crates/pokesle-app/assets/food/ へ出力)

本家の食材画像はゲームの画像なので使わず、Noto Emoji(OFL-1.1)と自作の絵で置き換える。
ファイル名は本家の画像名(master.json の foods[].img)に合わせる。本家に食材が増えたら
MAPPING に1行足して実行し直す。

    python tools/food-icons/make.py
"""
import colorsys
import io
import math
import pathlib
import urllib.request

from PIL import Image, ImageDraw

# 取得元を固定する(Noto Emoji のコミット)
NOTO_COMMIT = "e20cbc2bbec1926686be9f9bee7d1d2cfa1fea0e"
NOTO_URL = f"https://raw.githubusercontent.com/googlefonts/noto-emoji/{NOTO_COMMIT}"
SIZE = 72
OUT = pathlib.Path(__file__).resolve().parents[2] / "crates" / "pokesle-app" / "assets" / "food"

# 本家の画像名 → Noto のコードポイント、または自作("draw:...")
MAPPING = {
    "largeleek.png": "1f96c",            # ふといながねぎ: 🥬
    "tastymushroom.png": "1f344_200d_1f7eb",  # あじわいキノコ: 🍄‍🟫
    "fancyegg.png": "white:1f95a",       # とくせんエッグ: 🥚 を白く塗り替え
    "softpotato.png": "1f954",           # ほっこりポテト: 🥔
    "fancyapple.png": "1f34e",           # とくせんリンゴ: 🍎
    "fieryherb.png": "1f336",            # げきからハーブ: 🌶
    "beansausage.png": "1f356",          # マメミート: 🍖
    "moomoomilk.png": "draw:milk",       # モーモーミルク: 自作(牛乳パック)
    "honey.png": "1f36f",                # あまいミツ: 🍯
    "pureoil.png": "draw:oil",           # ピュアなオイル: 自作
    "warmingginger.png": "1fada",        # あったかジンジャー: 🫚
    "snoozytomato.png": "1f345",         # あんみんトマト: 🍅
    "soothingcacao.png": "1f36b",        # リラックスカカオ: 🍫
    "slowpoketail.png": "draw:tail",     # おいしいシッポ: 自作
    "greengrasssoybeans.png": "cream:1fad8",  # ワカクサ大豆: 🫘 を本家に近いクリーム色に塗り替え
    "greengrasscorn.png": "1f33d",       # ワカクサコーン: 🌽
    "coffee.png": "2615",                # めざましコーヒー: ☕
    "pumpkin.png": "1f383",              # ずっしりカボチャ: 🎃
    "avocado.png": "1f951",              # つやつやアボカド: 🥑
    "nuts.png": "draw:nuts",             # ひとくちナッツ: 自作(ミックスナッツ)
}


def fetch(path: str) -> bytes:
    req = urllib.request.Request(f"{NOTO_URL}/{path}", headers={"User-Agent": "pokesle-rs food-icons"})
    return urllib.request.urlopen(req).read()


def noto(code: str) -> Image.Image:
    return Image.open(io.BytesIO(fetch(f"2D/png/128/emoji_u{code}.png"))).convert("RGBA")


def whiten(im: Image.Image) -> Image.Image:
    """茶色の卵を白い卵にする(彩度を落として明るくする)"""
    px = im.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
            r2, g2, b2 = colorsys.hls_to_rgb(0.11, min(1, 0.55 + l * 0.5), s * 0.25)
            px[x, y] = (int(r2 * 255), int(g2 * 255), int(b2 * 255), a)
    return im


def cream(im: Image.Image) -> Image.Image:
    """赤い豆を本家の大豆に近いクリーム色にする(色相 0.1、明るめ)"""
    px = im.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
            r2, g2, b2 = colorsys.hls_to_rgb(0.1, min(1, 0.5 + l * 0.6), 0.85)
            px[x, y] = (int(r2 * 255), int(g2 * 255), int(b2 * 255), a)
    return im


def rehue(im: Image.Image, hue: float) -> Image.Image:
    """色のついた部分の色相を hue(0～1)に揃える。白や灰色の部分はそのまま"""
    px = im.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
            if s < 0.15:
                continue
            r2, g2, b2 = colorsys.hls_to_rgb(hue, l, s)
            px[x, y] = (int(r2 * 255), int(g2 * 255), int(b2 * 255), a)
    return im


S = 4  # 4倍で描いて縮小する


def draw_oil() -> Image.Image:
    im = Image.new("RGBA", (128 * S, 128 * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    s = lambda *v: [x * S for x in v]
    d.rounded_rectangle(s(52, 6, 76, 26), radius=4 * S, fill=(176, 122, 72))  # コルク
    d.rectangle(s(52, 20, 76, 26), fill=(150, 100, 58))
    d.rounded_rectangle(s(48, 22, 80, 52), radius=6 * S, fill=(225, 240, 215, 235))  # 首
    d.rounded_rectangle(s(26, 44, 102, 122), radius=20 * S, fill=(225, 240, 215, 235))  # 胴
    d.rounded_rectangle(s(31, 62, 97, 117), radius=16 * S, fill=(232, 190, 40))  # 油
    d.rounded_rectangle(s(31, 62, 97, 74), radius=6 * S, fill=(245, 212, 70))
    d.rounded_rectangle(s(40, 80, 88, 106), radius=6 * S, fill=(250, 248, 235))  # ラベル
    d.ellipse(s(50, 86, 64, 99), fill=(110, 150, 60))
    d.ellipse(s(62, 86, 78, 99), fill=(90, 130, 50))
    d.rounded_rectangle(s(34, 52, 42, 110), radius=4 * S, fill=(255, 255, 255, 140))  # つや
    return im.resize((128, 128), Image.LANCZOS)


def draw_milk() -> Image.Image:
    """屋根形の牛乳パック"""
    im = Image.new("RGBA", (128 * S, 128 * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    s = lambda *v: [x * S for x in v]
    d.polygon(s(36, 40, 64, 16, 92, 40), fill=(205, 222, 240))  # 屋根(前)
    d.polygon(s(64, 16, 92, 40, 100, 34, 72, 12), fill=(170, 195, 225))  # 屋根(横)
    d.rectangle(s(64, 6, 72, 16), fill=(225, 235, 245))  # 注ぎ口のつまみ
    d.polygon(s(92, 40, 100, 34, 100, 112, 92, 120), fill=(190, 210, 235))  # 側面
    d.rectangle(s(36, 40, 92, 120), fill=(250, 252, 255))  # 正面
    d.rectangle(s(36, 70, 92, 104), fill=(60, 120, 210))  # 青い帯
    # 白い M(フォントを使わず図形で描く)
    d.polygon(s(48, 98, 48, 76, 55, 76, 64, 88, 73, 76, 80, 76, 80, 98, 74, 98, 74, 86, 64, 98, 54, 86, 54, 98), fill=(255, 255, 255))
    d.rectangle(s(36, 40, 40, 120), fill=(230, 238, 248))  # 端の陰
    return im.resize((128, 128), Image.LANCZOS)


def draw_tail() -> Image.Image:
    im = Image.new("RGBA", (128 * S, 128 * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)

    def arc(color, wscale, t0, t1, r=36, width=lambda t: 22 - 8 * t):
        n = 200
        for i in range(n + 1):
            t = t0 + (t1 - t0) * i / n
            a = math.radians(200 - 250 * t)
            cx, cy = 64 * S + r * S * math.cos(a), 62 * S - r * S * math.sin(a)
            w = width(t) * S * wscale
            d.ellipse([cx - w, cy - w, cx + w, cy + w], fill=color)

    arc((120, 60, 80), 1.12, 0, 1)  # 縁取り
    arc((246, 160, 190), 1.0, 0, 0.78)  # ピンク
    arc((255, 250, 245), 1.0, 0.78, 1)  # 白い先
    arc((255, 215, 228), 1.0, 0.05, 0.55, r=44, width=lambda t: 3)  # つや
    return im.resize((128, 128), Image.LANCZOS)


def draw_nuts() -> Image.Image:
    """ミックスナッツ(カシューナッツ・アーモンド・ピーナッツ)の自作アイコン"""
    im = Image.new("RGBA", (128 * S, 128 * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    outline = (135, 68, 28)

    def stroke_path(pts_fn, n, r_fn, color):
        for i in range(n + 1):
            t = i / n
            cx, cy = pts_fn(t)
            r = r_fn(t) * S
            d.ellipse([cx * S - r, cy * S - r, cx * S + r, cy * S + r], fill=color)

    # カシューナッツ(上・奥)
    def cashew_pt(t):
        a = math.radians(170 - 205 * t)
        return 60.0 + 18.0 * math.cos(a), 50.0 - 16.5 * math.sin(a)

    def cashew_r(t):
        return 12.0 + 4.5 * math.sin(math.pi * t)

    stroke_path(cashew_pt, 120, lambda t: cashew_r(t) + 4.0, outline)
    stroke_path(cashew_pt, 120, cashew_r, (242, 192, 92))
    stroke_path(
        lambda t: (cashew_pt(t)[0] - 1.5, cashew_pt(t)[1] - 2.0),
        100,
        lambda t: cashew_r(t) * 0.68,
        (253, 224, 148),
    )

    # アーモンド(左下)
    def almond_poly(scale_r=1.0, offset=(0.0, 0.0)):
        pts = []
        n = 80
        ang = math.radians(-35)
        ca, sa = math.cos(ang), math.sin(ang)
        for i in range(n):
            th = 2 * math.pi * i / n
            u = math.sin(th) * (1.0 - 0.28 * math.cos(th)) * 22.0 * scale_r
            v = -math.cos(th) * 33.0 * scale_r
            x = 41.0 + offset[0] + u * ca - v * sa
            y = 75.0 + offset[1] + u * sa + v * ca
            pts.append((x * S, y * S))
        return pts

    d.polygon(almond_poly(1.15), fill=outline)
    d.polygon(almond_poly(1.0), fill=(214, 118, 52))
    d.polygon(almond_poly(0.75, (-1.8, -1.8)), fill=(228, 144, 76))
    ang = math.radians(-35)
    ca, sa = math.cos(ang), math.sin(ang)
    for shift in [-7.0, 0.0, 7.0]:
        line_pts = []
        for k in range(11):
            t = -0.62 + 1.24 * k / 10
            u = shift * (1.0 - 0.35 * t * t)
            v = t * 22.0
            line_pts.append(((41.0 + u * ca - v * sa) * S, (75.0 + u * sa + v * ca) * S))
        d.line(line_pts, fill=(172, 82, 32), width=2 * S)

    # ピーナッツ(右下・手前)
    def peanut_pt(t):
        return 95.0 - 33.0 * t, 48.0 + 44.0 * t

    def peanut_r(t):
        b1 = math.exp(-((t - 0.2) ** 2) / 0.07)
        b2 = math.exp(-((t - 0.8) ** 2) / 0.07)
        return 11.5 + 9.2 * (b1 + b2)

    stroke_path(peanut_pt, 120, lambda t: peanut_r(t) + 4.0, outline)
    stroke_path(peanut_pt, 120, peanut_r, (234, 178, 86))
    stroke_path(
        lambda t: (peanut_pt(t)[0] - 2.0, peanut_pt(t)[1] - 2.0),
        100,
        lambda t: peanut_r(t) * 0.72,
        (249, 214, 142),
    )

    spots = [
        (88, 50), (96, 55), (84, 60), (92, 65),
        (78, 70), (74, 80), (66, 84), (72, 92), (60, 93),
    ]
    for sx, sy in spots:
        r = 2.6 * S
        d.ellipse([sx * S - r, sy * S - r, sx * S + r, sy * S + r], fill=(198, 138, 56))
    return im.resize((128, 128), Image.LANCZOS)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, src in MAPPING.items():
        if src == "draw:oil":
            im = draw_oil()
        elif src == "draw:milk":
            im = draw_milk()
        elif src == "draw:tail":
            im = draw_tail()
        elif src == "draw:nuts":
            im = draw_nuts()
        elif src.startswith("cream:"):
            im = cream(noto(src.removeprefix("cream:")))
        elif src.startswith("hue:"):
            _, hue, code = src.split(":")
            im = rehue(noto(code), float(hue))
        elif src.startswith("white:"):
            im = whiten(noto(src.removeprefix("white:")))
        else:
            im = noto(src)
        im.resize((SIZE, SIZE), Image.LANCZOS).save(OUT / name, optimize=True)
        print(name)
    (OUT / "LICENSE-NOTO-EMOJI.txt").write_bytes(fetch("LICENSE"))


if __name__ == "__main__":
    main()
