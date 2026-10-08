//! 設定画面(主要な項目のみ)

use eframe::egui;
use pokesle_core::config::Config;
use pokesle_core::data::master;

use super::combo;
use crate::app::Shared;

#[derive(Default)]
pub struct SettingsView;

fn num(ui: &mut egui::Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>, speed: f64) -> bool {
    ui.label(label);
    let r = ui.add(egui::DragValue::new(value).range(range).speed(speed)).changed();
    ui.end_row();
    r
}

fn check(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    ui.label(label);
    let r = ui.checkbox(value, "").changed();
    ui.end_row();
    r
}

impl SettingsView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let m = master();
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let c: &mut Config = &mut shared.config;
            ui.heading("全般");
            egui::Grid::new("general").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                changed |= num(ui, "睡眠時間", &mut c.sleep_time, 0.0..=24.0, 0.1);
                changed |= num(ui, "1日の確認回数", &mut c.check_freq, 2.0..=48.0, 0.1);
                changed |= num(ui, "計算スレッド数", &mut c.worker_num, 1.0..=64.0, 0.1);
            });

            ui.separator();
            ui.heading("フィールド・料理");
            egui::Grid::new("field").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                ui.label("フィールド");
                changed |= combo(ui, "field", &mut c.simulation.field, m.fields.iter().map(|f| f.name.clone()), 200.0);
                ui.end_row();

                let field = m.field(&c.simulation.field);
                if let Some(options) = field.and_then(|f| f.berry_option_list.as_ref()) {
                    ui.label("好きなきのみ");
                    ui.horizontal(|ui| {
                        c.simulation.berry_list.resize(3, String::new());
                        for (i, list) in options.iter().enumerate() {
                            let opts = std::iter::once(String::new()).chain(list.iter().cloned());
                            changed |= combo(ui, ("berry", i), &mut c.simulation.berry_list[i], opts, 90.0);
                        }
                    });
                    ui.end_row();
                }
                if field.is_some_and(|f| f.ex) {
                    ui.label("EX効果");
                    egui::ComboBox::from_id_salt("field_ex")
                        .selected_text(match c.simulation.field_ex as i32 {
                            1 => "きのみ",
                            2 => "食材",
                            3 => "スキル",
                            _ => "",
                        })
                        .show_ui(ui, |ui| {
                            for (v, l) in [(1.0, "きのみ"), (2.0, "食材"), (3.0, "スキル")] {
                                changed |= ui.selectable_value(&mut c.simulation.field_ex, v, l).changed();
                            }
                        });
                    ui.end_row();
                }
                changed |= num(ui, "フィールドボーナス(%)", &mut c.simulation.field_bonus, 0.0..=999.0, 1.0);

                ui.label("料理カテゴリ");
                let mut ty = c.simulation.cooking_type.clone().unwrap_or_default();
                if combo(ui, "cooking_type", &mut ty, ["カレー", "サラダ", "デザート"].map(String::from), 120.0) {
                    c.simulation.cooking_type = Some(ty);
                    changed = true;
                }
                ui.end_row();
                changed |= num(ui, "鍋の容量", &mut c.simulation.pot_size, 1.0..=999.0, 1.0);
                changed |= check(ui, "キャンプチケット", &mut c.simulation.camp_ticket);
                changed |= check(ui, "げんき常に100%で計算", &mut c.simulation.genki_full);
                changed |= check(ui, "リサーチランクMAX", &mut c.simulation.research_rank_max);
                changed |= check(ui, "いつ育モードも候補にする", &mut c.simulation.bag_over_operation);
                changed |= num(ui, "ゆめのかけらの重み(%)", &mut c.simulation.shard_weight, 0.0..=1000.0, 1.0);
            });

            ui.separator();
            ui.heading("チーム編成");
            egui::Grid::new("team").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                let t = &mut c.team_simulation;
                changed |= num(ui, "対象: きのみスコア上位(匹)", &mut t.max_rank_berry, 0.0..=100.0, 0.1);
                changed |= num(ui, "対象: 食材スコア上位(匹)", &mut t.max_rank_food, 0.0..=100.0, 0.1);
                changed |= num(ui, "対象: 総合スコア上位(匹)", &mut t.max_rank_all, 0.0..=100.0, 0.1);
                changed |= num(ui, "対象: サポート抜きスコア上位(匹)", &mut t.max_rank_not_support, 0.0..=100.0, 0.1);
                changed |= num(ui, "結果の表示件数", &mut t.result_num, 1.0..=10.0, 0.1);
                changed |= num(ui, "ナイトキャップピカチュウ Lv(0は無し)", &mut t.night_cap_pikachu, 0.0..=20.0, 0.1);
            });

            ui.separator();
            if ui.button("初期設定に戻す").clicked() {
                *c = Config::default_config();
                changed = true;
            }
        });
        if changed {
            shared.save_config();
        }
    }
}
