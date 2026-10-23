//! ポケモン詳細画面のスクリーンショット(複数枚)を読み取って、ポケモンの情報にする。
//!
//! 詳細画面は縦に長く、1画面に収まらない。画面ごとの画像を縦に並べた1枚のページとして扱う
//! (2枚目の座標は、1枚目の高さだけ下にずらす)ので、本家の1枚ものの解釈をそのまま使える。

use image::RgbImage;

use crate::engine::{OcrEngine, OcrLine, merge_same_row};
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
}

/// 画像(PNG)を読み込み、幅をそろえて、エンジンで文字を認識する。
pub fn recognize_screen(engine: &dyn OcrEngine, png: &[u8]) -> Result<Screen, String> {
    let image = image::load_from_memory(png).map_err(|e| format!("画像を読み込めませんでした: {e}"))?.to_rgb8();
    let (image, png) = if image.width() == IMAGE_WIDTH {
        (image, png.to_vec())
    } else {
        let height = (u64::from(image.height()) * u64::from(IMAGE_WIDTH) / u64::from(image.width())).max(1) as u32;
        let resized = image::imageops::resize(&image, IMAGE_WIDTH, height, image::imageops::FilterType::Lanczos3);
        let mut encoded = Vec::new();
        resized
            .write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png)
            .map_err(|e| format!("画像を変換できませんでした: {e}"))?;
        (resized, encoded)
    };
    let line_list = merge_same_row(&engine.recognize(&png)?, MERGE_GAP_RATIO);
    Ok(Screen { image, line_list })
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
    parse_data_with_candy(&raw_text, &line_list, &[], &line_list, &line_list, candy)
}

/// 画像(PNG)を上から順に認識して、ポケモンの情報にする。
pub fn read_pngs(engine: &dyn OcrEngine, png_list: &[Vec<u8>]) -> Result<StatusResult, String> {
    let screen_list = png_list.iter().map(|png| recognize_screen(engine, png)).collect::<Result<Vec<_>, _>>()?;
    Ok(read_screens(&screen_list))
}
