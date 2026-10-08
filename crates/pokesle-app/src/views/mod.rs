pub mod box_view;
pub mod credit_view;
pub mod pokemon_edit;
pub mod settings_view;
pub mod team_view;

use eframe::egui;
use pokesle_core::data::master;

/// 食材の並び(ABC表記)
pub fn food_abc(name: &str, food_list: &[String]) -> String {
    let Some(base) = master().pokemon(name) else { return String::new() };
    food_list
        .iter()
        .map(|f| base.food_name_list.iter().position(|x| x == f).map(|i| (b'A' + i as u8) as char).unwrap_or('?'))
        .collect()
}

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
