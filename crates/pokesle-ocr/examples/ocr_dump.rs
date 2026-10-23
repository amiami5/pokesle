//! 画像(PNG)を OCR して、認識した行と座標を表示する。
//! 使い方: ocr_dump <画像.png> [windows|google]
//! google を選ぶときは、環境変数 GOOGLE_VISION_API_KEY に API キーを入れておく。

use std::sync::{Arc, Mutex};

use pokesle_ocr::engine::OcrEngine;
use pokesle_ocr::google_vision::{
    FREE_UNITS_PER_MONTH, GoogleVision, PRICING_URL, VisionUsage, current_month_key,
};
use pokesle_ocr::windows_ocr::WindowsOcr;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("使い方: ocr_dump <画像.png> [windows|google]");
        std::process::exit(2);
    };
    let png = std::fs::read(path).expect("画像を読み込めませんでした");

    let usage = Arc::new(Mutex::new(VisionUsage::default()));
    let engine: Box<dyn OcrEngine> = match args.get(1).map(String::as_str) {
        Some("google") => {
            let key = std::env::var("GOOGLE_VISION_API_KEY").unwrap_or_default();
            Box::new(GoogleVision::new(&key).with_usage(usage.clone()))
        }
        _ => Box::new(WindowsOcr::japanese()),
    };

    println!("エンジン: {}", engine.name());
    match engine.recognize(&png) {
        Ok(line_list) => {
            for line in line_list {
                println!("({:.0},{:.0})-({:.0},{:.0}) {}", line.x0, line.y0, line.x1, line.y1, line.text);
            }
            let month = current_month_key();
            let count = usage.lock().map_or(0, |u| u.count_in(&month));
            if count > 0 {
                println!("今回の利用: {count} ユニット(無料枠は月 {FREE_UNITS_PER_MONTH} ユニット。料金: {PRICING_URL})");
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
