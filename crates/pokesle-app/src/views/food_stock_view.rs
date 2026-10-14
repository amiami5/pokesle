//! 食材準備画面(本家 `pages/food_stock.vue`)

use eframe::egui;
use pokesle_core::data::{master, CookingType};
use pokesle_core::food_stock::{calculate, enable_cooking_list, max_candidate_num, FoodStockResult};

use super::{common_setting, fmt_num, food_icon, style};
use crate::app::Shared;
use crate::job::Job;

#[derive(Default)]
pub struct FoodStockView {
    job: Option<Job<FoodStockResult>>,
    result: Option<FoodStockResult>,
    error: String,
    started: bool,
}

impl FoodStockView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            match r {
                Ok(r) => self.result = Some(r),
                Err(e) => self.error = e,
            }
        }

        let m = master();
        let list = enable_cooking_list(&shared.config);
        let disabled = m.cookings.len() - list.len();
        let max_candidate = max_candidate_num(&shared.config, &list);

        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            changed |= common_setting::cooking_setting(ui, &mut shared.config);
            changed |= ui.checkbox(&mut shared.config.food_stock.exclude_max_recipe_lv, "カンスト除外").changed();
        });
        ui.add_space(5.0);

        egui::Frame::new().fill(style::SURFACE).stroke(egui::Stroke::new(1.0, style::LINE)).corner_radius(6).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let fs = &mut shared.config.food_stock;
            egui::Grid::new("food_stock_setting").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                style::th(ui, "種類別重み設定");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for t in CookingType::ALL {
                            ui.label(format!("{}：", t.name()));
                            let w = fs.weight.entry(t.name().to_string()).or_insert(0.0);
                            changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(w).range(0.0..=1000.0)).changed();
                            ui.label("%");
                            ui.add_space(10.0);
                        }
                    });
                    style::small(ui, "0%の種類は最適化の対象外です。週の出現しやすさに合わせて比重を調整できます。", 500.0);
                });
                ui.end_row();
                style::th(ui, "食材バッグ");
                ui.horizontal(|ui| {
                    changed |= ui.add_sized([80.0, 22.0], egui::DragValue::new(&mut fs.bag_size).range(0.0..=100000.0)).changed();
                    ui.label("個");
                });
                ui.end_row();
                style::th(ui, "料理回数");
                ui.horizontal(|ui| {
                    ui.label("最大");
                    changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut fs.cooking_num).range(1.0..=21.0)).changed();
                    ui.label("食");
                });
                ui.end_row();
                style::th(ui, "料理候補");
                ui.horizontal(|ui| {
                    ui.label("エナジー上位");
                    changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut fs.candidate_num).range(1.0..=(max_candidate.max(1) as f64))).changed();
                    ui.label("件");
                    ui.add_space(10.0);
                    ui.label("必要食材数");
                    changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut fs.min_food_num).range(0.0..=999.0)).changed();
                    ui.label("個以上");
                    ui.add_space(10.0);
                    ui.label("必要食材数");
                    changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut fs.max_food_num).range(0.0..=999.0)).changed();
                    ui.label("個以下");
                });
                ui.end_row();
            });
        });
        if changed {
            shared.save_config();
        }
        ui.add_space(5.0);

        // 本家は初回表示時にも計算する
        let running = self.job.is_some();
        let mut run = !self.started;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!running, |ui| {
                let label = if running { "計算中…" } else { "準備食材を計算" };
                run |= style::form_button(ui, label, style::ButtonKind::Execute).clicked();
            });
            if disabled > 0 {
                ui.label(format!("(料理の無効:{disabled}種)"));
            }
        });
        if run && !running {
            self.started = true;
            self.result = None;
            self.error.clear();
            let config = shared.config.clone();
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u32).unwrap_or(0);
            self.job = Some(Job::spawn(ui.ctx(), move |p| {
                p.set(0.0, "準備食材を探索しています…");
                calculate(&config, seed, &|v| p.set(v, ""))
            }));
        }
        if let Some(job) = &self.job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(0xb2, 0x1d, 0x1d), &self.error);
        }

        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            if let Some(result) = &self.result {
                if result_ui(ui, result, shared, &list) {
                    shared.save_config();
                }
            }
            ui.add_space(8.0);
            style::small(ui, "設定をもとに、エナジーが高くなりそうな料理の組み合わせを探して表示します。計算方法の性質上、最も良い組み合わせを必ず見つけられるとは限りません。", 700.0);
        });
    }
}

const SHORTAGE: egui::Color32 = egui::Color32::from_rgb(0xb2, 0x1d, 0x1d);

/// 設定が変わったら true
fn result_ui(ui: &mut egui::Ui, result: &FoodStockResult, shared: &mut Shared, list: &[pokesle_core::cooking::EvaluatedCooking]) -> bool {
    let m = master();
    let mut changed = false;
    ui.add_space(6.0);
    ui.label(egui::RichText::new(format!("準備する食材({} / {} 個)", fmt_num(result.total_food_num), fmt_num(shared.config.food_stock.bag_size.max(0.0)))).size(16.0).strong());
    ui.add_space(4.0);

    let geometric_mean = result.plans.iter().map(|p| p.energy).product::<f64>().powf(1.0 / 3.0);
    let unlimited = shared.config.food_unlimited;
    let stocked = |shared: &Shared, name: &str| -> f64 {
        if unlimited { 9999.0 } else { shared.config.food_default_num.get(name).copied().unwrap_or(0.0) }
    };

    egui::Grid::new("food_stock_result").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
        style::th(ui, "料理の種類");
        style::th(ui, "料理名");
        style::th(ui, "回数");
        for food in &m.foods {
            food_icon::food_icon(ui, &food.name, 24.0);
        }
        style::th(ui, "種類合計エナジー");
        ui.end_row();

        for (t, ty) in CookingType::ALL.iter().enumerate() {
            let plan = &result.plans[t];
            if plan.cooking_list.is_empty() {
                style::th(ui, ty.name());
                ui.label("重みが0%のため、最適化の対象外です。");
                ui.end_row();
                continue;
            }
            for (i, item) in plan.cooking_list.iter().enumerate() {
                if i == 0 {
                    ui.label(egui::RichText::new(ty.name()).strong());
                } else {
                    ui.label("");
                }
                ui.label(&item.name);
                ui.label(format!("{}食", item.num));
                for fi in 0..m.foods.len() {
                    match item.food_num_map.get(&fi) {
                        Some(n) => ui.label(fmt_num(*n)),
                        None => ui.label(""),
                    };
                }
                if i == 0 {
                    ui.label(fmt_num(plan.energy));
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        }

        // 合計
        style::th(ui, "準備食材量(3種類の相乗平均)");
        ui.label("");
        ui.label("");
        for n in &result.food_num_list {
            ui.label(egui::RichText::new(fmt_num(*n)).strong());
        }
        ui.label(egui::RichText::new(fmt_num(geometric_mean)).strong());
        ui.end_row();

        style::th(ui, "所持食材");
        ui.label("");
        ui.label("");
        for food in &m.foods {
            if unlimited {
                let mut v = 9999.0;
                ui.add_enabled(false, egui::DragValue::new(&mut v));
            } else {
                let v = shared.config.food_default_num.entry(food.name.clone()).or_insert(0.0);
                changed |= ui.add_sized([44.0, 20.0], egui::DragValue::new(v).range(0.0..=9999.0)).changed();
            }
        }
        ui.label("");
        ui.end_row();

        style::th(ui, "所持数との差分");
        ui.label("");
        ui.label("");
        for (fi, food) in m.foods.iter().enumerate() {
            let diff = stocked(shared, &food.name) - result.food_num_list[fi];
            let mut text = egui::RichText::new(fmt_num(diff));
            if diff < 0.0 {
                text = text.color(SHORTAGE).strong();
            }
            ui.label(text);
        }
        ui.label("");
        ui.end_row();
    });

    // 余った食材で作れる料理
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("余った食材で作れる料理").size(16.0).strong());
        ui.add_space(10.0);
        ui.label("各食材があと");
        changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut shared.config.food_stock.surplus_food_num).range(0.0..=9999.0)).changed();
        ui.label("個ある場合");
    });
    style::small(ui, "所持数との差分に設定個数を加え、必要数を満たす食材だけで作れる料理です。", 600.0);
    let surplus = shared.config.food_stock.surplus_food_num.floor().max(0.0);
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        for ty in CookingType::ALL {
            let mut cookings: Vec<_> = list
                .iter()
                .filter(|c| {
                    let cooking = c.cooking();
                    cooking.cooking_type == ty
                        && cooking.food_num > 0.0
                        && cooking.food_list.iter().all(|f| {
                            let diff = stocked(shared, &f.name) - result.food_num_list[f.food];
                            diff + surplus >= f.num
                        })
                })
                .collect();
            cookings.sort_by(|a, b| b.fix_energy.partial_cmp(&a.fix_energy).unwrap_or(std::cmp::Ordering::Equal));
            egui::Frame::new().stroke(egui::Stroke::new(1.0, style::LINE)).fill(style::SURFACE).corner_radius(4).inner_margin(6).show(ui, |ui| {
                ui.set_width(260.0);
                ui.label(egui::RichText::new(ty.name()).strong().color(style::PRIMARY_STRONG));
                if cookings.is_empty() {
                    ui.label(egui::RichText::new("作れる料理はありません。").size(12.0).color(style::MUTED));
                }
                for c in cookings {
                    let cooking = c.cooking();
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(190.0);
                            ui.label(egui::RichText::new(&cooking.name).strong());
                            let foods = cooking.food_list.iter().map(|f| format!("{}×{}", f.name, f.num)).collect::<Vec<_>>().join("、");
                            ui.label(egui::RichText::new(foods).size(11.0).color(style::MUTED));
                        });
                        ui.label(fmt_num(c.fix_energy));
                    });
                    ui.separator();
                }
            });
        }
    });
    changed
}
