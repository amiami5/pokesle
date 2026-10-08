//! ポケスリシミュレーター(Rust 版)
//!
//! reimer0204/pokesle-simulator (MIT) の非公式移植。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod job;
mod storage;
mod views;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ポケスリシミュレーター")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native("pokesle", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
