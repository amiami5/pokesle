//! クレジット・ライセンス表記

use eframe::egui;
use pokesle_core::data::master;

const LICENSE: &str = include_str!("../../../../LICENSE");
const THIRD_PARTY: &str = include_str!("../../../../THIRD_PARTY_LICENSES.md");

pub fn ui(ui: &mut egui::Ui) {
    let m = master();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.heading("このアプリについて");
        ui.label("ポケスリシミュレーター(reimer0204 氏作)の計算処理を Rust へ移植した、非公式の Windows アプリです。");
        ui.horizontal(|ui| {
            ui.label("本家:");
            ui.hyperlink("https://github.com/reimer0204/pokesle-simulator");
        });
        ui.label(format!("計算・データの基準となる本家のコミット: {}", m.upstream_commit));
        ui.label("本アプリは本家の作者および株式会社ポケモン等の各権利者とは関係ありません。");
        ui.label("ポケモン・ポケモンスリープに関する名称等の権利は各権利者に帰属します。");

        ui.separator();
        ui.heading("ライセンス");
        ui.add(egui::Label::new(egui::RichText::new(LICENSE).monospace()).wrap());

        ui.separator();
        ui.heading("使用しているライブラリ");
        ui.add(egui::Label::new(egui::RichText::new(THIRD_PARTY).monospace()).wrap());
    });
}
