//! ポケモン詳細画面の食材アイコンの認識。
//!
//! 本家 `pokemon-status-image-reader.ts` の食材まわり(円の検出・枠の選択・色の分布の比較)の移植。
//! 本家はゲームの食材画像を見本にしているが、ゲーム画像は同梱しないので、見本は画像ではなく
//! 色の分布(24個の数値)だけを `data/food-signature.json` に持つ。比較は色の分布だけで行う。

use std::collections::HashMap;
use std::sync::OnceLock;

use image::{Rgba, RgbaImage, RgbImage};

use crate::status::FoodRecognition;

/// 色の分布の次元(色相12 + 明度6 + 彩度6)
pub const SIGNATURE_LEN: usize = 24;
/// 色の分布を取る前に、アイコンをそろえる大きさ
const SIZE: u32 = 48;
/// 円を探すときの画像の幅(これより大きい画像は縮める)
const SEARCH_WIDTH: u32 = 360;

static SIGNATURE_JSON: &str = include_str!("../data/food-signature.json");

pub type Signature = [f64; SIGNATURE_LEN];

fn template_map() -> &'static HashMap<String, Signature> {
    static MAP: OnceLock<HashMap<String, Signature>> = OnceLock::new();
    MAP.get_or_init(|| {
        let raw: HashMap<String, Vec<f64>> =
            serde_json::from_str(SIGNATURE_JSON).expect("food-signature.json の読み込みに失敗しました");
        raw.into_iter()
            .map(|(name, list)| {
                let mut signature = [0.0; SIGNATURE_LEN];
                signature.copy_from_slice(&list);
                (name, signature)
            })
            .collect()
    })
}

/// 画面の色の判定に使う、0〜1 に直した画素。
struct Pixel {
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
}

impl Pixel {
    fn new(pixel: &Rgba<u8>) -> Self {
        let [red, green, blue, alpha] = pixel.0.map(|v| f64::from(v) / 255.0);
        Pixel { red, green, blue, alpha }
    }

    fn maximum(&self) -> f64 {
        self.red.max(self.green).max(self.blue)
    }

    fn chroma(&self) -> f64 {
        self.maximum() - self.red.min(self.green).min(self.blue)
    }

    /// 円の背景や白地など、アイコンではない画素(見本は透過画像なので判定しない)
    fn is_background(&self, template: bool) -> bool {
        self.alpha < 0.2
            || (!template && self.red > 0.86 && self.green > 0.86 && self.blue > 0.72 && self.chroma() < 0.2)
            || (self.chroma() < 0.08 && self.maximum() > 0.65)
    }
}

/// 48x48 にそろえたアイコンから、色の分布(長さ1に正規化)を作る。
pub fn signature(icon: &RgbaImage, template: bool) -> Signature {
    let mut histogram = [0.0; SIGNATURE_LEN];
    for pixel in icon.pixels() {
        let pixel = Pixel::new(pixel);
        if pixel.is_background(template) {
            continue;
        }
        let (maximum, chroma) = (pixel.maximum(), pixel.chroma());
        let mut hue = 0.0;
        if chroma > 0.0 {
            hue = if maximum == pixel.red {
                ((pixel.green - pixel.blue) / chroma + 6.0) % 6.0
            } else if maximum == pixel.green {
                (pixel.blue - pixel.red) / chroma + 2.0
            } else {
                (pixel.red - pixel.green) / chroma + 4.0
            };
            hue /= 6.0;
        }
        let saturation = if maximum == 0.0 { 0.0 } else { chroma / maximum };
        let weight = 0.25 + saturation;
        histogram[((hue * 12.0).floor() as usize).min(11)] += weight;
        histogram[12 + ((maximum * 6.0).floor() as usize).min(5)] += weight;
        histogram[18 + ((saturation * 6.0).floor() as usize).min(5)] += weight;
    }
    let norm = histogram.iter().map(|v| v * v).sum::<f64>().sqrt();
    let norm = if norm == 0.0 { 1.0 } else { norm };
    histogram.map(|v| v / norm)
}

fn cosine_similarity(left: &Signature, right: &Signature) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

/// 画像の一部(中心と一辺の長さで指定)を SIZE x SIZE に取り出す。
fn crop_icon(image: &RgbaImage, center_x: f64, center_y: f64, side: f64) -> RgbaImage {
    let left = (center_x - side / 2.0).round().max(0.0) as u32;
    let top = (center_y - side / 2.0).round().max(0.0) as u32;
    let width = (side.round() as u32).min(image.width().saturating_sub(left)).max(1);
    let height = (side.round() as u32).min(image.height().saturating_sub(top)).max(1);
    let cropped = image::imageops::crop_imm(image, left, top, width, height).to_image();
    image::imageops::resize(&cropped, SIZE, SIZE, image::imageops::FilterType::Triangle)
}

/// 見本の画像(透過PNG)から色の分布を作る。見本作成ツール用。
pub fn template_signature(icon: &RgbaImage) -> Signature {
    signature(&image::imageops::resize(icon, SIZE, SIZE, image::imageops::FilterType::Triangle), true)
}

#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 名前のポップアップなどで隠れていて、位置だけを推定した枠
    pub occluded: bool,
}

impl Region {
    fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
    fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }
}

/// 食材アイコンの後ろの淡い黄色だけを拾い、白背景やスキル枠の濃い黄色を除く。
fn is_food_circle_color(red: u8, green: u8, blue: u8) -> bool {
    let (red, green, blue) = (i32::from(red), i32::from(green), i32::from(blue));
    red >= 244 && green >= 238 && (190..=244).contains(&blue) && red - green <= 20 && green - blue >= 5
}

/// 淡い黄色の円(食材アイコンの背景)を探す。
fn find_food_circle_regions(image: &RgbImage) -> Vec<Region> {
    let scale = f64::min(1.0, f64::from(SEARCH_WIDTH) / f64::from(image.width()));
    let width = (f64::from(image.width()) * scale).round() as u32;
    let height = (f64::from(image.height()) * scale).round() as u32;
    let small = image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle);
    let (width, height) = (width as usize, height as usize);
    let mut mask: Vec<bool> = small.pixels().map(|p| is_food_circle_color(p[0], p[1], p[2])).collect();

    let mut region_list = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] {
            continue;
        }
        mask[start] = false;
        let mut queue = vec![start];
        let mut queue_index = 0;
        let (mut area, mut min_x, mut min_y, mut max_x, mut max_y) = (0usize, width, height, 0usize, 0usize);
        while queue_index < queue.len() {
            let index = queue[queue_index];
            queue_index += 1;
            let (x, y) = (index % width, index / width);
            area += 1;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            let mut neighbor_list = Vec::with_capacity(4);
            if x > 0 {
                neighbor_list.push(index - 1);
            }
            if x + 1 < width {
                neighbor_list.push(index + 1);
            }
            if y > 0 {
                neighbor_list.push(index - width);
            }
            if y + 1 < height {
                neighbor_list.push(index + width);
            }
            for neighbor in neighbor_list {
                if mask[neighbor] {
                    mask[neighbor] = false;
                    queue.push(neighbor);
                }
            }
        }

        let (w, h) = ((max_x - min_x + 1) as f64, (max_y - min_y + 1) as f64);
        let aspect_ratio = w / h;
        let fill_rate = area as f64 / (w * h);
        if area >= 120
            && w >= 20.0
            && h >= 20.0
            && w <= 75.0
            && h <= 75.0
            && (0.65..=1.5).contains(&aspect_ratio)
            && fill_rate >= 0.25
        {
            region_list.push(Region {
                x: min_x as f64 / scale,
                y: min_y as f64 / scale,
                width: w / scale,
                height: h / scale,
                occluded: false,
            });
        }
    }
    region_list
}

/// 見つかった円のうち、食材欄の3枠(または2枠から1枠目を復元したもの)を選ぶ。
fn select_food_slot_regions(region_list: &[Region]) -> Vec<Region> {
    // レイアウトが上下へ移動しても追従できるよう、絶対座標ではなく円の大きさを基準に同じ行をまとめる。
    let mut sorted: Vec<Region> = region_list.to_vec();
    sorted.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut row_list: Vec<Vec<Region>> = Vec::new();
    for region in sorted {
        let row = row_list.iter_mut().find(|row| {
            let reference = row[0];
            (region.center_y() - reference.center_y()).abs() <= region.height.max(reference.height) * 0.55
        });
        match row {
            Some(row) => row.push(region),
            None => row_list.push(vec![region]),
        }
    }

    let mut candidate_row: Vec<&Vec<Region>> = row_list.iter().filter(|row| row.len() >= 2).collect();
    candidate_row.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].y.total_cmp(&b[0].y)));
    let Some(food_row) = candidate_row.first() else { return Vec::new() };
    let mut food_row: Vec<Region> = (*food_row).clone();
    food_row.sort_by(|a, b| a.x.total_cmp(&b.x));

    let mut slot_list: Vec<Region> = Vec::new();
    // 3枠すべての円が見える場合は、中心間隔が最も均等な3つを食材欄として採用する。
    if food_row.len() >= 3 {
        let interval_gap = |w: &[Region]| {
            ((w[1].center_x() - w[0].center_x()) - (w[2].center_x() - w[1].center_x())).abs()
        };
        let mut best: Option<&[Region]> = None;
        for window in food_row.windows(3) {
            if best.is_none_or(|b| interval_gap(window) < interval_gap(b)) {
                best = Some(window);
            }
        }
        if let Some(best) = best {
            slot_list = best.to_vec();
            let first = slot_list[1].center_x() - slot_list[0].center_x();
            let second = slot_list[2].center_x() - slot_list[1].center_x();
            if (first - second).abs() > first.min(second) * 0.35 {
                slot_list.clear();
            }
        }
    }

    if slot_list.is_empty() {
        // ポップアップ内のポケモン画像を食材と誤認しないよう、食材円に近い間隔で並ぶ右側の2つを選ぶ。
        let average_size =
            food_row.iter().map(|r| r.width.max(r.height)).sum::<f64>() / food_row.len() as f64;
        let pair = food_row
            .windows(2)
            .filter(|w| {
                let interval = w[1].center_x() - w[0].center_x();
                interval >= average_size * 0.8 && interval <= average_size * 1.8
            })
            .last();
        if let Some(pair) = pair {
            slot_list = pair.to_vec();
        }
    }

    if slot_list.len() == 2 {
        // 名前ポップアップで1枠目の円が隠れる場合は、残る2枠の間隔から1枠目の領域だけを復元する。
        let (second, third) = (slot_list[0], slot_list[1]);
        let interval = third.center_x() - second.center_x();
        slot_list = vec![Region { x: second.x - interval, occluded: true, ..second }, second, third];
    }
    slot_list
}

/// 画像の食材欄を探して、3枠ぶんの認識結果(食材ごとの類似度つき)を返す。食材欄が無ければ空。
pub fn recognize_food_list(image: &RgbImage) -> Vec<FoodRecognition> {
    let slot_list = select_food_slot_regions(&find_food_circle_regions(image));
    if slot_list.is_empty() {
        return Vec::new();
    }
    let rgba = image::DynamicImage::ImageRgb8(image.clone()).to_rgba8();

    slot_list
        .iter()
        .map(|region| {
            let side = region.width.max(region.height) * 1.05;
            let icon = crop_icon(&rgba, region.center_x(), region.center_y(), side);
            let source = signature(&icon, false);

            let mut match_list: Vec<(String, f64)> =
                template_map().iter().map(|(name, template)| (name.clone(), cosine_similarity(&source, template))).collect();
            match_list.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));

            // 円周を除いた中央部に有色画素がなければ、アイコンではなく未設定の横線と判断できる。
            let center: Vec<Pixel> = icon
                .enumerate_pixels()
                .filter(|(x, y, _)| {
                    let side = f64::from(SIZE);
                    (f64::from(*x) >= side * 0.2 && f64::from(*x) < side * 0.8)
                        && (f64::from(*y) >= side * 0.2 && f64::from(*y) < side * 0.8)
                })
                .map(|(_, _, p)| Pixel::new(p))
                .collect();
            let rate = |f: &dyn Fn(&Pixel) -> bool| center.iter().filter(|p| f(p)).count() as f64 / center.len() as f64;
            let foreground_rate = rate(&|p| !p.is_background(false));
            let colored_rate = rate(&|p| !p.is_background(false) && p.chroma() >= 0.1);
            let empty = !region.occluded && (foreground_rate < 0.02 || colored_rate < 0.12);

            // 隠れた枠は色の比較が成立しないので、判定せず後段の種族補正に任せる
            let name = match_list.first().filter(|best| !empty && !region.occluded && best.1 >= 0.3).map(|b| b.0.clone());
            FoodRecognition { name, num: None, match_list, empty, occluded: region.occluded }
        })
        .collect()
}

/// 食材欄の各枠のアイコン(SIZE x SIZE に切り出したもの)。並びは `recognize_food_list` と同じ。
/// 利用者が確定した食材の見本として保存するのに使う。
pub fn food_icon_list(image: &RgbImage) -> Vec<RgbaImage> {
    let slot_list = select_food_slot_regions(&find_food_circle_regions(image));
    let rgba = image::DynamicImage::ImageRgb8(image.clone()).to_rgba8();
    slot_list
        .iter()
        .map(|region| crop_icon(&rgba, region.center_x(), region.center_y(), region.width.max(region.height) * 1.05))
        .collect()
}

/// 利用者が確定した食材のアイコン(見本)。この PC にだけ保存する。
#[derive(Clone)]
pub struct FoodSample {
    pub name: String,
    /// 未解放の枠の、薄く表示されたアイコンか(薄いものと通常のものは見た目が大きく違うので分けて比べる)
    pub faded: bool,
    pub icon: RgbaImage,
}

/// Lv が足りず未解放の枠か(2枠目は Lv30、3枠目は Lv60 で解放)。未解放の枠のアイコンは薄く表示される
pub fn food_slot_locked(lv: u32, slot: usize) -> bool {
    match slot {
        1 => lv < 30,
        2 => lv < 60,
        _ => false,
    }
}

/// 見本と同じ食材とみなす類似度の下限。同じ食材どうしは 0.85 以上、違う食材どうしは
/// 薄いアイコンでも 0.8 未満だった(実機のスクリーンショットで確認)
const SAMPLE_THRESHOLD: f64 = 0.82;
/// 通常表示の見本から合成した薄い見本と比べるときの下限。実際の薄いアイコンとの類似度は、
/// 同じ食材で 0.69〜0.81、違う食材で 0.62 以下だった(実機の3組で確認)
const FADED_SYNTHESIS_THRESHOLD: f64 = 0.65;

/// 未解放の枠の薄い表示は、元の色を薄めて背景のクリーム色に重ねたもの(チャンネルごとの濃さと背景色)。
/// 同じ画面に並んだ「通常表示と薄い表示の同じ食材」3組から求めた
const FADE_OPACITY: [f64; 3] = [0.41, 0.35, 0.28];
const FADE_BACKGROUND: [f64; 3] = [243.0, 231.0, 204.0];

/// 通常表示のアイコンから、未解放の枠と同じように薄く表示したアイコンを作る(円の背景はそのまま)。
pub fn fade_icon(icon: &RgbaImage) -> RgbaImage {
    let mut faded = icon.clone();
    for pixel in faded.pixels_mut() {
        let [red, green, blue, _] = pixel.0;
        let is_circle = red > 235 && green > 225 && blue > 190 && red.max(green).max(blue) - red.min(green).min(blue) < 50;
        if is_circle {
            continue;
        }
        for channel in 0..3 {
            let value = FADE_OPACITY[channel] * f64::from(pixel[channel]) + (1.0 - FADE_OPACITY[channel]) * FADE_BACKGROUND[channel];
            pixel[channel] = value.round() as u8;
        }
    }
    faded
}

/// 候補の食材の見本が揃っているとき、1位と2位の類似度にこれだけ差があれば1位に決める。
/// 食材一覧の画面から取り込んだ見本は、詳細画面とアイコンの大きさが少し違うので類似度の値は低めになるが、
/// 実機の14枠では、正解が常に1位で、2位との差は最小 0.036 だった
const RANK_MARGIN: f64 = 0.02;

/// 見本のうち、`icon` にいちばん近い食材(候補 `allowed` の中から)。決められなければ None。
///
/// 候補の全食材に見本があれば、候補の中での順位で決める。足りなければ、類似度が基準以上のものだけを使う。
/// 薄い表示の枠は、通常表示の見本から合成した薄い見本とも比べる(薄い表示の見本は集まりにくいため)。
pub fn match_sample(icon: &RgbaImage, faded: bool, allowed: &[String], sample_list: &[FoodSample]) -> Option<String> {
    // 候補の食材ごとの、いちばん近い見本との類似度
    let score_of = |sample: &FoodSample| {
        if sample.faded == faded {
            Some(pixel_similarity(icon, &sample.icon))
        } else if faded {
            Some(pixel_similarity(icon, &fade_icon(&sample.icon)))
        } else {
            None
        }
    };
    let mut ranking: Vec<(&String, Option<f64>)> = allowed
        .iter()
        .map(|name| {
            let best = sample_list.iter().filter(|s| &s.name == name).filter_map(score_of).reduce(f64::max);
            (name, best)
        })
        .collect();
    if ranking.len() >= 2 && ranking.iter().all(|(_, score)| score.is_some()) {
        ranking.sort_by(|a, b| b.1.unwrap_or(0.0).total_cmp(&a.1.unwrap_or(0.0)));
        let (first, second) = (ranking[0].1.unwrap_or(0.0), ranking[1].1.unwrap_or(0.0));
        return (first - second >= RANK_MARGIN).then(|| ranking[0].0.clone());
    }


    // 基準をどれだけ上回ったか(合成の見本は基準が違うので、上回った幅で比べる)
    let mut best: Option<(&str, f64)> = None;
    for sample in sample_list.iter().filter(|s| allowed.contains(&s.name)) {
        let margin = if sample.faded == faded {
            pixel_similarity(icon, &sample.icon) - SAMPLE_THRESHOLD
        } else if faded {
            pixel_similarity(icon, &fade_icon(&sample.icon)) - FADED_SYNTHESIS_THRESHOLD
        } else {
            continue;
        };
        if best.is_none_or(|(_, b)| margin > b) {
            best = Some((&sample.name, margin));
        }
    }
    best.filter(|(_, margin)| *margin >= 0.0).map(|(name, _)| name.to_string())
}

/// 見本と比べるときに試す不透明度。未解放の枠のアイコンは半透明で薄く表示されるため
const SAMPLE_OPACITY_LIST: [f64; 5] = [1.0, 0.75, 0.5, 0.35, 0.25];

/// アイコンどうしの、形と色の近さ(本家 `getPixelSimilarity` と同じ考え方)。
///
/// 見本を白に向けて薄めた場合も比べ、いちばん合う不透明度を使う。検出位置の数pxの揺れに備えて、
/// 上下左右に少しずらした中で最も合う位置を採る。
pub fn pixel_similarity(source: &RgbaImage, sample: &RgbaImage) -> f64 {
    let side = SIZE as i32;
    let pixels = |image: &RgbaImage| -> Vec<(Pixel, bool)> {
        image
            .pixels()
            .map(|p| {
                let pixel = Pixel::new(p);
                let foreground = !pixel.is_background(false);
                (pixel, foreground)
            })
            .collect()
    };
    let source = pixels(source);
    let sample = pixels(sample);
    let source_foreground = source.iter().filter(|p| p.1).count();
    let sample_foreground = sample.iter().filter(|p| p.1).count();
    let denominator = ((source_foreground * sample_foreground).max(1) as f64).sqrt();

    let score_list: Vec<f64> = SAMPLE_OPACITY_LIST
        .iter()
        .map(|&opacity| {
            let mut best = 0.0f64;
            for offset_y in [-3, 0, 3] {
                for offset_x in [-3, 0, 3] {
                    let mut matched = 0.0;
                    for (index, (pixel, foreground)) in source.iter().enumerate() {
                        if !foreground {
                            continue;
                        }
                        let (x, y) = (index as i32 % side + offset_x, index as i32 / side + offset_y);
                        if x < 0 || x >= side || y < 0 || y >= side {
                            continue;
                        }
                        let (other, other_foreground) = &sample[(y * side + x) as usize];
                        if !other_foreground {
                            continue;
                        }
                        let fade = |v: f64| 1.0 - (1.0 - v) * opacity;
                        let distance = ((pixel.red - fade(other.red)).abs()
                            + (pixel.green - fade(other.green)).abs()
                            + (pixel.blue - fade(other.blue)).abs())
                            / 3.0;
                        matched += f64::max(0.0, 1.0 - distance * 2.5);
                    }
                    best = best.max(matched / denominator);
                }
            }
            best
        })
        .collect();
    let max = score_list.iter().copied().fold(0.0, f64::max);
    score_list[0] * 0.25 + max * 0.75
}

/// 食材の一覧画面(ゲームのボックスの絞り込みなど、食材アイコンの下に名前が並ぶ画面)から、
/// 全食材の通常表示の見本を作る。アイコンの円を探し、そのすぐ下にある名前で食材を決める。
pub fn samples_from_food_list_screen(image: &RgbImage, line_list: &[crate::engine::OcrLine]) -> Vec<FoodSample> {
    let master = pokesle_core::data::master();
    let rgba = image::DynamicImage::ImageRgb8(image.clone()).to_rgba8();
    let mut sample_list: Vec<FoodSample> = Vec::new();
    for region in find_food_circle_regions(image) {
        // 円の真下(円の高さの 8 割まで)にある行をつなげて名前にする(長い名前は2行になる)
        let mut below: Vec<&crate::engine::OcrLine> = line_list
            .iter()
            .filter(|line| {
                let center_x = (line.x0 + line.x1) / 2.0;
                line.y0 >= region.y + region.height * 0.9
                    && line.y0 <= region.y + region.height * 1.8
                    && (center_x - region.center_x()).abs() <= region.width * 0.8
            })
            .collect();
        below.sort_by(|a, b| a.y0.total_cmp(&b.y0));
        let text: String = below.iter().map(|line| line.text.as_str()).collect();
        if text.is_empty() {
            continue;
        }
        let best = master
            .foods
            .iter()
            .map(|food| (food.name.as_str(), crate::status::text_similarity(&food.name, &text)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((name, score)) = best.filter(|(_, score)| *score >= 0.7) else { continue };
        let _ = score;
        if sample_list.iter().any(|s| s.name == name) {
            continue;
        }
        let side = region.width.max(region.height) * 1.05;
        sample_list.push(FoodSample {
            name: name.to_string(),
            faded: false,
            icon: crop_icon(&rgba, region.center_x(), region.center_y(), side),
        });
    }
    sample_list
}

/// 見本をフォルダから読み込む。ファイル名は「表示状態(0: 通常, 1: 薄い)_時刻_食材名.png」
pub fn load_food_samples(dir: &std::path::Path) -> Vec<FoodSample> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            let mut parts = stem.splitn(3, '_');
            let faded = parts.next()? == "1";
            let _time = parts.next()?;
            let name = parts.next()?.to_string();
            let icon = image::open(&path).ok()?.to_rgba8();
            Some(FoodSample { name, faded, icon })
        })
        .collect()
}

/// 見本をフォルダに保存する。同じ食材・表示状態の見本が `limit` を超えたら、古いものから消す。
pub fn save_food_sample(dir: &std::path::Path, sample: &FoodSample, limit: usize) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let state = if sample.faded { "1" } else { "0" };
    sample.icon.save(dir.join(format!("{state}_{time}_{}.png", sample.name))).map_err(|e| e.to_string())?;

    // 時刻は桁がそろうので、ファイル名の順が古い順になる
    let prefix = format!("{state}_");
    let suffix = format!("_{}.png", sample.name);
    let mut same: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix) && n.ends_with(&suffix))
        })
        .collect();
    same.sort();
    for old in same.iter().take(same.len().saturating_sub(limit)) {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}
