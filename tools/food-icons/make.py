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
    "largeleek.png": "draw:leek",         # ふといながねぎ: 自作
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
    "soothingcacao.png": "draw:cacao",     # リラックスカカオ: 自作(カカオポッド)
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


def draw_leek() -> Image.Image:
    """ふといながねぎの自作アイコン(斜め配置、白い茎＋分かれた緑の葉)"""
    im = Image.new("RGBA", (128 * S, 128 * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    outline = (42, 94, 34)

    ux, uy = 1.0 / math.sqrt(2.0), -1.0 / math.sqrt(2.0)  # 左下→右上
    vx, vy = -1.0 / math.sqrt(2.0), -1.0 / math.sqrt(2.0)  # 左上(幅方向)
    bx, by = 27.0, 101.0  # 根元

    def pt(along: float, across: float) -> tuple[float, float]:
        return bx + ux * along + vx * across, by + uy * along + vy * across

    def stroke(fn, n, color_fn, r_fn):
        for i in range(n + 1):
            t = i / n
            cx, cy = fn(t)
            r = r_fn(t) * S
            d.ellipse([cx * S - r, cy * S - r, cx * S + r, cy * S + r], fill=color_fn(t))

    def lerp_rgb(c1, c2, k):
        k = max(0.0, min(1.0, k))
        return tuple(int(c1[i] + (c2[i] - c1[i]) * k) for i in range(3))

    def main_pt(t):
        return pt(12.0 + 86.0 * t, 0.0)

    def main_r(t):
        return 14.5 - 3.5 * t

    def upper_leaf_pt(t):
        return pt(42.0 + 54.0 * t, 3.0 + 15.0 * (t ** 0.9))

    def upper_leaf_r(t):
        return 9.5 - 2.2 * t

    def lower_leaf_pt(t):
        return pt(48.0 + 40.0 * t, -3.0 - 13.0 * (t ** 0.9))

    def lower_leaf_r(t):
        return 8.5 - 2.0 * t

    # 1. 全体の縁取り
    stroke(upper_leaf_pt, 80, lambda _: outline, lambda t: upper_leaf_r(t) + 3.8)
    stroke(lower_leaf_pt, 70, lambda _: outline, lambda t: lower_leaf_r(t) + 3.8)
    stroke(main_pt, 120, lambda _: outline, lambda t: main_r(t) + 3.8)

    # 2. 奥側・手前側の分かれた葉
    stroke(upper_leaf_pt, 80, lambda _: (68, 152, 54), upper_leaf_r)
    stroke(
        lambda t: (upper_leaf_pt(t)[0] + vx * 2.2, upper_leaf_pt(t)[1] + vy * 2.2),
        60,
        lambda _: (108, 192, 76),
        lambda t: upper_leaf_r(t) * 0.48,
    )

    stroke(lower_leaf_pt, 70, lambda _: (64, 146, 50), lower_leaf_r)
    stroke(
        lambda t: (lower_leaf_pt(t)[0] + vx * 1.8, lower_leaf_pt(t)[1] + vy * 1.8),
        50,
        lambda _: (102, 184, 72),
        lambda t: lower_leaf_r(t) * 0.45,
    )

    # 3. メインの茎〜葉
    white = (248, 250, 240)
    lime = (156, 210, 82)
    green = (84, 174, 60)

    def main_color(t):
        if t < 0.36:
            return white
        if t < 0.54:
            return lerp_rgb(white, lime, (t - 0.36) / 0.18)
        if t < 0.68:
            return lerp_rgb(lime, green, (t - 0.54) / 0.14)
        return green

    stroke(main_pt, 140, main_color, main_r)

    # 影とハイライト
    def shadow_color(t):
        if t < 0.36:
            return (222, 232, 214)
        if t < 0.54:
            return lerp_rgb((222, 232, 214), (122, 178, 64), (t - 0.36) / 0.18)
        return (64, 144, 48)

    stroke(
        lambda t: pt(12.0 + 86.0 * t, -main_r(t) * 0.45),
        100,
        shadow_color,
        lambda t: main_r(t) * 0.42,
    )

    def hi_color(t):
        if t < 0.36:
            return (255, 255, 255)
        if t < 0.54:
            return lerp_rgb((255, 255, 255), (188, 232, 112), (t - 0.36) / 0.18)
        return (132, 212, 88)

    stroke(
        lambda t: pt(12.0 + 86.0 * t, main_r(t) * 0.42),
        100,
        hi_color,
        lambda t: main_r(t) * 0.36,
    )

    # 葉の重なり(茎から葉へ分かれる V 字ライン)
    v1 = [pt(44, 13), pt(54, 0), pt(48, -11)]
    d.line([(x * S, y * S) for x, y in v1], fill=(56, 128, 44), width=3 * S)

    # 根元の切り口
    cut_pts = []
    for i in range(36):
        a = 2 * math.pi * i / 36
        px, py = pt(10.0 + 4.5 * math.cos(a), 14.0 * math.sin(a))
        cut_pts.append((px * S, py * S))
    d.polygon(cut_pts, fill=(230, 236, 202), outline=(170, 186, 140), width=2 * S)

    return im.resize((128, 128), Image.LANCZOS)


def draw_cacao() -> Image.Image:
    """リラックスカカオの自作アイコン(縦筋のあるカカオポッド＋小さな葉)"""
    im = Image.new("RGBA", (128 * S, 128 * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    outline = (76, 32, 16)

    cx0, cy0 = 64.0, 68.0
    ang = math.radians(26.0)
    ca, sa = math.cos(ang), math.sin(ang)

    def pod_pt(along: float, across: float) -> tuple[float, float]:
        # along: -1(上・ヘタ側) 〜 +1(下・先端側), across: 左右方向
        x = cx0 + across * ca - along * 44.0 * sa
        y = cy0 + across * sa + along * 44.0 * ca
        return x, y

    def pod_w(t: float) -> float:
        base = max(0.0, 1.0 - t * t) ** 0.62
        return 27.5 * base * (1.0 - 0.14 * t)

    def pod_poly(expand: float = 0.0, shift_x: float = 0.0, scale_w: float = 1.0) -> list[tuple[float, float]]:
        n = 60
        left, right = [], []
        for i in range(n + 1):
            t = -1.0 + 2.0 * i / n
            w = pod_w(t) * scale_w + expand
            lx, ly = pod_pt(t, -w + shift_x)
            rx, ry = pod_pt(t, w + shift_x)
            left.append((lx * S, ly * S))
            right.append((rx * S, ry * S))
        return left + list(reversed(right))

    # 1. 小さな葉(左上・奥側)
    def leaf_poly(expand: float = 0.0) -> list[tuple[float, float]]:
        pts = []
        n = 48
        lang = math.radians(-52.0)
        lca, lsa = math.cos(lang), math.sin(lang)
        lx0, ly0 = pod_pt(-0.92, -16.0)
        for i in range(n):
            th = 2 * math.pi * i / n
            u = math.sin(th) * (10.5 + expand)
            v = -math.cos(th) * (18.5 + expand)
            pts.append(((lx0 + u * lca - v * lsa) * S, (ly0 + u * lsa + v * lca) * S))
        return pts

    d.polygon(leaf_poly(3.6), fill=(38, 86, 30))
    d.polygon(leaf_poly(0.0), fill=(86, 172, 60))
    # 葉脈
    l_start = pod_pt(-0.88, -6.0)
    l_end = pod_pt(-0.96, -28.0)
    d.line([(l_start[0] * S, l_start[1] * S), (l_end[0] * S, l_end[1] * S)], fill=(134, 210, 92), width=2 * S)

    # 2. ヘタ(右上)
    stem_pts = [pod_pt(-0.88, -4.5), pod_pt(-1.18, -3.0), pod_pt(-1.18, 3.0), pod_pt(-0.88, 4.5)]
    d.line([(pod_pt(-0.88, 0.0)[0] * S, pod_pt(-0.88, 0.0)[1] * S), (pod_pt(-1.18, 0.0)[0] * S, pod_pt(-1.18, 0.0)[1] * S)], fill=outline, width=13 * S)
    d.polygon([(x * S, y * S) for x, y in stem_pts], fill=(128, 76, 40))

    # 3. カカオポッド本体の縁取りとベース
    d.polygon(pod_poly(expand=4.0), fill=outline)
    d.polygon(pod_poly(expand=0.0), fill=(156, 78, 42))

    # 左上側の明るい立体面と右下の影面
    d.polygon(pod_poly(expand=0.0, shift_x=-3.5, scale_w=0.76), fill=(182, 98, 54))
    d.polygon(pod_poly(expand=0.0, shift_x=-6.0, scale_w=0.45), fill=(198, 114, 66))

    # 4. 縦のうね(溝とハイライトのペア)
    for k in (-0.56, -0.18, 0.22, 0.60):
        groove_pts = []
        hi_pts = []
        for i in range(41):
            t = -0.88 + 1.76 * i / 40
            w = pod_w(t)
            gx, gy = pod_pt(t, k * w)
            hx, hy = pod_pt(t, k * w - 2.6)
            groove_pts.append((gx * S, gy * S))
            hi_pts.append((hx * S, hy * S))
        d.line(hi_pts, fill=(214, 134, 82), width=2 * S)
        d.line(groove_pts, fill=(102, 44, 20), width=3 * S)

    # 5. 左上のツヤ
    shine_pts = []
    for i in range(25):
        t = -0.58 + 0.72 * i / 24
        sx, sy = pod_pt(t, -pod_w(t) * 0.74)
        shine_pts.append((sx * S, sy * S))
    d.line(shine_pts, fill=(236, 172, 124), width=3 * S)

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
        elif src == "draw:leek":
            im = draw_leek()
        elif src == "draw:cacao":
            im = draw_cacao()
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
