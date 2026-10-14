pub mod box_summary_view;
pub mod box_view;
pub mod check_list_view;
pub mod common_setting;
pub mod credit_view;
pub mod data_page_view;
pub mod evaluate_detail;
pub mod evaluate_view;
pub mod food_icon;
pub mod food_stock_view;
pub mod history_view;
pub mod pokemon_edit;
pub mod pokemon_filter;
pub mod pokemon_target;
pub mod style;
pub mod team_result;
pub mod team_view;
pub mod tmp_evaluate_view;
pub mod ui_setting_view;

use eframe::egui;
use pokesle_core::data::master;

/// 数値を桁区切りで表示
pub fn fmt_num(x: f64) -> String {
    if !x.is_finite() {
        return "-".into();
    }
    let n = x.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// 小数点以下の桁数を固定し、整数部を桁区切りで表示(JS の toLocaleString 相当)
pub fn fmt_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return "-".into();
    }
    // 丸めは JS の toFixed と同じ(同じ距離なら大きい方へ)
    let s = pokesle_core::js::to_fixed(x.abs(), digits);
    let (int, frac) = s.split_once('.').map_or((s.as_str(), None), |(i, f)| (i, Some(f)));
    let neg = x < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0');
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    if neg { format!("-{out}") } else { out }
}

/// 性格名の右に上昇(赤)・下降(青)の略字を小さく2段で表示する(本家 nature-info.vue)
pub fn nature_label(ui: &mut egui::Ui, name: &str) {
    let nature = master().nature(name);
    let r = ui.label(name);
    let Some(n) = nature.filter(|n| n.good_short.is_some() || n.weak_short.is_some()) else { return };
    let (good, weak) = if ui.visuals().dark_mode {
        (egui::Color32::from_rgb(255, 100, 100), egui::Color32::from_rgb(120, 160, 255))
    } else {
        (egui::Color32::RED, egui::Color32::BLUE)
    };
    // 行の高さを変えないよう、レイアウトを使わず上下半分ずつに直接描く
    let size = ui.style().text_styles[&egui::TextStyle::Body].size * 0.65;
    let font = egui::FontId::proportional(size);
    let width = size * 2.0;
    let height = ui.available_height().max(size * 2.0);
    let (rect, inner) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    for (text, color, y) in [(&n.good_short, good, rect.center().y - size * 0.55), (&n.weak_short, weak, rect.center().y + size * 0.55)] {
        if let Some(text) = text {
            let pos = egui::pos2(rect.center().x, y);
            painter.text(pos, egui::Align2::CENTER_CENTER, text, font.clone(), color);
            // 太字の代わりに少しずらして重ね描きする
            painter.text(pos + egui::vec2(0.4, 0.0), egui::Align2::CENTER_CENTER, text, font.clone(), color);
        }
    }
    let hover = format!("↑ {}
↓ {}", n.good.as_deref().unwrap_or("-"), n.weak.as_deref().unwrap_or("-"));
    r.union(inner).on_hover_text(hover);
}

/// 文字列の選択肢から選ぶコンボボックス
pub fn combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut String, options: impl IntoIterator<Item = String>, width: f32) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(value.as_str()).width(width).show_ui(ui, |ui| {
        for o in options {
            if ui.selectable_label(*value == o, &o).clicked() {
                *value = o;
                changed = true;
            }
        }
    });
    changed
}
