//! 画像の食材欄を認識して、枠ごとの上位の候補と類似度を表示する(種族による補正の前の結果)。
//! 使い方: food_dump <画像.png>

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("使い方: food_dump <画像.png>");
        std::process::exit(2);
    };
    let image = image::open(&path).expect("画像を読み込めませんでした").to_rgb8();
    let list = pokesle_ocr::food::recognize_food_list(&image);
    if list.is_empty() {
        println!("食材欄を見つけられませんでした");
    }
    for (index, food) in list.iter().enumerate() {
        let top: Vec<String> = food.match_list.iter().take(3).map(|(n, s)| format!("{n} {s:.3}")).collect();
        println!(
            "枠{}: {} (空き={} 隠れ={}) 上位: {}",
            index + 1,
            food.name.as_deref().unwrap_or("―"),
            food.empty,
            food.occluded,
            top.join(" / ")
        );
    }
}
