//! 簡易診断画面(本家 `pages/tmp-evaluate.vue`)
//!
//! 入力した1匹の厳選度を、進化先・レベル別に評価する。OCR(スクリーンショット入力)は対象外。

use eframe::egui;
use pokesle_core::data::master;
use pokesle_core::evaluate_table::EvalLv;
use pokesle_core::sim_pokemon::BoxPokemon;
use pokesle_core::team_sim::simulate_tmp_evaluate;

use super::evaluate_view::evaluate_setting;
use super::pokemon_edit::{EditResult, PokemonEdit};
use super::{combo, fmt_fixed, food_icon, style};
use crate::app::Shared;
use crate::job::Job;

const LV_LIST: [u32; 5] = [30, 50, 60, 70, 80];

/// グラフの系列(キー, 表示名, 色, とくい)
const SERIES: [(&str, &str, u32, Option<&str>); 4] = [
    ("energy", "総合厳選度", 0xbf6575, None),
    ("berry", "きのみ厳選度", 0x6cbd7f, Some("きのみ")),
    ("food", "食材厳選度", 0xdf8650, Some("食材")),
    ("skill", "スキル厳選度", 0x3d73c9, Some("スキル")),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Pokemon,
    Setting,
}

/// 評価結果(進化先ごと)
struct Evaluated {
    /// 評価した個体(食材の表示に使う)
    pokemon: BoxPokemon,
    /// 進化先ごと: 名前, とくい, レベル別の(キー → 割合)と診断値
    evolutions: Vec<Evolution>,
}

struct Evolution {
    name: String,
    specialty: String,
    /// LV_LIST 順
    rates: Vec<[Option<f64>; 4]>,
    berry_energy: Vec<Option<f64>>,
    skill_activation_num: Vec<Option<f64>>,
    /// 食材名 → レベル別の個数
    foods: Vec<(String, Vec<Option<f64>>)>,
}

pub struct TmpEvaluateView {
    tab: Tab,
    pokemon: BoxPokemon,
    search: String,
    job: Option<Job<Evaluated>>,
    result: Option<Evaluated>,
    error: String,
    edit: Option<PokemonEdit>,
}

impl Default for TmpEvaluateView {
    fn default() -> Self {
        let m = master();
        let base = &m.pokemon[0];
        let mut p = BoxPokemon {
            name: base.name.clone(),
            lv: 1.0,
            skill_lv: Some(1.0),
            food_list: Vec::new(),
            sub_skill_list: vec![String::new(); 5],
            nature: "がんばりや".into(),
            ..Default::default()
        };
        reset_foods(&mut p);
        TmpEvaluateView { tab: Tab::Pokemon, search: p.name.clone(), pokemon: p, job: None, result: None, error: String::new(), edit: None }
    }
}

/// 各食材枠で選べる食材(1枠目はA、2枠目はAかB、3枠目はA〜C。4種以上のポケモンは全て)
fn food_options(name: &str, slot: usize) -> Vec<String> {
    let Some(base) = master().pokemon(name) else { return Vec::new() };
    base.food_name_list
        .iter()
        .enumerate()
        .filter(|(i, _)| base.food_name_list.len() > 3 || *i <= slot)
        .map(|(_, n)| n.clone())
        .collect()
}

/// 食材を各枠の先頭の候補にする(本家はポケモンを変えたとき各枠の最初の食材にする)
fn reset_foods(p: &mut BoxPokemon) {
    p.food_list = (0..3).map(|slot| food_options(&p.name, slot).into_iter().next().unwrap_or_default()).collect();
}

impl TmpEvaluateView {
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

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(900.0);
            style::alert(
                ui,
                "入力したポケモンが、全サブスキル・せいかくの組合せのうちどの程度上位に位置するかを評価します。\n複数の個体の厳選度を比較したい場合は、「基準生成」ページで評価テーブルの生成を行った後、ボックスに対象ポケモンを追加してください。",
            );
            ui.add_space(6.0);
            let labels = vec!["ポケモン".to_string(), "設定".to_string()];
            if let Some(i) = style::tab_list(ui, &labels, if self.tab == Tab::Pokemon { 0 } else { 1 }) {
                self.tab = if i == 0 { Tab::Pokemon } else { Tab::Setting };
            }
            ui.add_space(6.0);
            match self.tab {
                Tab::Pokemon => self.pokemon_tab(ui, shared),
                Tab::Setting => self.setting_tab(ui, shared),
            }
        });

        if let Some(edit) = &mut self.edit {
            match edit.ui(ui.ctx()) {
                EditResult::None => {}
                EditResult::Cancel => self.edit = None,
                EditResult::Save(_, pokemon) => {
                    shared.box_file.list.push(*pokemon);
                    shared.save_box();
                    self.edit = None;
                }
            }
        }
    }

    fn pokemon_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let m = master();
        egui::Frame::new().fill(style::surface()).stroke(egui::Stroke::new(1.0, style::line_color())).corner_radius(6).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("tmp_form").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                style::th(ui, "ポケモン");
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("名前で検索").desired_width(260.0));
                    let query = self.search.trim().to_string();
                    if !query.is_empty() && query != self.pokemon.name {
                        let hits: Vec<_> = m.pokemon.iter().filter(|p| p.name.contains(&query)).take(12).collect();
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            if hits.is_empty() {
                                ui.label("該当なし");
                            }
                            for p in hits {
                                if ui.selectable_label(false, format!("{} ({}・{})", p.name, p.specialty.name(), m.pokemon_skill(p).name)).clicked() {
                                    self.pokemon.name = p.name.clone();
                                    self.search = p.name.clone();
                                    reset_foods(&mut self.pokemon);
                                }
                            }
                        });
                    } else {
                        ui.label(egui::RichText::new(&self.pokemon.name).strong());
                    }
                });
                ui.end_row();

                style::th(ui, "食材");
                ui.horizontal(|ui| {
                    for slot in 0..3 {
                        let options = food_options(&self.pokemon.name, slot);
                        if self.pokemon.food_list.len() <= slot {
                            self.pokemon.food_list.resize(3, String::new());
                        }
                        if !options.contains(&self.pokemon.food_list[slot]) {
                            self.pokemon.food_list[slot] = options.first().cloned().unwrap_or_default();
                        }
                        combo(ui, ("tmp_food", slot), &mut self.pokemon.food_list[slot], options, 120.0);
                    }
                });
                ui.end_row();

                style::th(ui, "サブスキル");
                ui.vertical(|ui| {
                    self.pokemon.sub_skill_list.resize(5, String::new());
                    for (slot, lv) in [10, 25, 50, 75, 100].iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Lv{lv}"));
                            let mut options = vec![String::new()];
                            options.extend(m.sub_skills.iter().map(|s| s.name.clone()));
                            combo(ui, ("tmp_sub", slot), &mut self.pokemon.sub_skill_list[slot], options, 200.0);
                        });
                    }
                });
                ui.end_row();

                style::th(ui, "せいかく");
                combo(ui, "tmp_nature", &mut self.pokemon.nature, m.natures.iter().map(|n| n.name.clone()), 160.0);
                ui.end_row();
            });
        });
        ui.add_space(6.0);

        let running = self.job.is_some();
        ui.add_enabled_ui(!running, |ui| {
            if style::form_button(ui, "評価", style::ButtonKind::Execute).clicked() {
                self.result = None;
                self.error.clear();
                let config = shared.config.clone();
                let pokemon = self.pokemon.clone();
                self.job = Some(Job::spawn(ui.ctx(), move |p| evaluate(&config, pokemon, &p)));
            }
        });
        if let Some(job) = &self.job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }

        let Some(result) = &self.result else { return };
        ui.add_space(8.0);
        // 厳選度表示(本家 config.pokemonEdit.selectDisplayMode)
        let mut graph = shared
            .config
            .extra
            .get("pokemonEdit")
            .and_then(|v| v.get("selectDisplayMode"))
            .and_then(|v| v.as_str())
            .is_none_or(|s| s != "table");
        let before = graph;
        ui.horizontal(|ui| {
            ui.label("厳選度表示：");
            ui.radio_value(&mut graph, true, "グラフ");
            ui.radio_value(&mut graph, false, "表");
        });
        if graph != before {
            let e = shared.config.extra.entry("pokemonEdit").or_insert_with(|| serde_json::json!({}));
            e["selectDisplayMode"] = serde_json::json!(if graph { "graph" } else { "table" });
            shared.save_config();
        }
        ui.add_space(6.0);

        let food_names: Vec<String> = {
            let mut v: Vec<String> = Vec::new();
            for f in &result.pokemon.food_list {
                if !v.contains(f) {
                    v.push(f.clone());
                }
            }
            v
        };
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for evo in &result.evolutions {
                ui.vertical(|ui| {
                    ui.set_width(380.0);
                    ui.label(egui::RichText::new(&evo.name).size(16.0).strong());
                    if graph {
                        chart(ui, evo);
                    } else {
                        rate_table(ui, evo);
                    }
                    ui.add_space(6.0);
                    diagnostic_table(ui, evo, &food_names);
                });
            }
        });
        ui.add_space(8.0);
        if style::form_button(ui, "ボックスに追加", style::ButtonKind::Execute).clicked() {
            self.edit = Some(PokemonEdit::new(None, Some(result.pokemon.clone())));
        }
    }

    fn setting_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        // 本家 EvaluateSetting に tmpEvaluate を渡す。画面側は通常設定と同じ入れ物に写して編集し、書き戻す
        let tmp = shared.config.tmp_evaluate.clone();
        let mut c = shared.config.clone();
        c.select_evaluate = tmp.clone();
        c.sleep_time = tmp.sleep_time.unwrap_or(c.sleep_time);
        c.check_freq = tmp.check_freq.unwrap_or(c.check_freq);
        c.worker_num = tmp.worker_num.unwrap_or(c.worker_num);
        let before = serde_json::to_string(&c.select_evaluate).unwrap_or_default();
        let before_basic = (c.sleep_time, c.check_freq, c.worker_num);

        evaluate_setting(ui, &mut c, true);

        ui.add_space(8.0);
        ui.label(egui::RichText::new("簡易診断用の基準値").strong());
        let mut healer = tmp.score_for_healer_evaluate.unwrap_or(0.0);
        let mut support = tmp.score_for_support_evaluate.unwrap_or(0.0);
        let mut changed = false;
        egui::Grid::new("tmp_base").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            style::th(ui, "ヒーラー評価基準");
            ui.vertical(|ui| {
                changed |= ui.add_sized([100.0, 22.0], egui::DragValue::new(&mut healer)).changed();
                style::small(ui, "げんきオールなどの評価に使う、他のポケモンの1日エナジーです。", 400.0);
            });
            ui.end_row();
            style::th(ui, "サポート評価基準");
            ui.vertical(|ui| {
                changed |= ui.add_sized([100.0, 22.0], egui::DragValue::new(&mut support)).changed();
                style::small(ui, "おてつだいサポートなどの評価に使う、他のポケモンの1回のおてつだい価値です。", 400.0);
            });
            ui.end_row();
        });

        let after = serde_json::to_string(&c.select_evaluate).unwrap_or_default();
        if changed || before != after || before_basic != (c.sleep_time, c.check_freq, c.worker_num) {
            let mut new = c.select_evaluate;
            new.sleep_time = Some(c.sleep_time);
            new.check_freq = Some(c.check_freq);
            new.worker_num = Some(c.worker_num);
            new.score_for_healer_evaluate = Some(healer);
            new.score_for_support_evaluate = Some(support);
            shared.config.tmp_evaluate = new;
            shared.save_config();
        }
    }
}

/// 本家 `evaluate()`: 簡易診断表を作ってから、入力した個体を評価する
fn evaluate(config: &pokesle_core::config::Config, pokemon: BoxPokemon, progress: &crate::job::Progress) -> Result<Evaluated, String> {
    let r = simulate_tmp_evaluate(config, &pokemon, progress)?;
    let er = r.evaluate_result.ok_or("厳選度を評価できませんでした")?;

    let m = master();
    let evolutions = er
        .names(EvalLv::Lv(LV_LIST[0]))
        .into_iter()
        .map(|name| {
            let entry = |lv: u32| er.get(EvalLv::Lv(lv), &name);
            Evolution {
                specialty: m.pokemon(&name).map(|p| p.specialty.name().to_string()).unwrap_or_default(),
                rates: LV_LIST
                    .iter()
                    .map(|&lv| SERIES.map(|(key, ..)| entry(lv).and_then(|e| e.get(key)).map(|i| i.rate)))
                    .collect(),
                berry_energy: LV_LIST.iter().map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).map(|d| d.berry_energy)).collect(),
                skill_activation_num: LV_LIST.iter().map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).map(|d| d.skill_activation_num)).collect(),
                foods: pokemon
                    .food_list
                    .iter()
                    .fold(Vec::<String>::new(), |mut v, f| {
                        if !v.contains(f) {
                            v.push(f.clone());
                        }
                        v
                    })
                    .into_iter()
                    .map(|food| {
                        let values = LV_LIST
                            .iter()
                            .map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).and_then(|d| d.food_num_map.iter().find(|(n, _)| *n == food).map(|(_, v)| *v)))
                            .collect();
                        (food, values)
                    })
                    .collect(),
                name,
            }
        })
        .collect();
    Ok(Evaluated { pokemon, evolutions })
}

/// 折れ線グラフ(縦軸 0〜100%、横軸は Lv30〜80)
fn chart(ui: &mut egui::Ui, evo: &Evolution) {
    let size = egui::vec2(360.0, 230.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let plot = egui::Rect::from_min_max(rect.min + egui::vec2(40.0, 8.0), rect.max - egui::vec2(8.0, 40.0));
    let grid = style::line_color();
    let muted = style::muted();
    let font = egui::FontId::proportional(11.0);
    for i in 0..=5 {
        let v = i as f32 * 20.0;
        let y = plot.bottom() - plot.height() * v / 100.0;
        p.hline(plot.x_range(), y, egui::Stroke::new(1.0, grid));
        p.text(egui::pos2(plot.left() - 4.0, y), egui::Align2::RIGHT_CENTER, format!("{v:.0}%"), font.clone(), muted);
    }
    let x_of = |i: usize| plot.left() + plot.width() * (i as f32 + 0.5) / LV_LIST.len() as f32;
    for (i, lv) in LV_LIST.iter().enumerate() {
        p.text(egui::pos2(x_of(i), plot.bottom() + 4.0), egui::Align2::CENTER_TOP, format!("Lv{lv}"), font.clone(), muted);
    }
    for (si, (_, label, color, specialty)) in SERIES.iter().enumerate() {
        let c = style::pick(*color, *color);
        // 進化先のとくい以外は補助情報として薄く表示する
        let dim = specialty.is_some_and(|s| s != evo.specialty);
        let c = if dim { c.gamma_multiply(0.4) } else { c };
        let points: Vec<egui::Pos2> = evo
            .rates
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let v = (r[si].unwrap_or(0.0) * 100.0).clamp(0.0, 100.0) as f32;
                egui::pos2(x_of(i), plot.bottom() - plot.height() * v / 100.0)
            })
            .collect();
        p.add(egui::Shape::line(points.clone(), egui::Stroke::new(2.0, c)));
        for pt in points {
            p.circle_filled(pt, 3.0, c);
        }
        // 凡例
        let lx = rect.left() + 12.0 + (si % 2) as f32 * 170.0;
        let ly = rect.bottom() - 28.0 + (si / 2) as f32 * 14.0;
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(lx, ly + 2.0), egui::vec2(14.0, 6.0)), 2.0, c);
        p.text(egui::pos2(lx + 20.0, ly), egui::Align2::LEFT_TOP, *label, font.clone(), muted);
    }
}

fn header_row(ui: &mut egui::Ui) {
    ui.label("");
    for lv in LV_LIST {
        style::th(ui, &format!("Lv{lv}"));
    }
    ui.end_row();
}

fn rate_table(ui: &mut egui::Ui, evo: &Evolution) {
    egui::Grid::new(("tmp_rate", &evo.name)).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
        header_row(ui);
        for (si, (_, label, ..)) in SERIES.iter().enumerate() {
            ui.label(*label);
            for r in &evo.rates {
                ui.label(format!("{}%", fmt_fixed(r[si].unwrap_or(0.0) * 100.0, 1)));
            }
            ui.end_row();
        }
    });
}

/// 診断用のエナジー・食材・スキル発動回数
fn diagnostic_table(ui: &mut egui::Ui, evo: &Evolution, _food_names: &[String]) {
    let show = |v: Option<f64>, digits: usize| v.map_or("－".to_string(), |v| fmt_fixed(v, digits));
    egui::Grid::new(("tmp_diag", &evo.name)).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
        header_row(ui);
        ui.label("きのみエナジー");
        for v in &evo.berry_energy {
            ui.label(show(*v, 1));
        }
        ui.end_row();
        for (food, values) in &evo.foods {
            ui.horizontal(|ui| {
                food_icon::food_icon(ui, food, 20.0);
                ui.label(food);
            });
            for v in values {
                ui.label(show(*v, 1));
            }
            ui.end_row();
        }
        ui.label("スキル発動回数");
        for v in &evo.skill_activation_num {
            ui.label(show(*v, 2));
        }
        ui.end_row();
    });
}
