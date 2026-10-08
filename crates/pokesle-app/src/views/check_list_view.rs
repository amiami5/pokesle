//! チェックリスト画面(本家 pages/check-list.vue と check-list/*.vue)

use std::hash::{Hash, Hasher};

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use pokesle_core::box_sim::BoxResult;
use pokesle_core::check_list::{
    field_check_list, food_combination, CheckList, CheckListResult, CheckTarget, FieldRow, FoodCheckRow, PokemonCheckRow, ScoreEntry, SkillCheckRow,
    SLEEP_TYPES,
};
use pokesle_core::config::{Config, PokemonCondition, PokemonTarget};
use pokesle_core::data::master;
use pokesle_core::evaluate_table::{EvaluateResult, EvaluateTable};
use pokesle_core::team_sim::simulate_box;
use serde_json::Value;

use super::{fmt_fixed, food_icon, pokemon_target};
use crate::app::Shared;
use crate::job::Job;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Pokemon,
    Food,
    Skill,
    Field,
    Setting,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingTab {
    Pokemon,
    Food,
    Skill,
    Other,
}

/// ボックス内訳の小窓に出す行
#[derive(Clone, Copy, PartialEq, Eq)]
enum Detail {
    Pokemon(usize),
    Food(usize),
    Skill(usize),
}

/// 小窓の1行(ボックス内の位置, 値1, 値2)
type DetailLine = (Option<usize>, String, String);

struct Computed {
    pokemon: CheckListResult<PokemonCheckRow>,
    food: CheckListResult<FoodCheckRow>,
    skill: CheckListResult<SkillCheckRow>,
    field: Vec<FieldRow>,
}

pub struct CheckListView {
    tab: Tab,
    setting_tab: SettingTab,
    job: Option<Job<Vec<BoxResult>>>,
    /// 一覧用の概算結果(仮定計算なし・いつ育なし)
    results: Vec<BoxResult>,
    /// 概算に使った設定・ボックス・厳選テーブルの指紋
    sim_key: Option<u64>,
    computed: Option<Computed>,
    unchecked_only: bool,
    detail: Option<Detail>,
    target_edit: Option<usize>,
    hidden_open: bool,
    error: String,
}

impl Default for CheckListView {
    fn default() -> Self {
        CheckListView {
            tab: Tab::Pokemon,
            setting_tab: SettingTab::Pokemon,
            job: None,
            results: Vec::new(),
            sim_key: None,
            computed: None,
            unchecked_only: false,
            detail: None,
            target_edit: None,
            hidden_open: false,
            error: String::new(),
        }
    }
}

fn percent(x: Option<f64>) -> String {
    match x {
        Some(v) if v.is_finite() => format!("{}%", fmt_fixed(v * 100.0, 1)),
        _ => String::new(),
    }
}

/// 一覧用の概算の設定(本家 check-list.vue: いつ育・仮定計算なし)
fn page_config(config: &Config) -> Config {
    let mut c = config.clone();
    c.simulation.bag_over_operation = false;
    c.simulation.fix = false;
    c
}

/// 概算をやり直す必要があるかの判定に使う指紋(チェックリスト設定以外の設定・ボックス・厳選テーブル)
fn sim_key(shared: &Shared, table: &EvaluateTable) -> u64 {
    let mut v = page_config(&shared.config).to_value();
    if let Value::Object(m) = &mut v {
        m.remove("summary");
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.to_string().hash(&mut h);
    shared.box_file.time.hash(&mut h);
    shared.box_file.list.len().hash(&mut h);
    format!("{:?}{:?}", table.healer, table.support).hash(&mut h);
    table.pokemon.len().hash(&mut h);
    h.finish()
}

/// ボックスのポケモンの短い表示(名前 Lv)
fn pokemon_text(r: &BoxResult) -> String {
    let b = r.pokemon.box_pokemon.as_ref();
    format!("{} Lv{}", b.map(|b| b.name.as_str()).unwrap_or(&r.pokemon.base().name), r.pokemon.lv)
}

fn pokemon_hover(r: &BoxResult) -> String {
    let Some(b) = r.pokemon.box_pokemon.as_ref() else { return String::new() };
    format!("ボックス No.{}\n{}\n{}\n{}", b.index + 1, b.nature, b.food_list.join(" / "), b.sub_skill_list.join(" / "))
}

/// 空欄を null とする数値入力
fn opt_num(ui: &mut egui::Ui, id: impl Hash + std::fmt::Debug, value: &mut Option<f64>, hint: &str, width: f32) -> bool {
    let id = egui::Id::new(id);
    let fmt = |v: &Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| fmt(value));
    let r = ui.add(egui::TextEdit::singleline(&mut text).id(id.with("edit")).hint_text(hint).desired_width(width));
    let mut changed = false;
    if r.changed() {
        let t = text.trim();
        let parsed = if t.is_empty() { Some(None) } else { t.parse::<f64>().ok().map(Some) };
        if let Some(v) = parsed
            && v != *value
        {
            *value = v;
            changed = true;
        }
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    changed
}

fn lv_label(v: &Value) -> String {
    match v {
        Value::String(s) if s == "max" => "全レベル内最大値".into(),
        Value::Number(n) => format!("Lv. {n}"),
        Value::String(s) => format!("Lv. {s}"),
        _ => String::new(),
    }
}

impl CheckListView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let lv_list = shared.config.select_evaluate.enabled_levels();
        // 本家と同じく、基準レベルが未設定・無効なら Lv60 以上の最小に合わせる
        if shared.config.summary.checklist.normalize_border_lv(&lv_list) {
            shared.save_config();
            self.computed = None;
        }

        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Pokemon, "ポケモン"),
                (Tab::Food, "食材"),
                (Tab::Skill, "スキル"),
                (Tab::Field, "フィールド別"),
                (Tab::Setting, "厳選基準設定"),
            ] {
                ui.selectable_value(&mut self.tab, tab, label);
            }
            if matches!(self.tab, Tab::Pokemon | Tab::Food | Tab::Skill) {
                ui.separator();
                ui.checkbox(&mut self.unchecked_only, "未達成のみ");
            }
        });
        ui.separator();

        if self.tab == Tab::Setting {
            if self.setting_ui(ui, shared, &lv_list) {
                shared.save_config();
                self.computed = None;
            }
            return;
        }

        let Some(table) = shared.usable_evaluate_table().cloned() else {
            ui.label("厳選テーブルがありません。ボックス画面の「厳選テーブルを作成」で作成してください。");
            return;
        };
        self.poll_job();
        let key = sim_key(shared, &table);
        if self.job.is_none() && self.sim_key != Some(key) && !shared.box_file.list.is_empty() {
            let config = page_config(&shared.config);
            let list = shared.box_file.list.clone();
            let t = table.clone();
            self.sim_key = Some(key);
            self.job = Some(Job::spawn(ui.ctx(), move |p| {
                p.set(0.0, "ボックスの厳選度を計算しています…");
                simulate_box(&config, &list, Some(&t), true)
            }));
        }
        if let Some(job) = &self.job {
            job.show_progress(ui);
            return;
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
            return;
        }
        if self.computed.is_none() {
            self.computed = Some(self.compute(&shared.config, &table));
        }
        let Some(c) = &self.computed else { return };
        let open = match self.tab {
            Tab::Pokemon => self.pokemon_table(ui, c),
            Tab::Food => self.food_table(ui, c),
            Tab::Skill => self.skill_table(ui, c),
            Tab::Field => {
                field_table(ui, c);
                None
            }
            Tab::Setting => None,
        };
        if open.is_some() {
            self.detail = open;
        }
        self.detail_window(ui.ctx(), &shared.config);
    }

    fn poll_job(&mut self) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            self.computed = None;
            match r {
                Ok(list) => {
                    self.results = list;
                    self.error.clear();
                }
                Err(e) => self.error = e,
            }
        }
    }

    fn compute(&self, config: &Config, table: &EvaluateTable) -> Computed {
        let empty = EvaluateResult::default();
        let combos: Vec<String> = self
            .results
            .iter()
            .map(|r| r.pokemon.box_pokemon.as_ref().map(|b| food_combination(&r.pokemon.base().food_name_list, &b.food_list)).unwrap_or_default())
            .collect();
        let targets: Vec<CheckTarget> = self
            .results
            .iter()
            .zip(&combos)
            .map(|(r, c)| CheckTarget { base_name: &r.pokemon.base().name, food_combination: c, evaluate_result: r.evaluate_result.as_ref().unwrap_or(&empty) })
            .collect();
        let cl = CheckList::new(config, Some(table));
        let food = cl.food_check_list(&targets, None);
        let skill = cl.skill_check_list(&targets, None);
        let pokemon = cl.pokemon_check_list(&targets, None, Some(&food), Some(&skill));
        let all_targets: Vec<String> =
            pokemon.target_pokemon_list.iter().chain(&food.target_pokemon_list).chain(&skill.target_pokemon_list).cloned().collect();
        let field = field_check_list(config, Some(table), &all_targets);
        Computed { pokemon, food, skill, field }
    }

    fn result(&self, index: usize) -> Option<&BoxResult> {
        self.results.get(index)
    }

    fn pokemon_cell(&self, ui: &mut egui::Ui, index: Option<usize>) {
        if let Some(r) = index.and_then(|i| self.result(i)) {
            ui.label(pokemon_text(r)).on_hover_text(pokemon_hover(r));
        }
    }

    fn checked_cell(ui: &mut egui::Ui, checked: bool) {
        if checked {
            ui.colored_label(egui::Color32::from_rgb(40, 170, 80), "済");
        }
    }

    fn pokemon_table(&self, ui: &mut egui::Ui, c: &Computed) -> Option<Detail> {
        let m = master();
        let rows: Vec<usize> = (0..c.pokemon.rows.len()).filter(|&i| !self.unchecked_only || !c.pokemon.rows[i].checked).collect();
        ui.label(format!("達成 {} / {}", c.pokemon.rows.iter().filter(|x| x.checked).count(), c.pokemon.rows.len()));
        let heights: Vec<f32> = rows.iter().map(|&i| 22.0_f32.max(c.pokemon.rows[i].target.lines().count() as f32 * 17.0 + 5.0)).collect();
        let mut open = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("checklist_pokemon")
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .columns(Column::auto().at_least(40.0), 10)
                .column(Column::remainder())
                .header(22.0, |mut h| {
                    for t in ["厳選対象", "とくい", "きのみ", "タイプ", "食材構成", "目標", "厳選済", "総合厳選度", "とくい厳選度", "ポケモン", ""] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.heterogeneous_rows(heights.into_iter(), |mut row| {
                        let i = rows[row.index()];
                        let r = &c.pokemon.rows[i];
                        let p = &m.pokemon[r.pokemon];
                        let berry = m.pokemon_berry(p);
                        row.col(|ui| {
                            ui.label(&p.name);
                        });
                        row.col(|ui| {
                            ui.label(p.specialty.name());
                        });
                        row.col(|ui| {
                            ui.label(&berry.name);
                        });
                        row.col(|ui| {
                            ui.label(&berry.type_name);
                        });
                        row.col(|ui| {
                            ui.label(&r.food_name);
                        });
                        row.col(|ui| {
                            ui.label(&r.target);
                        });
                        row.col(|ui| Self::checked_cell(ui, r.checked));
                        row.col(|ui| {
                            ui.label(percent(r.energy_score));
                        });
                        row.col(|ui| {
                            ui.label(percent(r.specialty_score));
                        });
                        row.col(|ui| self.pokemon_cell(ui, r.hit));
                        row.col(|ui| {
                            if ui.small_button(format!("ボックス({})", r.list.len())).clicked() {
                                open = Some(Detail::Pokemon(i));
                            }
                        });
                    });
                });
        });
        open
    }

    fn food_table(&self, ui: &mut egui::Ui, c: &Computed) -> Option<Detail> {
        let m = master();
        let rows: Vec<usize> = (0..c.food.rows.len()).filter(|&i| !self.unchecked_only || !c.food.rows[i].checked).collect();
        ui.label(format!("達成 {} / {}", c.food.rows.iter().filter(|x| x.checked).count(), c.food.rows.len()));
        let notes: Vec<String> = c.food.rows.iter().map(|r| r.note()).collect();
        let heights: Vec<f32> = rows.iter().map(|&i| 24.0_f32.max(notes[i].lines().count() as f32 * 17.0 + 5.0)).collect();
        let mut open = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("checklist_food")
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .columns(Column::auto().at_least(40.0), 9)
                .column(Column::remainder())
                .header(22.0, |mut h| {
                    for t in ["厳選対象", "対象ポケモン(期待値)", "最大必要数×3", "基準値", "厳選済", "最良個数", "最良/最大", "最良ポケモン", "", ""] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.heterogeneous_rows(heights.into_iter(), |mut row| {
                        let i = rows[row.index()];
                        let r = &c.food.rows[i];
                        let food = &m.foods[r.food];
                        row.col(|ui| {
                            food_icon::food_icon(ui, &food.name, 20.0);
                            ui.label(&food.name);
                        });
                        row.col(|ui| {
                            ui.label(&notes[i]);
                        });
                        row.col(|ui| {
                            ui.label(fmt_fixed(r.max_num * 3.0, 0));
                        });
                        row.col(|ui| {
                            ui.label(fmt_fixed(r.border, 1));
                        });
                        row.col(|ui| Self::checked_cell(ui, r.checked));
                        row.col(|ui| {
                            ui.label(fmt_fixed(r.score, 1)).on_hover_text(format!("Lv{}", r.lv));
                        });
                        row.col(|ui| {
                            ui.label(percent(Some(r.rate)));
                        });
                        row.col(|ui| self.pokemon_cell(ui, Some(r.hit)));
                        row.col(|ui| {
                            if ui.small_button(format!("ボックス({})", r.list.len())).clicked() {
                                open = Some(Detail::Food(i));
                            }
                        });
                        row.col(|_| {});
                    });
                });
        });
        open
    }

    fn skill_table(&self, ui: &mut egui::Ui, c: &Computed) -> Option<Detail> {
        let m = master();
        let rows: Vec<usize> = (0..c.skill.rows.len()).filter(|&i| !self.unchecked_only || !c.skill.rows[i].checked).collect();
        ui.label(format!("達成 {} / {}", c.skill.rows.iter().filter(|x| x.checked).count(), c.skill.rows.len()));
        let notes: Vec<String> = c.skill.rows.iter().map(|r| r.note()).collect();
        let heights: Vec<f32> = rows.iter().map(|&i| 22.0_f32.max(notes[i].lines().count() as f32 * 17.0 + 5.0)).collect();
        let mut open = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("checklist_skill")
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .columns(Column::auto().at_least(40.0), 8)
                .column(Column::remainder())
                .header(22.0, |mut h| {
                    for t in ["厳選対象", "対象ポケモン(期待値)", "基準値", "最良回数", "厳選済", "最良/最大", "最良ポケモン", "", ""] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.heterogeneous_rows(heights.into_iter(), |mut row| {
                        let i = rows[row.index()];
                        let r = &c.skill.rows[i];
                        row.col(|ui| {
                            ui.label(&m.skills[r.skill].name);
                        });
                        row.col(|ui| {
                            ui.label(&notes[i]);
                        });
                        row.col(|ui| {
                            ui.label(fmt_fixed(r.border, 1));
                        });
                        row.col(|ui| {
                            ui.label(fmt_fixed(r.score, 1)).on_hover_text(format!("Lv{}", r.lv));
                        });
                        row.col(|ui| Self::checked_cell(ui, r.checked));
                        row.col(|ui| {
                            ui.label(percent(Some(r.rate)));
                        });
                        row.col(|ui| self.pokemon_cell(ui, Some(r.hit)));
                        row.col(|ui| {
                            if ui.small_button(format!("ボックス({})", r.list.len())).clicked() {
                                open = Some(Detail::Skill(i));
                            }
                        });
                        row.col(|_| {});
                    });
                });
        });
        open
    }

    /// 「ボックス」ボタンで開く、ボックス内の全ポケモンの値
    fn detail_window(&mut self, ctx: &egui::Context, config: &Config) {
        let (Some(detail), Some(c)) = (self.detail, &self.computed) else { return };
        let m = master();
        let mut open = true;
        let (title, header, lines): (String, [&str; 3], Vec<DetailLine>) = match detail {
            Detail::Pokemon(i) => {
                let Some(r) = c.pokemon.rows.get(i) else { return };
                let name = &m.pokemon[r.pokemon].name;
                let lv = config.summary.checklist.pokemon_condition.select_lv();
                let lines = r
                    .list
                    .iter()
                    .map(|&(idx, _)| {
                        let e = lv.and_then(|lv| self.result(idx).and_then(|x| x.evaluate_result.as_ref()).and_then(|er| er.get(lv, name)));
                        (Some(idx), percent(e.and_then(|e| e.energy.as_ref()).map(|x| x.score)), percent(e.and_then(|e| e.specialty.as_ref()).map(|x| x.score)))
                    })
                    .collect();
                (name.clone(), ["総合厳選度", "とくい厳選度", "ポケモン"], lines)
            }
            Detail::Food(i) => {
                let Some(r) = c.food.rows.get(i) else { return };
                (m.foods[r.food].name.clone(), ["個数", "レベル", "ポケモン"], score_lines(&r.list))
            }
            Detail::Skill(i) => {
                let Some(r) = c.skill.rows.get(i) else { return };
                (m.skills[r.skill].name.clone(), ["回数", "レベル", "ポケモン"], score_lines(&r.list))
            }
        };
        egui::Window::new(format!("ボックス: {title}")).id(egui::Id::new("checklist_detail")).open(&mut open).default_size([420.0, 480.0]).show(ctx, |ui| {
            TableBuilder::new(ui)
                .id_salt("checklist_detail_table")
                .striped(true)
                .column(Column::auto().at_least(80.0))
                .column(Column::auto().at_least(80.0))
                .column(Column::remainder())
                .header(22.0, |mut h| {
                    for t in header {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.rows(22.0, lines.len(), |mut row| {
                        let (idx, a, b) = &lines[row.index()];
                        row.col(|ui| {
                            ui.label(a);
                        });
                        row.col(|ui| {
                            ui.label(b);
                        });
                        row.col(|ui| self.pokemon_cell(ui, *idx));
                    });
                });
        });
        if !open {
            self.detail = None;
        }
    }

    // ---- 設定 ----

    fn setting_ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared, lv_list: &[u32]) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            for (tab, label) in [
                (SettingTab::Pokemon, "各ポケモン厳選基準"),
                (SettingTab::Food, "食材厳選基準"),
                (SettingTab::Skill, "スキル厳選基準"),
                (SettingTab::Other, "その他"),
            ] {
                ui.selectable_value(&mut self.setting_tab, tab, label);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.setting_tab {
            SettingTab::Pokemon => changed |= self.pokemon_setting(ui, &mut shared.config, lv_list),
            SettingTab::Food => changed |= food_setting(ui, &mut shared.config, lv_list),
            SettingTab::Skill => changed |= skill_setting(ui, &mut shared.config, lv_list),
            SettingTab::Other => {
                let f = &mut shared.config.summary.checklist.field;
                changed |= ui.checkbox(&mut f.bakeccha_matome, "バケッチャ・パンプジンをまとめて表示").changed();
                changed |= ui.checkbox(&mut f.shinkago, "フィールド別画面に進化後を表示").changed();
            }
        });
        changed |= self.target_window(ui.ctx(), &mut shared.config);
        changed |= self.hidden_window(ui.ctx(), &mut shared.config);
        changed
    }

    fn pokemon_setting(&mut self, ui: &mut egui::Ui, config: &mut Config, lv_list: &[u32]) -> bool {
        let mut changed = false;
        ui.heading("評価方法");
        ui.horizontal(|ui| {
            ui.label("基準レベル:");
            let pc = &mut config.summary.checklist.pokemon_condition;
            egui::ComboBox::from_id_salt("checklist_select_lv").selected_text(lv_label(&pc.select_lv)).show_ui(ui, |ui| {
                for v in lv_list.iter().map(|&lv| Value::from(lv)).chain([Value::from("max")]) {
                    let label = lv_label(&v);
                    changed |= ui.selectable_value(&mut pc.select_lv, v, label).changed();
                }
            });
        });
        let sim = &mut config.simulation;
        ui.horizontal(|ui| {
            changed |= ui.radio_value(&mut sim.select_type, 0.0, "厳選度").changed();
            changed |= ui.radio_value(&mut sim.select_type, 1.0, "2段階評価").changed();
            if sim.select_type == 1.0 {
                ui.label("厳選度");
                changed |= ui.add(egui::DragValue::new(&mut sim.select_border).range(0.0..=100.0)).changed();
                ui.label("%の出力に対して");
            }
        });
        ui.separator();

        let mut action: Option<(usize, i32)> = None; // (番号, -1:上へ 1:下へ 0:削除)
        let list = &mut config.summary.checklist.pokemon_condition.list;
        for (i, item) in list.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("条件 {}", i + 1));
                    if ui.small_button("上へ").clicked() {
                        action = Some((i, -1));
                    }
                    if ui.small_button("下へ").clicked() {
                        action = Some((i, 1));
                    }
                    if ui.small_button("削除").clicked() {
                        action = Some((i, 0));
                    }
                });
                egui::Grid::new(("checklist_cond", i)).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("対象");
                    let text = item.target.as_ref().map(pokemon_target::label).unwrap_or_else(|| "(未設定)".into());
                    if ui.button(text).on_hover_text("クリックで対象を設定").clicked() {
                        self.target_edit = Some(i);
                    }
                    ui.end_row();
                    ui.label("食材構成");
                    ui.horizontal(|ui| {
                        for (name, v) in item.food_flags_mut() {
                            changed |= ui.checkbox(v, name).changed();
                        }
                    });
                    ui.end_row();
                    ui.label("厳選度");
                    ui.horizontal(|ui| {
                        ui.label("総合厳選度");
                        changed |= opt_num(ui, ("cl_energy", i), &mut item.energy_border, "", 48.0);
                        ui.label("%以上");
                        ui.add_space(12.0);
                        ui.label("とくい厳選度");
                        changed |= opt_num(ui, ("cl_specialty", i), &mut item.specialty_border, "", 48.0);
                        ui.label("%以上");
                    });
                    ui.end_row();
                    ui.label("食材/スキル基準達成時");
                    changed |= ui.checkbox(&mut item.also_food_and_skill, "厳選済みとする").changed();
                    ui.end_row();
                });
            });
        }
        if let Some((i, op)) = action {
            match op {
                -1 if i > 0 => list.swap(i, i - 1),
                1 if i + 1 < list.len() => list.swap(i, i + 1),
                0 => {
                    list.remove(i);
                    self.target_edit = None;
                }
                _ => {}
            }
            changed = true;
        }
        ui.horizontal(|ui| {
            if ui.button("条件を追加").clicked() {
                list.push(PokemonCondition {
                    target: Some(PokemonTarget { target_type: Some("all".into()), all: true, ..Default::default() }),
                    aaa: true,
                    aab: true,
                    aac: true,
                    aba: true,
                    abb: true,
                    abc: true,
                    ..Default::default()
                });
                changed = true;
            }
            let hidden = config.summary.checklist.pokemon_condition.disable_pokemon_map.values().filter(|x| **x).count();
            let label = if hidden > 0 { format!("非表示ポケモン設定：{hidden}匹") } else { "非表示ポケモン設定".into() };
            if ui.button(label).clicked() {
                self.hidden_open = true;
            }
        });
        ui.label(egui::RichText::new("後の条件ほど優先されます(同じポケモンに複数の条件が当てはまる場合)").weak());
        changed
    }

    fn target_window(&mut self, ctx: &egui::Context, config: &mut Config) -> bool {
        let Some(i) = self.target_edit else { return false };
        let Some(item) = config.summary.checklist.pokemon_condition.list.get_mut(i) else {
            self.target_edit = None;
            return false;
        };
        let mut open = true;
        let mut changed = false;
        egui::Window::new(format!("条件 {} の対象", i + 1)).id(egui::Id::new("checklist_target")).open(&mut open).default_width(560.0).show(ctx, |ui| {
            let t = item.target.get_or_insert_with(Default::default);
            changed = pokemon_target::editor(ui, ("checklist_target", i), t);
        });
        if !open {
            self.target_edit = None;
        }
        changed
    }

    fn hidden_window(&mut self, ctx: &egui::Context, config: &mut Config) -> bool {
        if !self.hidden_open {
            return false;
        }
        let m = master();
        let mut list: Vec<usize> = m.pokemon.iter().filter(|p| p.is_last && !p.field_list.is_empty()).map(|p| p.index).collect();
        list.sort_by(|&a, &b| m.pokemon[a].name.encode_utf16().cmp(m.pokemon[b].name.encode_utf16()));
        let mut open = true;
        let mut changed = false;
        egui::Window::new("非表示ポケモン設定").open(&mut open).default_size([300.0, 560.0]).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let map = &mut config.summary.checklist.pokemon_condition.disable_pokemon_map;
                for p in list {
                    let name = &m.pokemon[p].name;
                    let v = map.entry(name.clone()).or_insert(false);
                    changed |= ui.checkbox(v, name).changed();
                }
            });
        });
        self.hidden_open = open;
        changed
    }
}

fn score_lines(list: &[ScoreEntry]) -> Vec<DetailLine> {
    list.iter().map(|x| (Some(x.index), fmt_fixed(x.score, 1), format!("Lv{}", x.lv))).collect()
}

fn border_lv_combo(ui: &mut egui::Ui, id: &str, value: &mut Option<f64>, lv_list: &[u32]) -> bool {
    let mut changed = false;
    let text = value.map(|x| format!("Lv. {x}")).unwrap_or_default();
    egui::ComboBox::from_id_salt(id).selected_text(text).show_ui(ui, |ui| {
        for &lv in lv_list {
            changed |= ui.selectable_value(value, Some(lv as f64), format!("Lv. {lv}")).changed();
        }
    });
    changed
}

const CAPTURE_HELP: &str = "食材やスキルの厳選は複数のポケモンを対象にすることが多く、2番手3番手のポケモンは捕まえても基準を満たしにくいため、捕まえるか悩むラインになります。\n\
この値は、そのポケモンの理論値の何%で基準を超えられれば捕獲対象とするかの設定です。\n\
例) 基準が10回/日で、あるポケモンの理論値が10回の場合、捕獲対象を80%にすると8回で判定するので、このポケモンは捕獲候補から外れます。";

fn food_setting(ui: &mut egui::Ui, config: &mut Config, lv_list: &[u32]) -> bool {
    let m = master();
    let f = &mut config.summary.checklist.food;
    let mut changed = false;
    ui.heading("対象食材");
    ui.horizontal_wrapped(|ui| {
        for food in &m.foods {
            let v = f.enable_map.entry(food.name.clone()).or_insert(false);
            ui.horizontal(|ui| {
                changed |= ui.checkbox(v, "").changed();
                food_icon::food_icon(ui, &food.name, 20.0);
                ui.label(&food.name);
            });
            ui.add_space(8.0);
        }
    });
    ui.separator();
    ui.heading("厳選基準");
    egui::Grid::new("checklist_food_setting").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("基準");
        ui.vertical(|ui| {
            changed |= ui.radio_value(&mut f.border_type, 0.0, "ポケモンの能力に対しての割合").changed();
            changed |= ui.radio_value(&mut f.border_type, 1.0, "必要最大数に対しての割合").changed();
            ui.horizontal(|ui| {
                changed |= border_lv_combo(ui, "checklist_food_lv", &mut f.border_lv, lv_list);
                ui.label("における各ポケモンの厳選度");
                changed |= opt_num(ui, "checklist_food_rate", &mut f.border_rate, "理論値", 48.0);
                ui.label("%");
            });
        });
        ui.end_row();
        ui.label("達成条件");
        ui.horizontal(|ui| {
            changed |= ui.add(egui::DragValue::new(&mut f.border_value).range(0.0..=1000.0)).changed();
            ui.label("%以上");
        });
        ui.end_row();
        ui.label("捕獲対象").on_hover_text(CAPTURE_HELP);
        ui.horizontal(|ui| {
            ui.label("そのポケモンの理論値の");
            changed |= ui.add(egui::DragValue::new(&mut f.target_value).range(0.0..=1000.0)).changed();
            ui.label("%以上が上記基準を超えるポケモン");
        });
        ui.end_row();
    });
    changed
}

fn skill_setting(ui: &mut egui::Ui, config: &mut Config, lv_list: &[u32]) -> bool {
    let m = master();
    let s = &mut config.summary.checklist.skill;
    let mut changed = false;
    ui.heading("対象スキル");
    changed |= ui.checkbox(&mut s.skill_specialty_only, "スキルとくいが持っていないスキルをまとめて除外").changed();
    for cat in &m.skill_categories {
        let skills: Vec<_> = m.skills.iter().filter(|x| x.category == cat.id).collect();
        if skills.is_empty() {
            continue;
        }
        ui.label(egui::RichText::new(&cat.name).strong());
        ui.horizontal_wrapped(|ui| {
            for skill in skills {
                let disabled = s.skill_specialty_only && !skill.skill_specialty_only;
                let v = s.enable_map.entry(skill.name.clone()).or_insert(false);
                if disabled {
                    let mut off = false;
                    ui.add_enabled(false, egui::Checkbox::new(&mut off, &skill.name));
                } else {
                    changed |= ui.checkbox(v, &skill.name).changed();
                }
            }
        });
    }
    ui.separator();
    ui.heading("厳選基準");
    egui::Grid::new("checklist_skill_setting").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("基準レベル");
        ui.horizontal(|ui| {
            changed |= border_lv_combo(ui, "checklist_skill_lv", &mut s.border_lv, lv_list);
            ui.label("における");
        });
        ui.end_row();
        ui.label("厳選度");
        ui.horizontal(|ui| {
            ui.label("各ポケモンの厳選度");
            changed |= opt_num(ui, "checklist_skill_rate", &mut s.border_rate, "理論値", 48.0);
            ui.label("%の");
            changed |= ui.add(egui::DragValue::new(&mut s.border_value).range(0.0..=1000.0)).changed();
            ui.label("%以上");
        });
        ui.end_row();
        ui.label("捕獲対象").on_hover_text(CAPTURE_HELP);
        ui.horizontal(|ui| {
            ui.label("そのポケモンの理論値の");
            changed |= ui.add(egui::DragValue::new(&mut s.target_value).range(0.0..=1000.0)).changed();
            ui.label("%以上が上記基準を超えるポケモン");
        });
        ui.end_row();
    });
    changed
}

fn field_table(ui: &mut egui::Ui, c: &Computed) {
    let rows = &c.field;
    let names: Vec<[String; 3]> = rows.iter().map(|r| std::array::from_fn(|i| r.pokemon[i].iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("\n"))).collect();
    let heights: Vec<f32> = names.iter().map(|n| 22.0_f32.max(n.iter().map(|x| x.lines().count()).max().unwrap_or(1) as f32 * 17.0 + 5.0)).collect();
    let geo = |v: [f64; 3]| (v[0] * v[1] * v[2]).powf(1.0 / 3.0);
    egui::ScrollArea::horizontal().show(ui, |ui| {
        TableBuilder::new(ui)
            .id_salt("checklist_field")
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(40.0), 13)
            .column(Column::remainder())
            .header(40.0, |mut h| {
                let titles = [
                    "フィールド".to_string(),
                    SLEEP_TYPES[0].into(),
                    SLEEP_TYPES[1].into(),
                    SLEEP_TYPES[2].into(),
                    format!("{}\n対象率", SLEEP_TYPES[0]),
                    format!("{}\n対象率", SLEEP_TYPES[1]),
                    format!("{}\n対象率", SLEEP_TYPES[2]),
                    "平均\n対象率".into(),
                    "相乗平均\n対象率".into(),
                    format!("{}\nエナジー", SLEEP_TYPES[0]),
                    format!("{}\nエナジー", SLEEP_TYPES[1]),
                    format!("{}\nエナジー", SLEEP_TYPES[2]),
                    "平均\nエナジー".into(),
                    "相乗平均\nエナジー".into(),
                ];
                for t in titles {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.heterogeneous_rows(heights.into_iter(), |mut row| {
                    let i = row.index();
                    let r = &rows[i];
                    row.col(|ui| {
                        ui.label(&r.name);
                    });
                    for n in &names[i] {
                        row.col(|ui| {
                            ui.label(n);
                        });
                    }
                    for k in 0..3 {
                        row.col(|ui| {
                            ui.label(percent(Some(r.rate[k]))).on_hover_text(format!("{} / {}匹", r.pokemon[k].len(), r.all_num[k]));
                        });
                    }
                    row.col(|ui| {
                        ui.label(percent(Some(r.rate.iter().sum::<f64>() / 3.0)));
                    });
                    row.col(|ui| {
                        ui.label(percent(Some(geo(r.rate))));
                    });
                    for k in 0..3 {
                        row.col(|ui| {
                            ui.label(fmt_fixed(r.energy[k], 1));
                        });
                    }
                    row.col(|ui| {
                        ui.label(fmt_fixed(r.energy.iter().sum::<f64>() / 3.0, 1));
                    });
                    row.col(|ui| {
                        ui.label(fmt_fixed(geo(r.energy), 1));
                    });
                });
            });
    });
}
