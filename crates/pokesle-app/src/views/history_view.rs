//! 更新履歴画面(本家 `pages/history.vue`)

use eframe::egui;
use pokesle_core::history::{history, Item, Segment};

use super::style;

/// 本文を描く。アプリ内リンク(`/credit` など)がクリックされたらその画面のパスを返す
pub fn ui(ui: &mut egui::Ui) -> Option<String> {
    let h = history();
    let mut route = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.label(egui::RichText::new("更新履歴").size(20.0).strong());
        ui.add_space(6.0);
        for entry in &h.entries {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(&entry.date).size(15.0).strong().color(style::primary_strong()));
            ui.add_space(2.0);
            for item in &entry.items {
                item_ui(ui, item, 0, &mut route);
            }
        }
    });
    route
}

fn item_ui(ui: &mut egui::Ui, item: &Item, depth: usize, route: &mut Option<String>) {
    ui.horizontal_top(|ui| {
        ui.add_space(14.0 * depth as f32 + 6.0);
        ui.label(if depth % 2 == 0 { "・" } else { "◦" });
        ui.vertical(|ui| segments(ui, &item.segments, route));
    });
    for child in &item.children {
        item_ui(ui, child, depth + 1, route);
    }
}

/// 文章の断片を並べる(改行を含む断片は行を分ける)
fn segments(ui: &mut egui::Ui, list: &[Segment], route: &mut Option<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in list {
            let mut parts = s.text.split('\n').peekable();
            while let Some(part) = parts.next() {
                if !part.is_empty() {
                    if let Some(href) = &s.href {
                        ui.hyperlink_to(part, href);
                    } else if let Some(r) = &s.route {
                        if ui.link(part).clicked() {
                            *route = Some(r.clone());
                        }
                    } else {
                        ui.label(part);
                    }
                }
                if parts.peek().is_some() {
                    ui.end_row();
                }
            }
        }
    });
}
