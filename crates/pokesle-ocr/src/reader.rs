//! ポケモン詳細画面のスクリーンショット(複数枚)を読み取って、ポケモンの情報にする。
//!
//! 詳細画面は縦に長く、1画面に収まらない。画面ごとの画像を縦に並べた1枚のページとして扱う
//! (2枚目の座標は、1枚目の高さだけ下にずらす)ので、本家の1枚ものの解釈をそのまま使える。

use image::{RgbImage, RgbaImage};

use crate::engine::{OcrEngine, OcrLine, merge_same_row};
use crate::food::{FoodSample, food_icon_list, food_slot_locked, match_sample, recognize_food_list};
use crate::status::{FoodRecognition, allowed_food_list, find_candy_line};
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
    /// 食材欄の各枠のアイコン(見本との比較と、見本の保存に使う)
    pub food_icon_list: Vec<RgbaImage>,
}

/// 読み取りの結果。
pub struct ReadResult {
    pub status: StatusResult,
    /// 食材欄の各枠のアイコン。利用者が確定した食材の見本として保存できる
    pub food_icon_list: Vec<RgbaImage>,
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
    let food_icon_list = if food_list.is_empty() { Vec::new() } else { food_icon_list(&image) };
    Ok(Screen { image, line_list, food_list, food_icon_list })
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
/// `sample_list` は利用者が確定した食材の見本で、あれば色の分布より優先して食材を決める。
pub fn read_screens(screen_list: &[Screen], sample_list: &[FoodSample]) -> ReadResult {
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
    let food_screen = screen_list.iter().find(|screen| !screen.food_list.is_empty());
    let food_list = food_screen.map_or(&[][..], |screen| screen.food_list.as_slice());
    let mut status = parse_data_with_candy(&raw_text, &line_list, food_list, &line_list, &line_list, candy);
    let food_icon_list = food_screen.map(|screen| screen.food_icon_list.clone()).unwrap_or_default();
    apply_food_samples(&mut status, &food_icon_list, sample_list);
    ReadResult { status, food_icon_list }
}

/// 見本と形まで比べて、持てる食材の中から十分に近いものがあれば、その食材に決める。
/// 色の分布だけでは、未解放の枠の薄いアイコンを見分けられないため。
fn apply_food_samples(status: &mut StatusResult, icon_list: &[RgbaImage], sample_list: &[FoodSample]) {
    let (Some(pokemon_name), Some(lv)) = (status.pokemon_name.clone(), status.lv) else { return };
    for (slot, icon) in icon_list.iter().enumerate().take(3) {
        let allowed = allowed_food_list(&pokemon_name, slot);
        if allowed.len() < 2 {
            continue;
        }
        if let Some(name) = match_sample(icon, food_slot_locked(lv, slot), &allowed, sample_list) {
            if status.food_list.len() <= slot {
                status.food_list.resize(slot + 1, None);
            }
            status.food_list[slot] = Some(name);
        }
    }
}

/// 画像(PNG)を上から順に認識して、ポケモンの情報にする。
pub fn read_pngs(engine: &dyn OcrEngine, png_list: &[Vec<u8>], sample_list: &[FoodSample]) -> Result<ReadResult, String> {
    let screen_list = png_list.iter().map(|png| recognize_screen(engine, png)).collect::<Result<Vec<_>, _>>()?;
    Ok(read_screens(&screen_list, sample_list))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 円の色の上に、指定の色の四角を描いた 48x48 のアイコン
    fn icon(color: [u8; 3]) -> RgbaImage {
        RgbaImage::from_fn(48, 48, |x, y| {
            if (12..36).contains(&x) && (12..36).contains(&y) {
                image::Rgba([color[0], color[1], color[2], 255])
            } else {
                image::Rgba([255, 247, 220, 255])
            }
        })
    }

    fn sample(name: &str, faded: bool, color: [u8; 3]) -> FoodSample {
        FoodSample { name: name.to_string(), faded, icon: icon(color) }
    }

    fn gardevoir(lv: u32) -> StatusResult {
        StatusResult {
            pokemon_name: Some("サーナイト".into()),
            lv: Some(lv),
            food_list: vec![Some("とくせんリンゴ".into()), Some("とくせんリンゴ".into()), Some("とくせんリンゴ".into())],
            ..Default::default()
        }
    }

    const RED: [u8; 3] = [210, 40, 40];
    const BLUE: [u8; 3] = [40, 60, 210];

    #[test]
    fn sample_decides_the_food_among_allowed() {
        // サーナイトの2枠目は、とくせんリンゴかワカクサコーン
        let sample_list = [sample("とくせんリンゴ", false, RED), sample("ワカクサコーン", false, BLUE)];
        let mut status = gardevoir(67);
        apply_food_samples(&mut status, &[icon(RED), icon(BLUE), icon(BLUE)], &sample_list);
        assert_eq!(status.food_list[1].as_deref(), Some("ワカクサコーン"));
        assert_eq!(status.food_list[2].as_deref(), Some("ワカクサコーン"));
    }

    #[test]
    fn samples_of_other_display_state_are_not_used() {
        // Lv25 の2枠目は未解放(薄い表示)なので、通常表示の見本とは比べない
        let sample_list = [sample("ワカクサコーン", false, BLUE)];
        let mut status = gardevoir(25);
        apply_food_samples(&mut status, &[icon(RED), icon(BLUE), icon(BLUE)], &sample_list);
        assert_eq!(status.food_list[1].as_deref(), Some("とくせんリンゴ"));

        let sample_list = [sample("ワカクサコーン", true, BLUE)];
        apply_food_samples(&mut status, &[icon(RED), icon(BLUE), icon(BLUE)], &sample_list);
        assert_eq!(status.food_list[1].as_deref(), Some("ワカクサコーン"));
    }

    #[test]
    fn dissimilar_sample_is_ignored() {
        let sample_list = [sample("ワカクサコーン", false, BLUE)];
        let mut status = gardevoir(67);
        apply_food_samples(&mut status, &[icon(RED), icon(RED), icon(RED)], &sample_list);
        assert_eq!(status.food_list[1].as_deref(), Some("とくせんリンゴ"));
    }
}
