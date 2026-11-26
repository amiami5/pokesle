//! 簡易診断画面(本家 `pages/tmp-evaluate.vue`)
//!
//! 入力した1匹の厳選度を、進化先・レベル別に評価する。OCR(スクリーンショット入力)は対象外。

use eframe::egui;
use pokesle_core::config::AllSpecialtyMode;
use pokesle_core::data::master;
use pokesle_core::evaluate_table::EvalLv;
use pokesle_core::js::to_fixed;
use pokesle_core::sim_pokemon::BoxPokemon;
use pokesle_core::team_sim::simulate_tmp_evaluate;

use super::evaluate_view::evaluate_setting;
use super::pokemon_edit::{EditResult, PokemonEdit};
use super::{combo, fmt_fixed, food_icon, style};
use crate::app::Shared;
use crate::job::Job;

pub(crate) const LV_LIST: [u32; 5] = [30, 50, 60, 70, 80];
const COL_W: [f32; 6] = [120.0, 56.0, 56.0, 56.0, 56.0, 56.0];
const TABLE_W: f32 = 400.0;

/// グラフの系列(キー, 表示名, 色, とくい)
pub(crate) const SERIES: [(&str, &str, u32, Option<&str>); 4] = [
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MintChartDisplay {
    Both,
    Before,
    After,
}

/// 評価結果(進化先ごと)
pub(crate) struct Evaluated {
    /// 評価した個体(食材の表示に使う)
    pub(crate) pokemon: BoxPokemon,
    pub(crate) has_mint: bool,
    /// 進化先ごと: 名前, とくい, レベル別の(キー → 割合)と診断値
    pub(crate) evolutions: Vec<Evolution>,
}

pub(crate) struct Evolution {
    pub(crate) name: String,
    pub(crate) specialty: String,
    /// LV_LIST 順
    pub(crate) rates: Vec<[Option<f64>; 4]>,
    pub(crate) mint_rates: Vec<[Option<f64>; 4]>,
    pub(crate) berry_energy: Vec<Option<f64>>,
    pub(crate) mint_berry_energy: Vec<Option<f64>>,
    pub(crate) skill_activation_num: Vec<Option<f64>>,
    pub(crate) mint_skill_activation_num: Vec<Option<f64>>,
    /// 食材名 → (ミント前, ミント後)
    pub(crate) foods: Vec<(String, Vec<Option<f64>>, Vec<Option<f64>>)>,
}

pub struct TmpEvaluateView {
    tab: Tab,
    pokemon: BoxPokemon,
    search: String,
    job: Option<Job<Evaluated>>,
    result: Option<Evaluated>,
    mint_chart_display: MintChartDisplay,
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
        TmpEvaluateView {
            tab: Tab::Pokemon,
            search: p.name.clone(),
            pokemon: p,
            job: None,
            result: None,
            mint_chart_display: MintChartDisplay::Both,
            error: String::new(),
            edit: None,
        }
    }
}

/// 各食材枠で選べる食材(1枠目はA、2枠目はAかB、3枠目はA〜C。4種以上のポケモンは全て)
pub(crate) fn food_options(name: &str, slot: usize) -> Vec<String> {
    let Some(base) = master().pokemon(name) else { return Vec::new() };
    base.food_name_list
        .iter()
        .enumerate()
        .filter(|(i, _)| base.food_name_list.len() > 3 || *i <= slot)
        .map(|(_, n)| n.clone())
        .collect()
}

/// 食材を各枠の先頭の候補にする(本家はポケモンを変えたとき各枠の最初の食材にする)
pub(crate) fn reset_foods(p: &mut BoxPokemon) {
    p.food_list = (0..3).map(|slot| food_options(&p.name, slot).into_iter().next().unwrap_or_default()).collect();
}

impl TmpEvaluateView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            match r {
                Ok(r) => {
                    self.result = Some(r);
                    self.mint_chart_display = MintChartDisplay::Both;
                }
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

    fn start_evaluate(&mut self, ctx: &egui::Context, shared: &Shared, with_mint: bool) {
        self.result = None;
        self.error.clear();
        let config = shared.sim_config();
        let pokemon = self.pokemon.clone();
        self.job = Some(Job::spawn(ctx, move |p| evaluate(&config, pokemon, with_mint, &p)));
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
        let can_evaluate_mint = m.nature(&self.pokemon.nature).is_some_and(|n| n.good.is_some());
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                if style::form_button(ui, "通常評価", style::ButtonKind::Execute).clicked() {
                    self.start_evaluate(ui.ctx(), shared, false);
                }
                if ui.add_enabled_ui(can_evaluate_mint, |ui| style::form_button(ui, "ミント評価", style::ButtonKind::Execute)).inner.clicked() {
                    self.start_evaluate(ui.ctx(), shared, true);
                }
            });
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
        if graph && result.has_mint {
            ui.horizontal(|ui| {
                ui.label("ミント表示：");
                ui.radio_value(&mut self.mint_chart_display, MintChartDisplay::Both, "両方");
                ui.radio_value(&mut self.mint_chart_display, MintChartDisplay::Before, "ミント前");
                ui.radio_value(&mut self.mint_chart_display, MintChartDisplay::After, "ミント後");
            });
        }
        ui.add_space(6.0);

        let has_mint = result.has_mint;
        let mint_display = self.mint_chart_display;
        let all_sp = shared.ui_setting.all_specialty;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for evo in &result.evolutions {
                ui.vertical(|ui| {
                    ui.set_width(TABLE_W);
                    ui.label(egui::RichText::new(&evo.name).size(16.0).strong());
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("厳選度").size(13.0).strong().color(style::muted()));
                    if graph {
                        chart(ui, evo, has_mint, mint_display, all_sp);
                    } else {
                        rate_table(ui, evo, has_mint);
                    }
                    style::small(
                        ui,
                        "厳選度は、全サブスキル・全せいかくの組合せのうち、どの程度優れているかを示す値です。100%が理論値です。",
                        TABLE_W,
                    );
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("実際の値").size(13.0).strong().color(style::muted()));
                    diagnostic_table(ui, evo, has_mint);
                    style::small(
                        ui,
                        "食材の下に表示されている数値は、現環境の必要最大数に対する割合です。300%＝3食分です。",
                        TABLE_W,
                    );
                });
            }
        });
        ui.add_space(8.0);
        if style::form_button(ui, "ボックスに追加", style::ButtonKind::Execute).clicked() {
            self.edit = Some(PokemonEdit::new(None, Some(result.pokemon.clone())));
        }
    }

    fn setting_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        tmp_setting_tab(ui, shared);
    }
}

/// 簡易診断・A/B比較で共有する設定タブ
pub(crate) fn tmp_setting_tab(ui: &mut egui::Ui, shared: &mut Shared) {
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

/// 本家 `evaluate()`: 簡易診断表を作ってから、入力した個体を評価する
fn evaluate(config: &pokesle_core::config::Config, pokemon: BoxPokemon, with_mint: bool, progress: &crate::job::Progress) -> Result<Evaluated, String> {
    let r = simulate_tmp_evaluate(config, &pokemon, with_mint, progress)?;
    box_result_to_evaluated(pokemon, r, with_mint)
}

pub(crate) fn box_result_to_evaluated(pokemon: BoxPokemon, r: pokesle_core::box_sim::BoxResult, with_mint: bool) -> Result<Evaluated, String> {
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
                mint_rates: LV_LIST
                    .iter()
                    .map(|&lv| SERIES.map(|(key, ..)| entry(lv).and_then(|e| e.get(key)).and_then(|i| i.pure_mint.as_ref()).map(|i| i.rate)))
                    .collect(),
                berry_energy: LV_LIST.iter().map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).map(|d| d.berry_energy)).collect(),
                mint_berry_energy: LV_LIST
                    .iter()
                    .map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).and_then(|d| d.pure_mint.as_ref()).map(|d| d.berry_energy))
                    .collect(),
                skill_activation_num: LV_LIST.iter().map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).map(|d| d.skill_activation_num)).collect(),
                mint_skill_activation_num: LV_LIST
                    .iter()
                    .map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).and_then(|d| d.pure_mint.as_ref()).map(|d| d.skill_activation_num))
                    .collect(),
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
                        let before = LV_LIST
                            .iter()
                            .map(|&lv| entry(lv).and_then(|e| e.diagnostic.as_ref()).and_then(|d| d.food_num_map.iter().find(|(n, _)| *n == food).map(|(_, v)| *v)))
                            .collect();
                        let after = LV_LIST
                            .iter()
                            .map(|&lv| {
                                entry(lv)
                                    .and_then(|e| e.diagnostic.as_ref())
                                    .and_then(|d| d.pure_mint.as_ref())
                                    .and_then(|d| d.food_num_map.iter().find(|(n, _)| *n == food).map(|(_, v)| *v))
                            })
                            .collect();
                        (food, before, after)
                    })
                    .collect(),
                name,
            }
        })
        .collect();
    Ok(Evaluated { pokemon, has_mint: with_mint, evolutions })
}

/// 折れ線グラフ(縦軸 0〜100%、横軸は Lv30〜80)
pub(crate) fn chart(ui: &mut egui::Ui, evo: &Evolution, has_mint: bool, display: MintChartDisplay, all_sp: AllSpecialtyMode) {
    let show_legend_note = has_mint && display == MintChartDisplay::Both;
    let size = egui::vec2(TABLE_W, if show_legend_note { 246.0 } else { 230.0 });
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let bottom_pad = if show_legend_note { 56.0 } else { 40.0 };
    let plot = egui::Rect::from_min_max(rect.min + egui::vec2(40.0, 8.0), rect.max - egui::vec2(8.0, bottom_pad));
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
    let show_before = !has_mint || display != MintChartDisplay::After;
    let show_after = has_mint && display != MintChartDisplay::Before;
    for (si, (_, label, color, specialty)) in SERIES.iter().enumerate() {
        let c = style::pick(*color, *color);
        // 進化先のとくい以外は補助情報として薄く表示する
        let dim = specialty.is_some_and(|s| {
            s != evo.specialty
                && !(evo.specialty == "オール"
                    && matches!(
                        (all_sp, s),
                        (AllSpecialtyMode::Max, _)
                            | (AllSpecialtyMode::Berry, "きのみ")
                            | (AllSpecialtyMode::Food, "食材")
                            | (AllSpecialtyMode::Skill, "スキル")
                    ))
        });
        let c = if dim { c.gamma_multiply(0.4) } else { c };
        let to_points = |rates: &[[Option<f64>; 4]]| -> Vec<egui::Pos2> {
            rates
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let v = (r[si].unwrap_or(0.0) * 100.0).clamp(0.0, 100.0) as f32;
                    egui::pos2(x_of(i), plot.bottom() - plot.height() * v / 100.0)
                })
                .collect()
        };
        if show_before {
            let points = to_points(&evo.rates);
            p.add(egui::Shape::line(points.clone(), egui::Stroke::new(2.0, c)));
            for pt in points {
                p.circle_filled(pt, 3.0, c);
            }
        }
        if show_after {
            let points = to_points(&evo.mint_rates);
            p.extend(egui::Shape::dashed_line(&points, egui::Stroke::new(2.0, c), 6.0, 3.0));
            for pt in points {
                p.circle_filled(pt, 3.0, c);
            }
        }
        // 凡例
        let lx = rect.left() + 12.0 + (si % 2) as f32 * 180.0;
        let ly = rect.bottom() - (if show_legend_note { 44.0 } else { 28.0 }) + (si / 2) as f32 * 14.0;
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(lx, ly + 2.0), egui::vec2(14.0, 6.0)), 2.0, c);
        let text = if has_mint && display == MintChartDisplay::After { format!("{label}（ミント後）") } else { (*label).to_string() };
        p.text(egui::pos2(lx + 20.0, ly), egui::Align2::LEFT_TOP, text, font.clone(), muted);
    }
    if show_legend_note {
        p.text(egui::pos2(rect.left() + 12.0, rect.bottom() - 14.0), egui::Align2::LEFT_TOP, "実線: ミント前  /  破線: ミント後", font, muted);
    }
}

#[derive(Clone, Copy)]
enum RowBg {
    Header,
    Normal,
    Striped,
    Mint,
}

fn table_row(ui: &mut egui::Ui, bg: RowBg, cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>>) {
    let left = ui.cursor().min.x;
    let top = ui.cursor().min.y;
    let pad = 4.0;
    let bg_slot = ui.painter().add(egui::Shape::Noop);
    let mut x = left;
    let mut bottom = top;
    for (col, add) in cells.into_iter().enumerate() {
        let w = COL_W[col];
        let rect = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(w, 0.0));
        let align = if col == 0 && !matches!(bg, RowBg::Mint) { egui::Align::Min } else { egui::Align::Max };
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(pad, 0.0)).translate(egui::vec2(0.0, pad)).with_max_y(f32::INFINITY))
                .layout(egui::Layout::top_down(align)),
        );
        child.set_width(w - pad * 2.0);
        child.spacing_mut().item_spacing.y = 1.0;
        add(&mut child);
        bottom = bottom.max(child.min_rect().bottom() + pad);
        x += w;
    }
    let fill = match bg {
        RowBg::Header => style::primary_soft(),
        RowBg::Normal => style::surface(),
        RowBg::Striped => style::surface_subtle(),
        RowBg::Mint => style::mint_soft(),
    };
    let row_rect = egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + TABLE_W, bottom));
    let shapes = vec![
        egui::Shape::rect_filled(row_rect, 0.0, fill),
        egui::Shape::hline(row_rect.x_range(), bottom, egui::Stroke::new(1.0, style::line_color())),
    ];
    ui.painter().set(bg_slot, egui::Shape::Vec(shapes));
    ui.allocate_rect(row_rect, egui::Sense::hover());
}

fn header_cells<'a>() -> Vec<Box<dyn FnOnce(&mut egui::Ui) + 'a>> {
    let mut cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + 'a>> = vec![Box::new(|ui| {
        ui.label("");
    })];
    for lv in LV_LIST {
        cells.push(Box::new(move |ui| {
            style::th(ui, &format!("Lv{lv}"));
        }));
    }
    cells
}

pub(crate) fn diff_info(before: Option<f64>, after: Option<f64>, digits: usize, suffix: &str) -> Option<(String, egui::Color32)> {
    let (Some(b), Some(a)) = (before, after) else { return None };
    let diff = a - b;
    let sign = if diff > 0.0 { "+" } else { "" };
    let text = format!("{sign}{}{suffix}", fmt_fixed(diff, digits));
    let color = if diff > 0.0 {
        style::pick(0xbf6575, 0xe87c8f)
    } else if diff < 0.0 {
        style::pick(0x3d73c9, 0x6ea0f0)
    } else {
        style::muted()
    };
    Some((text, color))
}

fn mint_label_cell<'a>() -> Box<dyn FnOnce(&mut egui::Ui) + 'a> {
    Box::new(|ui| {
        ui.label(egui::RichText::new("ミント後").size(10.5).color(style::muted()));
    })
}

pub(crate) fn rate_table(ui: &mut egui::Ui, evo: &Evolution, has_mint: bool) {
    let old = std::mem::replace(&mut ui.spacing_mut().item_spacing.y, 0.0);
    table_row(ui, RowBg::Header, header_cells());
    for (si, (_, label, ..)) in SERIES.iter().enumerate() {
        let row_bg = if !has_mint && si % 2 == 1 { RowBg::Striped } else { RowBg::Normal };
        let mut cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![Box::new(move |ui| {
            ui.label(*label);
        })];
        for r in &evo.rates {
            let v = r[si].unwrap_or(0.0) * 100.0;
            cells.push(Box::new(move |ui| {
                ui.label(format!("{}%", fmt_fixed(v, 1)));
            }));
        }
        table_row(ui, row_bg, cells);

        if has_mint {
            let mut mint_cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![mint_label_cell()];
            for (li, mr) in evo.mint_rates.iter().enumerate() {
                let before = evo.rates[li][si].map(|x| x * 100.0);
                let after = mr[si].map(|x| x * 100.0);
                let diff = diff_info(before, after, 1, "%");
                mint_cells.push(Box::new(move |ui| {
                    ui.label(format!("{}%", fmt_fixed(after.unwrap_or(0.0), 1)));
                    if let Some((text, color)) = diff {
                        ui.label(egui::RichText::new(text).size(10.5).color(color));
                    }
                }));
            }
            table_row(ui, RowBg::Mint, mint_cells);
        }
    }
    ui.spacing_mut().item_spacing.y = old;
}

pub(crate) fn food_requirement_rate(value: Option<f64>, food_name: &str) -> String {
    let Some(v) = value else { return String::new() };
    let required_num = master()
        .cookings
        .iter()
        .flat_map(|c| c.food_list.iter())
        .filter(|f| f.name == food_name)
        .map(|f| f.num)
        .fold(0.0_f64, f64::max);
    if required_num > 0.0 {
        format!("{}%", to_fixed((v / required_num) * 100.0, 0))
    } else {
        String::new()
    }
}

/// 診断用のエナジー・食材・スキル発動回数
pub(crate) fn diagnostic_table(ui: &mut egui::Ui, evo: &Evolution, has_mint: bool) {
    let show = |v: Option<f64>, digits: usize| v.map_or("－".to_string(), |v| fmt_fixed(v, digits));
    let old = std::mem::replace(&mut ui.spacing_mut().item_spacing.y, 0.0);
    table_row(ui, RowBg::Header, header_cells());

    // きのみエナジー
    let mut cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![Box::new(|ui| {
        ui.label("きのみエナジー");
    })];
    for v in &evo.berry_energy {
        let s = show(*v, 1);
        cells.push(Box::new(move |ui| {
            ui.label(s);
        }));
    }
    table_row(ui, RowBg::Normal, cells);
    if has_mint {
        let mut mint_cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![mint_label_cell()];
        for (li, v) in evo.mint_berry_energy.iter().enumerate() {
            let s = show(*v, 1);
            let diff = diff_info(evo.berry_energy[li], *v, 1, "");
            mint_cells.push(Box::new(move |ui| {
                ui.label(s);
                if let Some((text, color)) = diff {
                    ui.label(egui::RichText::new(text).size(10.5).color(color));
                }
            }));
        }
        table_row(ui, RowBg::Mint, mint_cells);
    }

    // 食材
    for (fi, (food, values, mint_values)) in evo.foods.iter().enumerate() {
        let row_bg = if !has_mint && fi % 2 == 0 { RowBg::Striped } else { RowBg::Normal };
        let mut cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![Box::new(move |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                food_icon::food_icon(ui, food, 20.0);
                ui.label(food);
            });
        })];
        for v in values {
            let s = show(*v, 1);
            let req = food_requirement_rate(*v, food);
            cells.push(Box::new(move |ui| {
                ui.label(s);
                if !req.is_empty() {
                    ui.label(egui::RichText::new(req).size(10.5).color(style::muted()));
                }
            }));
        }
        table_row(ui, row_bg, cells);

        if has_mint {
            let mut mint_cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![mint_label_cell()];
            for (li, v) in mint_values.iter().enumerate() {
                let s = show(*v, 1);
                let req = food_requirement_rate(*v, food);
                let diff = diff_info(values[li], *v, 1, "");
                mint_cells.push(Box::new(move |ui| {
                    ui.label(s);
                    if !req.is_empty() {
                        ui.label(egui::RichText::new(req).size(10.5).color(style::muted()));
                    }
                    if let Some((text, color)) = diff {
                        ui.label(egui::RichText::new(text).size(10.5).color(color));
                    }
                }));
            }
            table_row(ui, RowBg::Mint, mint_cells);
        }
    }

    // スキル発動回数
    let skill_bg = if !has_mint && evo.foods.len() % 2 == 1 { RowBg::Striped } else { RowBg::Normal };
    let mut cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![Box::new(|ui| {
        ui.label("スキル発動回数");
    })];
    for v in &evo.skill_activation_num {
        let s = show(*v, 2);
        cells.push(Box::new(move |ui| {
            ui.label(s);
        }));
    }
    table_row(ui, skill_bg, cells);
    if has_mint {
        let mut mint_cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>> = vec![mint_label_cell()];
        for (li, v) in evo.mint_skill_activation_num.iter().enumerate() {
            let s = show(*v, 2);
            let diff = diff_info(evo.skill_activation_num[li], *v, 2, "");
            mint_cells.push(Box::new(move |ui| {
                ui.label(s);
                if let Some((text, color)) = diff {
                    ui.label(egui::RichText::new(text).size(10.5).color(color));
                }
            }));
        }
        table_row(ui, RowBg::Mint, mint_cells);
    }

    ui.spacing_mut().item_spacing.y = old;
}
