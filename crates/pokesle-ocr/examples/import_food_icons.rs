//! 食材の一覧画面(ゲームのボックスの絞り込み画面など)のスクリーンショットから、食材アイコンの見本を作る。
//! 使い方: import_food_icons <画像.png> <保存先フォルダ>
//! 保存先には、アプリと同じ名前の付け方(「0_時刻_食材名.png」)で保存する。

use pokesle_ocr::engine::OcrEngine;
use pokesle_ocr::windows_ocr::WindowsOcr;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(path), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("使い方: import_food_icons <画像.png> <保存先フォルダ>");
        std::process::exit(2);
    };
    let png = std::fs::read(path).expect("画像を読み込めませんでした");
    let image = image::load_from_memory(&png).expect("画像を読み込めませんでした").to_rgb8();
    let line_list = WindowsOcr::japanese().recognize(&png).expect("文字を認識できませんでした");
    let sample_list = pokesle_ocr::food::samples_from_food_list_screen(&image, &line_list);
    std::fs::create_dir_all(out).expect("保存先を作れませんでした");
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    for sample in &sample_list {
        let file = std::path::Path::new(out).join(format!("0_{time}_{}.png", sample.name));
        sample.icon.save(&file).expect("保存できませんでした");
    }
    let mut name_list: Vec<&str> = sample_list.iter().map(|s| s.name.as_str()).collect();
    name_list.sort();
    println!("{} 種の見本を保存しました: {}", sample_list.len(), name_list.join("、"));
}
