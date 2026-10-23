//! ポケモン詳細画面のスクリーンショット(複数枚)を読み取って、ポケモンの情報にする。
//!
//! 詳細画面は縦に長く、1画面に収まらない。画面ごとの画像を縦に並べた1枚のページとして扱う
//! (2枚目の座標は、1枚目の高さだけ下にずらす)ので、本家の1枚ものの解釈をそのまま使える。

use image::RgbImage;

use crate::engine::{OcrEngine, OcrLine, merge_same_row};
use crate::food::recognize_food_list;
use crate::status::{FoodRecognition, find_candy_line};
use crate::status::{StatusLine, StatusResult, find_candy_name, parse_data_with_candy};

/// 画像を、本家の基準に合わせた幅にそろえる。細い文字を認識しやすい大きさに揃えつつ、
/// 端末の解像度に比例して処理が重くなるのも防ぐ。
const IMAGE_WIDTH: u32 = 1080;
/// 同じ高さに並んだ行をつなぐときの、隙間の上限(行の高さの何倍まで)
const MERGE_GAP_RATIO: f64 = 2.5;

/// 認識済みの1画面。
pub struct Screen {
    pub image: RgbImage,
    pub line_list: Vec<OcrLine>,
    /// この画面に食材欄があれば、その3枠の認識結果
    pub food_list: Vec<FoodRecognition>,
}

/// 明るさが基準より暗い画素だけを黒、それ以外を白にした画像。
/// ゲーム画面の文字は背景より暗いので、色付きの枠やアイコンを概ね除いて文字だけが残る。
fn binarize(image: &RgbImage) -> RgbImage {
    let mut result = image.clone();
    for pixel in result.pixels_mut() {
        let [red, green, blue] = pixel.0.map(f64::from);
        let value = if red * 0.299 + green * 0.587 + blue * 0.114 < 180.0 { 0 } else { 255 };
        pixel.0 = [value; 3];
    }
    result
}

fn encode_png(image: &RgbImage) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png)
        .map_err(|e| format!("画像を変換できませんでした: {e}"))?;
    Ok(encoded)
}

/// 画像(PNG)を読み込み、幅をそろえて、エンジンで文字を認識する。
pub fn recognize_screen(engine: &dyn OcrEngine, png: &[u8]) -> Result<Screen, String> {
    let image = image::load_from_memory(png).map_err(|e| format!("画像を読み込めませんでした: {e}"))?.to_rgb8();
    let (image, png) = if image.width() == IMAGE_WIDTH {
        (image, png.to_vec())
    } else {
        let height = (u64::from(image.height()) * u64::from(IMAGE_WIDTH) / u64::from(image.width())).max(1) as u32;
        let resized = image::imageops::resize(&image, IMAGE_WIDTH, height, image::imageops::FilterType::Lanczos3);
        let encoded = encode_png(&resized)?;
        (resized, encoded)
    };
    let mut line_list = merge_same_row(&engine.recognize(&png)?, MERGE_GAP_RATIO);
    if engine.needs_binarized_pass() {
        // 元の画像で読めなかった色付きの文字を拾うため、文字だけの画像の結果も足す(重なる行はそのまま残す)
        let binarized = engine.recognize(&encode_png(&binarize(&image))?)?;
        line_list.extend(merge_same_row(&binarized, MERGE_GAP_RATIO));
    }
    if engine.needs_binarized_pass() {
        line_list.extend(recognize_header(engine, &image, &line_list)?);
    }
    let food_list = recognize_food_list(&image);
    Ok(Screen { image, line_list, food_list })
}

/// 名前の行(「Lv. 22 キテルグマ」)を、切り出して拡大した画像で認識し直す。
///
/// 画像全体では緑の「Lv.」を読み落とすことがあり、食材の未解放ラベル(「Lv. 60」など)を
/// ポケモンのLvと取り違える原因になる。名前の行は「○○のアメ」の少し上にあるので、そこだけを読む。
fn recognize_header(engine: &dyn OcrEngine, image: &RgbImage, line_list: &[OcrLine]) -> Result<Vec<OcrLine>, String> {
    const SCALE: u32 = 2;
    let status_line_list: Vec<StatusLine> = line_list.iter().map(StatusLine::from).collect();
    let Some((_, candy_line)) = find_candy_line(&status_line_list) else { return Ok(Vec::new()) };

    // アメの行の 190〜40px 上が名前の行(1080px 幅の画像で)
    let top = (candy_line.y0 - 190.0).max(0.0) as u32;
    let bottom = ((candy_line.y0 - 40.0).max(0.0) as u32).min(image.height());
    let (left, right) = (100.min(image.width() / 2), image.width().saturating_sub(100));
    if bottom <= top || right <= left {
        return Ok(Vec::new());
    }
    let cropped = image::imageops::crop_imm(image, left, top, right - left, bottom - top).to_image();
    let scaled = image::imageops::resize(&cropped, cropped.width() * SCALE, cropped.height() * SCALE, image::imageops::FilterType::Lanczos3);
    let lines = engine.recognize(&encode_png(&binarize(&scaled))?)?;
    let scale = f64::from(SCALE);
    Ok(lines
        .into_iter()
        .map(|line| OcrLine {
            text: line.text,
            x0: line.x0 / scale + f64::from(left),
            y0: line.y0 / scale + f64::from(top),
            x1: line.x1 / scale + f64::from(left),
            y1: line.y1 / scale + f64::from(top),
        })
        .collect())
}

/// 行の周りの色から、サブスキルの枠のレア度(1: 白、2: 青、3: 金)を調べる。
fn sub_skill_rarity(image: &RgbImage, line: &OcrLine) -> u32 {
    let padding = 8.0;
    let min_x = (line.x0 - padding).floor().max(0.0) as u32;
    let max_x = ((line.x1 + padding).ceil() as u32).min(image.width() - 1);
    let min_y = (line.y0 - padding).floor().max(0.0) as u32;
    let max_y = ((line.y1 + padding).ceil() as u32).min(image.height() - 1);

    let (mut gold, mut blue, mut sample) = (0u32, 0u32, 0u32);
    for y in (min_y..=max_y).step_by(3) {
        for x in (min_x..=max_x).step_by(3) {
            let [red, green, blue_value] = image.get_pixel(x, y).0.map(i32::from);
            sample += 1;
            // 未解放の金枠は白に近い淡色になるため、明度ではなく青成分との差を中心に判定する。
            if red >= 185 && green >= 155 && red - blue_value >= 20 && green - blue_value >= 5 {
                gold += 1;
            }
            if green >= 205 && blue_value >= 215 && blue_value - red >= 8 {
                blue += 1;
            }
        }
    }
    // 金・青を検出できないスキル枠は白として扱う。文字類似度も併用するため、通常の白背景文は候補になりにくい。
    let threshold = f64::from(sample) * 0.08;
    if f64::from(gold) >= threshold {
        3
    } else if f64::from(blue) >= threshold {
        2
    } else {
        1
    }
}

/// 認識済みの画面(上から順)を1枚のページとして解釈する。
pub fn read_screens(screen_list: &[Screen]) -> StatusResult {
    let mut line_list: Vec<StatusLine> = Vec::new();
    let mut offset = 0.0;
    for screen in screen_list {
        for line in &screen.line_list {
            let rarity = sub_skill_rarity(&screen.image, line);
            line_list.push(StatusLine {
                text: line.text.clone(),
                x0: line.x0,
                y0: line.y0 + offset,
                x1: line.x1,
                y1: line.y1 + offset,
                sub_skill_rarity: Some(rarity),
            });
        }
        offset += f64::from(screen.image.height());
    }

    let raw_text = line_list.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
    let candy = find_candy_name(&line_list);
    // 食材欄は1つの画面にしか映らないので、最初に見つかった画面の結果を使う
    let food_list = screen_list.iter().map(|screen| &screen.food_list).find(|list| !list.is_empty());
    parse_data_with_candy(&raw_text, &line_list, food_list.map_or(&[], |l| l.as_slice()), &line_list, &line_list, candy)
}

/// 画像(PNG)を上から順に認識して、ポケモンの情報にする。
pub fn read_pngs(engine: &dyn OcrEngine, png_list: &[Vec<u8>]) -> Result<StatusResult, String> {
    let screen_list = png_list.iter().map(|png| recognize_screen(engine, png)).collect::<Result<Vec<_>, _>>()?;
    Ok(read_screens(&screen_list))
}
