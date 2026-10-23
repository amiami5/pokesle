//! 画像(PNG)を OCR して、認識した行と座標を表示する。使い方: ocr_dump <画像.png>

use pokesle_ocr::engine::OcrEngine;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("使い方: ocr_dump <画像.png>");
        std::process::exit(2);
    };
    let png = std::fs::read(&path).expect("画像を読み込めませんでした");
    let engine = pokesle_ocr::windows_ocr::WindowsOcr::japanese();
    println!("エンジン: {}", engine.name());
    match engine.recognize(&png) {
        Ok(line_list) => {
            for line in line_list {
                println!("({:.0},{:.0})-({:.0},{:.0}) {}", line.x0, line.y0, line.x1, line.y1, line.text);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
