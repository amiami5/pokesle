//! チェックリスト画面(本家 pages/check-list.vue と check-list/*.vue)

use std::hash::{Hash, Hasher};

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use pokesle_core::box_sim::BoxResult;
use pokesle_core::check_list::{
    field_check_list, food_combination, with_mint_checked, CheckList, CheckListResult, CheckTarget, FieldRow, FoodCheckRow, PokemonCheckRow,
    ScoreEntry, SkillCheckRow, SLEEP_TYPES,
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
    /// 食材の取得数のグラフ(本家 check-list/food/_name.vue)
    FoodGraph(usize),
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
    job: Option<Job<(Vec<BoxResult>, usize)>>,
    /// 一覧用の概算結果(仮定計算なし・いつ育なし)。前半が通常個体、後半(mint_start以降)がミント後個体
    results: Vec<BoxResult>,
    mint_start: usize,
    /// 概算に使った設定・ボックス・厳選テーブルの指紋
    sim_key: Option<u64>,
    computed: Option<Computed>,
    unchecked_only: bool,
    detail: Option<Detail>,
    target_edit: Option<usize>,
    hidden_open: bool,
    error: String,
    /// 食材グラフの横軸の表示範囲(%)
    graph_x: (f64, f64),
}

impl Default for CheckListView {
    fn default() -> Self {
        CheckListView {
            tab: Tab::Setting,
            setting_tab: SettingTab::Pokemon,
            job: None,
            results: Vec::new(),
            mint_start: 0,
            sim_key: None,
            computed: None,
            unchecked_only: false,
            detail: None,
            target_edit: None,
            hidden_open: false,
            error: String::new(),
            graph_x: (0.0, 100.0),
        }
    }
}

/// HSL(色相は度、彩度・輝度は 0〜1)から色を作る
fn hsl(h: f32, s: f32, l: f32) -> egui::Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f32| ((v + m) * 255.0).round() as u8;
    egui::Color32::from_rgb(to(r), to(g), to(b))
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
    c.pure_mint = false;
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
    shared.config.summary.checklist.show_mint.hash(&mut h);
    shared.ui_setting.all_specialty.hash(&mut h);
    shared.box_file.time.hash(&mut h);
    shared.box_file.list.len().hash(&mut h);
    format!("{:?}{:?}", table.healer, table.support).hash(&mut h);
    table.pokemon.len().hash(&mut h);
    h.finish()
}

/// ボックスのポケモンの短い表示(名前 Lv)
fn pokemon_text(r: &BoxResult, is_mint: bool) -> String {
    let b = r.pokemon.box_pokemon.as_ref();
    let name = b.map(|b| b.name.as_str()).unwrap_or(&r.pokemon.base().name);
    if is_mint {
        format!("{name} Lv{} 🌿", r.pokemon.lv)
    } else {
        format!("{name} Lv{}", r.pokemon.lv)
    }
}

fn pokemon_hover(r: &BoxResult, is_mint: bool) -> String {
    let Some(b) = r.pokemon.box_pokemon.as_ref() else { return String::new() };
    let nature = if is_mint { "🌿 ミント使用後 (がんばりや)".to_string() } else { b.nature.clone() };
    format!("ボックス No.{}\n{}\n{}\n{}", b.index + 1, nature, b.food_list.join(" / "), b.sub_skill_list.join(" / "))
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
                (Tab::Setting, "厳選基準設定"),
                (Tab::Pokemon, "ポケモン"),
                (Tab::Food, "食材"),
                (Tab::Skill, "スキル"),
                (Tab::Field, "フィールド別"),
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
            let config = page_config(&shared.sim_config());
            let mut list = shared.box_file.list.clone();
            let mint_start = list.len();
            if shared.config.summary.checklist.show_mint {
                let m = master();
                let mint_list: Vec<_> = shared
                    .box_file
                    .list
                    .iter()
                    .filter(|p| m.nature(&p.nature).is_some_and(|n| n.good.is_some()))
                    .map(|p| {
                        let mut mp = p.clone();
                        mp.nature = "がんばりや".to_string();
                        mp
                    })
                    .collect();
                list.extend(mint_list);
            }
            let t = table.clone();
            self.sim_key = Some(key);
            self.job = Some(Job::spawn(ui.ctx(), move |p| {
                p.set(0.0, "ボックスの厳選度を計算しています…");
                simulate_box(&config, &list, Some(&t), true).map(|r| (r, mint_start))
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
            self.computed = Some(self.compute(&shared.sim_config(), &table));
        }
        let Some(c) = &self.computed else { return };
        let open = match self.tab {
            Tab::Pokemon => self.pokemon_table(ui, c),
            Tab::Food => self.food_table(ui, c),
            Tab::Skill => self.skill_table(ui, c),
            Tab::Field => {
                ui.label("各エナジー列は厳選基準生成で計算したエナジーの期待値です。単に未厳選が多いフィールドではなく、優先して厳選したい強いポケモンが多く出るフィールドに行きたい時に参考にしてください。");
                ui.add_space(4.0);
                field_table(ui, c);
                None
            }
            Tab::Setting => None,
        };
        if let Some(d) = open {
            if matches!(d, Detail::FoodGraph(_)) {
                self.graph_x = (0.0, 100.0);
            }
            self.detail = Some(d);
        }
        if let Some(Detail::FoodGraph(i)) = self.detail {
            self.graph_window(ui.ctx(), &shared.config, shared.usable_evaluate_table(), i);
        } else {
            self.detail_window(ui.ctx(), &shared.config);
        }
    }

    fn poll_job(&mut self) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            self.computed = None;
            match r {
                Ok((list, mint_start)) => {
                    self.results = list;
                    self.mint_start = mint_start;
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
        let normal_len = self.mint_start.min(targets.len());
        let (food, skill, pokemon) = if config.summary.checklist.show_mint && normal_len < targets.len() {
            let normal_targets = &targets[..normal_len];
            let mint_targets = &targets[normal_len..];
            let normal_food = cl.food_check_list(normal_targets, None);
            let normal_skill = cl.skill_check_list(normal_targets, None);
            let normal_pokemon = cl.pokemon_check_list(normal_targets, None, Some(&normal_food), Some(&normal_skill));
            let mint_food = cl.food_check_list(mint_targets, None);
            let mint_skill = cl.skill_check_list(mint_targets, None);
            let mint_pokemon = cl.pokemon_check_list(mint_targets, None, Some(&mint_food), Some(&mint_skill));
            let combined_food = cl.food_check_list(&targets, None);
            let combined_skill = cl.skill_check_list(&targets, None);
            let combined_pokemon = cl.pokemon_check_list(&targets, None, Some(&combined_food), Some(&combined_skill));
            let exclude = config.summary.checklist.field.exclude_mint;
            (
                with_mint_checked(&normal_food, &mint_food, combined_food, exclude),
                with_mint_checked(&normal_skill, &mint_skill, combined_skill, exclude),
                with_mint_checked(&normal_pokemon, &mint_pokemon, combined_pokemon, exclude),
            )
        } else {
            let normal_targets = &targets[..normal_len];
            let food = cl.food_check_list(normal_targets, None);
            let skill = cl.skill_check_list(normal_targets, None);
            let pokemon = cl.pokemon_check_list(normal_targets, None, Some(&food), Some(&skill));
            (food, skill, pokemon)
        };
        let all_targets: Vec<String> =
            pokemon.target_pokemon_list.iter().chain(&food.target_pokemon_list).chain(&skill.target_pokemon_list).cloned().collect();
        let field = field_check_list(config, Some(table), &all_targets);
        Computed { pokemon, food, skill, field }
    }

    fn result(&self, index: usize) -> Option<&BoxResult> {
        self.results.get(index)
    }

    fn pokemon_cell(&self, ui: &mut egui::Ui, index: Option<usize>) {
        if let Some(i) = index
            && let Some(r) = self.results.get(i)
        {
            let is_mint = i >= self.mint_start;
            ui.label(pokemon_text(r, is_mint)).on_hover_text(pokemon_hover(r, is_mint));
        }
    }

    fn checked_cell(ui: &mut egui::Ui, checked: bool, mint_checked: bool) {
        if checked {
            ui.colored_label(egui::Color32::from_rgb(40, 170, 80), "済");
        } else if mint_checked {
            ui.colored_label(egui::Color32::from_rgb(40, 170, 80), "済（🌿）");
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
                        row.col(|ui| Self::checked_cell(ui, r.checked, r.mint_checked));
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
                        row.col(|ui| Self::checked_cell(ui, r.checked, r.mint_checked));
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
                        row.col(|ui| {
                            if ui.small_button("グラフ").on_hover_text("全食材構成×全サブスキル×全せいかくの食材取得数のグラフ").clicked() {
                                open = Some(Detail::FoodGraph(i));
                            }
                        });
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
                        row.col(|ui| Self::checked_cell(ui, r.checked, r.mint_checked));
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

    /// 食材の取得数のグラフ。全食材構成×全サブスキル×全せいかくの分布を、ポケモンごとの線で出す。
    /// 赤線は設定した厳選基準、青線はボックスの最良(本家と同じく2段階評価のときだけ)。
    /// マウスホイールで横軸を拡大・縮小、ドラッグで横に移動する
    fn graph_window(&mut self, ctx: &egui::Context, config: &Config, table: Option<&EvaluateTable>, row: usize) {
        let (Some(c), Some(table)) = (&self.computed, table) else { return };
        let Some(r) = c.food.rows.get(row) else { return };
        let m = master();
        let food_name = m.foods[r.food].name.clone();
        let lv = config.summary.checklist.food.border_lv.map(|v| v as u32);
        let (border, best) = (r.border, r.score);

        // ポケモンごとの点列(昇順に並べた個数を、0〜100% の位置に並べる)
        struct Line {
            label: String,
            points: Vec<(f64, f64)>,
        }
        let mut lines: Vec<Line> = Vec::new();
        for (name, by_lv) in &table.pokemon {
            let Some(pokemon) = m.pokemon(name) else { continue };
            let Some(combos) = lv.and_then(|lv| by_lv.get(&lv)) else { continue };
            let mut matched = false;
            let mut scores: Vec<f64> = Vec::new();
            for set in combos.values() {
                for (index, food) in pokemon.food_list.iter().enumerate() {
                    let list = match index {
                        0 => &set.food1,
                        1 => &set.food2,
                        _ => &set.food3,
                    };
                    if list.is_empty() || food.name != food_name {
                        continue;
                    }
                    if list.iter().any(|s| border <= *s) {
                        matched = true;
                    }
                    scores.extend(list.iter().copied());
                }
            }
            if matched && !scores.is_empty() {
                scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let n = scores.len();
                let last = scores[n - 1];
                let points = scores.iter().enumerate().map(|(i, s)| (if n > 1 { i as f64 / (n - 1) as f64 * 100.0 } else { 0.0 }, *s)).collect();
                lines.push(Line { label: format!("{name}({})", fmt_fixed(last, 1)), points });
            }
        }
        lines.sort_by(|a, b| b.points.last().map(|p| p.1).partial_cmp(&a.points.last().map(|p| p.1)).unwrap_or(std::cmp::Ordering::Equal));

        let mut open = true;
        egui::Window::new(format!("{food_name}の取得数")).id(egui::Id::new("checklist_food_graph")).open(&mut open).default_size([820.0, 600.0]).show(ctx, |ui| {
            ui.label("全食材構成×全サブスキル×全せいかくの組み合わせの食材取得数のグラフです。赤線は設定した厳選基準のスコア、青線はあなたのボックスにいる最良のポケモンのスコアです。ホイールで横軸を拡大・縮小、ドラッグで横に移動できます。");
            let size = egui::vec2(ui.available_width().max(400.0), 380.0);
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
            let plot = egui::Rect::from_min_max(rect.min + egui::vec2(48.0, 8.0), rect.max - egui::vec2(10.0, 26.0));
            let painter = ui.painter_at(rect);
            let (mut x0, mut x1) = self.graph_x;

            // 縦軸は全データの範囲
            let show_lines = config.simulation.select_type == 1.0;
            let mut y_min = f64::INFINITY;
            let mut y_max = f64::NEG_INFINITY;
            for l in &lines {
                for (_, y) in &l.points {
                    y_min = y_min.min(*y);
                    y_max = y_max.max(*y);
                }
            }
            if show_lines {
                for v in [border, best] {
                    if v.is_finite() {
                        y_min = y_min.min(v);
                        y_max = y_max.max(v);
                    }
                }
            }
            if !y_min.is_finite() || y_max <= y_min {
                y_min = 0.0;
                y_max = y_max.max(1.0);
            }

            // 拡大・縮小(ポインタの位置を中心に)とドラッグでの移動
            let range = x1 - x0;
            if resp.hovered() {
                let wheel = ui.input(|i| i.smooth_scroll_delta.y);
                if wheel != 0.0
                    && let Some(pos) = resp.hover_pos()
                {
                    let pos_rate = ((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0) as f64;
                    let next = (range * if wheel > 0.0 { 0.8 } else { 1.25 }).clamp(1.0, 100.0);
                    let at = x0 + range * pos_rate;
                    x0 = (at - next * pos_rate).clamp(0.0, 100.0 - next);
                    x1 = x0 + next;
                }
            }
            if resp.dragged() {
                let dx = -resp.drag_delta().x as f64 / plot.width() as f64 * (x1 - x0);
                let r = x1 - x0;
                x0 = (x0 + dx).clamp(0.0, 100.0 - r);
                x1 = x0 + r;
            }
            self.graph_x = (x0, x1);

            let to_px = |x: f64, y: f64| {
                egui::pos2(
                    plot.left() + ((x - x0) / (x1 - x0)) as f32 * plot.width(),
                    plot.bottom() - ((y - y_min) / (y_max - y_min)) as f32 * plot.height(),
                )
            };
            let grid = super::style::line_color();
            let muted = super::style::muted();
            let font = egui::FontId::proportional(11.0);
            for k in 0..=5 {
                let y = y_min + (y_max - y_min) * k as f64 / 5.0;
                let pos = to_px(x0, y);
                painter.hline(plot.x_range(), pos.y, egui::Stroke::new(1.0, grid));
                painter.text(egui::pos2(plot.left() - 4.0, pos.y), egui::Align2::RIGHT_CENTER, fmt_fixed(y, 1), font.clone(), muted);
            }
            for k in 0..=5 {
                let x = x0 + (x1 - x0) * k as f64 / 5.0;
                let pos = to_px(x, y_min);
                painter.vline(pos.x, plot.y_range(), egui::Stroke::new(1.0, grid));
                painter.text(egui::pos2(pos.x, plot.bottom() + 4.0), egui::Align2::CENTER_TOP, format!("{x:.0}"), font.clone(), muted);
            }
            let clip = ui.painter_at(plot);
            let total = lines.len().max(1);
            for (n, l) in lines.iter().enumerate() {
                let color = hsl(n as f32 * 360.0 / total as f32, 1.0, 0.5);
                let pts: Vec<egui::Pos2> = l.points.iter().map(|(x, y)| to_px(*x, *y)).collect();
                clip.add(egui::Shape::line(pts, egui::Stroke::new(1.5, color)));
            }
            if show_lines {
                for (v, color) in [(border, egui::Color32::RED), (best, egui::Color32::BLUE)] {
                    if v.is_finite() {
                        clip.hline(plot.x_range(), to_px(x0, v).y, egui::Stroke::new(1.0, color));
                    }
                }
            }

            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (n, l) in lines.iter().enumerate() {
                        let color = hsl(n as f32 * 360.0 / total as f32, 1.0, 0.5);
                        let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 2.0, color);
                        ui.label(&l.label);
                        ui.add_space(8.0);
                    }
                });
            });
        });
        if !open {
            self.detail = None;
        }
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
            // グラフは別の窓(graph_window)で描く
            Detail::FoodGraph(_) => return,
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
                let cl = &mut shared.config.summary.checklist;
                changed |= ui.checkbox(&mut cl.show_mint, "ミント使用後も一覧に表示する").changed();
                changed |= ui
                    .add_enabled(
                        cl.show_mint,
                        egui::Checkbox::new(&mut cl.field.exclude_mint, "ミント使用後がチェックリストに該当した場合、フィールド別からも除外"),
                    )
                    .changed();
                changed |= ui.checkbox(&mut cl.field.bakeccha_matome, "バケッチャ・パンプジンをまとめて表示").changed();
                changed |= ui.checkbox(&mut cl.field.shinkago, "フィールド別画面に進化後を表示").changed();
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
