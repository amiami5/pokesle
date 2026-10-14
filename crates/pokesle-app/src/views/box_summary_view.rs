//! サマリー画面(本家 `pages/box-summary.vue` と `box-summary/*.vue`)

use eframe::egui;
use pokesle_core::box_summary::{self, EvaluateType, PokemonSummary, COLUMN_NAMES};
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::{BoxPokemon, SimPokemon};

use super::{common_setting, fmt_fixed, food_icon, style, team_result};
use crate::app::Shared;
use crate::job::Job;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SummaryTab {
    Pokemon,
    Berry,
    Food,
    Skill,
}

const TABS: [(SummaryTab, &str); 4] = [(SummaryTab::Pokemon, "ポケモン"), (SummaryTab::Berry, "きのみ"), (SummaryTab::Food, "食材"), (SummaryTab::Skill, "スキル")];

/// 表のセル
#[derive(Clone)]
enum Cell {
    Text(String),
    Num(Option<f64>, usize),
    /// 割合(0〜1)と、その下に小さく出す値
    Score(Option<f64>, Option<f64>),
    Pokemon(Option<Box<SimPokemon>>),
    /// 食材のポケモン(名前)と食材構成
    Foods(String, Vec<String>),
    /// 食材の取得数目安(数量, ポケモン名, 食材構成)
    FoodMax(f64, String, Vec<String>),
    /// スキルの最大(数量, ポケモン名)
    SkillMax(f64, String),
}

impl Cell {
    /// 並べ替えの値。None は常に末尾
    fn sort_key(&self) -> SortKey {
        match self {
            Cell::Text(s) => SortKey::Str(s.clone()),
            Cell::Num(v, _) => v.map_or(SortKey::None, SortKey::Num),
            Cell::Score(v, _) => v.map_or(SortKey::None, SortKey::Num),
            Cell::Pokemon(p) => p.as_ref().map_or(SortKey::None, |p| SortKey::Str(p.base().name.clone())),
            Cell::Foods(..) => SortKey::None,
            Cell::FoodMax(v, ..) | Cell::SkillMax(v, _) => SortKey::Num(*v),
        }
    }
}

enum SortKey {
    None,
    Num(f64),
    Str(String),
}

struct Column {
    header: String,
    width: f32,
}

struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
}

enum Output {
    Pokemon(PokemonSummary),
    Berry(Vec<box_summary::BerryRow>),
    Food(Vec<box_summary::FoodRow>),
    Skill(Vec<box_summary::SkillRow>),
}

pub struct BoxSummaryView {
    tab: SummaryTab,
    /// 食材構成ごと
    each_food: bool,
    show_food_num: bool,
    show_box_detail: bool,
    evaluate_type: EvaluateType,
    /// 食材タブの表示数量(0: 最大要求数に対する割合、1: 取得数目安に対する割合)
    food_mode: u8,
    job: Option<(SummaryTab, Job<Output>)>,
    output: [Option<Output>; 4],
    /// 再計算が必要なタブ
    stale: [bool; 4],
    error: String,
    sort: Option<(usize, bool)>,
    box_time: Option<String>,
}

impl Default for BoxSummaryView {
    fn default() -> Self {
        // 見た目の確認用(デバッグ版のみ): POKESLE_DEBUG_SUMMARY=berry / food / skill で開くタブを指定する
        #[cfg(debug_assertions)]
        let tab = match std::env::var("POKESLE_DEBUG_SUMMARY").as_deref() {
            Ok("berry") => SummaryTab::Berry,
            Ok("food") => SummaryTab::Food,
            Ok("skill") => SummaryTab::Skill,
            _ => SummaryTab::Pokemon,
        };
        #[cfg(not(debug_assertions))]
        let tab = SummaryTab::Pokemon;
        BoxSummaryView {
            tab,
            each_food: false,
            show_food_num: true,
            show_box_detail: true,
            evaluate_type: EvaluateType::Energy,
            food_mode: 0,
            job: None,
            output: [None, None, None, None],
            stale: [true; 4],
            error: String::new(),
            sort: None,
            box_time: None,
        }
    }
}

fn index(tab: SummaryTab) -> usize {
    match tab {
        SummaryTab::Pokemon => 0,
        SummaryTab::Berry => 1,
        SummaryTab::Food => 2,
        SummaryTab::Skill => 3,
    }
}

impl BoxSummaryView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        // 計算結果の受け取り
        if let Some((tab, job)) = &mut self.job
            && let Some(r) = job.poll()
        {
            let tab = *tab;
            self.job = None;
            match r {
                Ok(o) => {
                    self.output[index(tab)] = Some(o);
                    self.error.clear();
                }
                Err(e) => self.error = e,
            }
        }
        // ボックスが変わったら全タブを計算し直す
        if self.box_time != shared.box_file.time {
            self.box_time = shared.box_file.time.clone();
            self.stale = [true; 4];
        }

        // タブ
        let mut tab_changed = false;
        let labels: Vec<String> = TABS.iter().map(|(_, l)| l.to_string()).collect();
        let active = TABS.iter().position(|(t, _)| *t == self.tab).unwrap_or(0);
        if let Some(i) = style::tab_list(ui, &labels, active)
            && TABS[i].0 != self.tab
        {
            self.tab = TABS[i].0;
            self.sort = None;
            tab_changed = true;
        }
        ui.add_space(6.0);

        // 共通設定(変わったら全タブを計算し直す)
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            changed |= common_setting::ui(ui, &mut shared.config, &shared.box_file.list);
        });
        if changed {
            shared.save_config();
            self.stale = [true; 4];
        }
        ui.add_space(4.0);

        match self.tab {
            SummaryTab::Pokemon => {
                let mut opt_changed = false;
                ui.horizontal_wrapped(|ui| {
                    opt_changed |= ui.checkbox(&mut self.each_food, "食材構成ごと").changed();
                    ui.checkbox(&mut self.show_food_num, "食材数表示").on_hover_text("サブスキル、せいかくを考慮せずにLv60になった場合の食材数です。");
                    ui.checkbox(&mut self.show_box_detail, "個体詳細表示");
                    ui.add_space(8.0);
                    opt_changed |= ui.radio_value(&mut self.evaluate_type, EvaluateType::Energy, "総合スコア").changed();
                    opt_changed |= ui.radio_value(&mut self.evaluate_type, EvaluateType::Specialty, "とくい分野").changed();
                });
                if opt_changed {
                    self.stale[0] = true;
                }
            }
            SummaryTab::Berry => style::alert(ui, "このページでは育成仮定が強制的にONになります。"),
            SummaryTab::Food => {
                style::alert(
                    ui,
                    "このページでは育成仮定が強制的にONになります。\n最大要求数はその食材を最も使用するレシピの数量です。取得数目安はサブスキルやせいかく補正抜きで最もその食材を持ってくるポケモンとその数量です。\n各ポケモンの数量は選択した基準に対しての%です。ポケモン1匹で3食作りたい場合、最大要求数に対して300%以上が目安になります。",
                );
                ui.horizontal(|ui| {
                    ui.label("表示数量：");
                    ui.radio_value(&mut self.food_mode, 0, "最大要求数に対する割合");
                    ui.radio_value(&mut self.food_mode, 1, "取得数目安に対する割合");
                });
            }
            SummaryTab::Skill => style::alert(
                ui,
                "このページでは育成仮定が強制的にONになります。\n最大はスキル確率S+M+なまいき+おてつだいスピード35%短縮+Lv100で計算した時のスキル発動回数です。\n各ポケモンの数量は最大に対しての%です。現環境では60%前後が厳選終了の目安になるかと思われます。",
            ),
        }
        let _ = tab_changed;

        // 必要なら計算を始める
        let i = index(self.tab);
        if self.stale[i] && self.job.is_none() {
            self.stale[i] = false;
            self.output[i] = None;
            let config = shared.config.clone();
            let list = shared.box_file.list.clone();
            let table = shared.usable_evaluate_table().cloned();
            let (each_food, ty, tab) = (self.each_food, self.evaluate_type, self.tab);
            self.job = Some((
                tab,
                Job::spawn(ui.ctx(), move |p| {
                    p.set(0.0, "ボックスを集計しています…");
                    let on = |v: f64| p.set(v, "");
                    let t = table.as_ref();
                    Ok(match tab {
                        SummaryTab::Pokemon => Output::Pokemon(box_summary::pokemon_summary(&config, &list, t, each_food, ty, &on)?),
                        SummaryTab::Berry => Output::Berry(box_summary::berry_summary(&config, &list, t, &on)?),
                        SummaryTab::Food => Output::Food(box_summary::food_summary(&config, &list, t, &on)?),
                        SummaryTab::Skill => Output::Skill(box_summary::skill_summary(&config, &list, t, &on)?),
                    })
                }),
            ));
        } else if self.stale[i] {
            // 別タブの計算が終わるのを待つ
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }

        if let Some((_, job)) = &self.job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        if self.tab == SummaryTab::Pokemon && shared.usable_evaluate_table().is_none() {
            ui.label("厳選テーブルがないため、厳選度は表示されません(基準生成で作成してください)");
        }
        if shared.box_file.list.is_empty() {
            ui.label("ボックスにポケモンを登録してください");
        }

        let Some(output) = &self.output[i] else { return };
        let table = match output {
            Output::Pokemon(s) => self.pokemon_table(s),
            Output::Berry(rows) => berry_table(rows),
            Output::Food(rows) => food_table(rows, self.food_mode),
            Output::Skill(rows) => skill_table(rows),
        };
        table_ui(ui, &table, &mut self.sort, i);
    }

    fn pokemon_table(&self, s: &PokemonSummary) -> Table {
        let m = master();
        let mut columns = vec![
            Column { header: "名前".into(), width: 130.0 },
            Column { header: "タイプ".into(), width: 60.0 },
            Column { header: "とくい".into(), width: 60.0 },
        ];
        if self.each_food {
            columns.push(Column { header: "食材".into(), width: 90.0 });
            if self.show_food_num {
                for f in &m.foods {
                    columns.push(Column { header: f.name.clone(), width: 48.0 });
                }
            }
        }
        columns.push(Column { header: "スキル".into(), width: 130.0 });
        columns.push(Column { header: "厳選(最大)".into(), width: 80.0 });
        if self.show_box_detail {
            columns.push(Column { header: "厳選(最大)\n1位ポケモン".into(), width: 330.0 });
        }
        for lv in &s.lv_list {
            columns.push(Column { header: format!("厳選({lv})"), width: 80.0 });
            if self.show_box_detail {
                columns.push(Column { header: format!("厳選({lv})\n1位ポケモン"), width: 330.0 });
            }
        }

        let rows = s
            .rows
            .iter()
            .map(|r| {
                let base = &m.pokemon[r.base];
                let mut row = vec![Cell::Text(base.name.clone()), Cell::Text(base.type_name.clone()), Cell::Text(base.specialty.name().to_string())];
                if self.each_food {
                    row.push(Cell::Foods(base.name.clone(), r.food_list.clone()));
                    if self.show_food_num {
                        for fi in 0..m.foods.len() {
                            row.push(Cell::Num(r.food_count.get(fi).copied(), 1));
                        }
                    }
                }
                row.push(Cell::Text(m.skills[base.skill].name.clone()));
                row.push(Cell::Score(r.max.value, None));
                if self.show_box_detail {
                    row.push(Cell::Pokemon(r.max.pokemon.clone().map(Box::new)));
                }
                for (_, ranked) in &r.by_lv {
                    row.push(Cell::Score(ranked.value, None));
                    if self.show_box_detail {
                        row.push(Cell::Pokemon(ranked.pokemon.clone().map(Box::new)));
                    }
                }
                row
            })
            .collect();
        Table { columns, rows }
    }
}

fn berry_table(rows: &[box_summary::BerryRow]) -> Table {
    let mut columns = vec![Column { header: "名前".into(), width: 130.0 }, Column { header: "タイプ".into(), width: 60.0 }];
    for name in COLUMN_NAMES {
        let prefix = if name.is_empty() { String::new() } else { format!("{name}\n") };
        columns.push(Column { header: format!("{prefix}エナジー/日"), width: 90.0 });
        columns.push(Column { header: format!("{prefix}1位ポケモン"), width: 330.0 });
    }
    let rows = rows
        .iter()
        .map(|r| {
            let mut row = vec![Cell::Text(r.name.clone()), Cell::Text(r.type_name.clone())];
            for c in &r.cols {
                row.push(Cell::Num(c.value, 0));
                row.push(Cell::Pokemon(c.pokemon.clone().map(Box::new)));
            }
            row
        })
        .collect();
    Table { columns, rows }
}

fn food_table(rows: &[box_summary::FoodRow], mode: u8) -> Table {
    let m = master();
    let mut columns = vec![
        Column { header: "名前".into(), width: 130.0 },
        Column { header: "最大\n要求数".into(), width: 60.0 },
        Column { header: "取得数目安\n(ポケモン)".into(), width: 130.0 },
    ];
    for name in COLUMN_NAMES {
        let prefix = if name.is_empty() { String::new() } else { format!("{name}\n") };
        columns.push(Column { header: format!("{prefix}数量/日"), width: 90.0 });
        columns.push(Column { header: format!("{prefix}1位ポケモン"), width: 330.0 });
    }
    let rows = rows
        .iter()
        .map(|r| {
            let max_name = r.max_pokemon.map(|i| m.pokemon[i].name.clone()).unwrap_or_default();
            let mut row = vec![Cell::Text(r.name.clone()), Cell::Num(Some(r.require), 0), Cell::FoodMax(r.max, max_name, r.max_food_list.clone())];
            for (i, c) in r.cols.iter().enumerate() {
                row.push(Cell::Score(r.score(i, mode), Some(c.value.unwrap_or(0.0))));
                row.push(Cell::Pokemon(c.pokemon.clone().map(Box::new)));
            }
            row
        })
        .collect();
    Table { columns, rows }
}

fn skill_table(rows: &[box_summary::SkillRow]) -> Table {
    let mut columns = vec![Column { header: "名前".into(), width: 150.0 }, Column { header: "最大".into(), width: 110.0 }];
    for name in COLUMN_NAMES {
        let prefix = if name.is_empty() { String::new() } else { format!("{name}\n") };
        columns.push(Column { header: format!("{prefix}数量/日"), width: 90.0 });
        columns.push(Column { header: format!("{prefix}1位ポケモン"), width: 330.0 });
    }
    let rows = rows
        .iter()
        .map(|r| {
            let mut row = vec![Cell::Text(r.name.clone()), Cell::SkillMax(r.max, r.max_pokemon.clone().unwrap_or_default())];
            for (i, c) in r.cols.iter().enumerate() {
                row.push(Cell::Score(r.score(i), Some(c.value.unwrap_or(0.0))));
                row.push(Cell::Pokemon(c.pokemon.clone().map(Box::new)));
            }
            row
        })
        .collect();
    Table { columns, rows }
}

fn table_ui(ui: &mut egui::Ui, table: &Table, sort: &mut Option<(usize, bool)>, id: usize) {
    // 並べ替え(値が無いものは常に末尾)
    let mut order: Vec<usize> = (0..table.rows.len()).collect();
    if let Some((col, desc)) = *sort {
        order.sort_by(|&a, &b| {
            use std::cmp::Ordering::*;
            match (table.rows[a][col].sort_key(), table.rows[b][col].sort_key()) {
                (SortKey::Num(x), SortKey::Num(y)) => {
                    let o = x.partial_cmp(&y).unwrap_or(Equal);
                    if desc { o.reverse() } else { o }
                }
                (SortKey::Str(x), SortKey::Str(y)) => {
                    let o = x.cmp(&y);
                    if desc { o.reverse() } else { o }
                }
                (SortKey::None, SortKey::None) => Equal,
                (SortKey::None, _) => Greater,
                (_, SortKey::None) => Less,
                _ => Equal,
            }
        });
    }

    egui::ScrollArea::both().id_salt(("summary_scroll", id)).auto_shrink(false).show(ui, |ui| {
        egui::Grid::new(("summary_grid", id)).striped(true).spacing([10.0, 6.0]).show(ui, |ui| {
            for (ci, c) in table.columns.iter().enumerate() {
                let mark = match *sort {
                    Some((s, desc)) if s == ci => if desc { " ▼" } else { " ▲" },
                    _ => "",
                };
                let text = egui::RichText::new(format!("{}{mark}", c.header)).strong().color(style::th_color());
                let r = ui.add_sized([c.width, 34.0], egui::Label::new(text).sense(egui::Sense::click()));
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    *sort = match *sort {
                        Some((s, false)) if s == ci => Some((ci, true)),
                        Some((s, true)) if s == ci => None,
                        _ => Some((ci, false)),
                    };
                }
            }
            ui.end_row();
            for &ri in &order {
                for (ci, cell) in table.rows[ri].iter().enumerate() {
                    cell_ui(ui, cell, table.columns[ci].width);
                }
                ui.end_row();
            }
        });
    });
}

fn cell_ui(ui: &mut egui::Ui, cell: &Cell, width: f32) {
    ui.vertical(|ui| {
        ui.set_min_width(width);
        match cell {
            Cell::Text(s) => {
                ui.label(s);
            }
            Cell::Num(v, digits) => {
                if let Some(v) = v {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| ui.label(fmt_fixed(*v, *digits)));
                }
            }
            Cell::Score(v, sub) => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    if let Some(v) = v {
                        ui.label(format!("{}%", fmt_fixed(v * 100.0, 1)));
                    }
                    if let Some(s) = sub {
                        ui.label(egui::RichText::new(format!("({})", fmt_fixed(*s, 1))).size(11.0).color(style::muted()));
                    }
                });
            }
            Cell::Pokemon(p) => {
                if let Some(p) = p {
                    pokemon_info(ui, p);
                }
            }
            Cell::Foods(name, foods) => foods_icons(ui, name, 60.0, foods),
            Cell::FoodMax(v, name, foods) => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.label(fmt_fixed(*v, 1));
                    ui.label(egui::RichText::new(format!("({name})")).size(11.0).color(style::muted()));
                });
                foods_icons(ui, name, 60.0, foods);
            }
            Cell::SkillMax(v, name) => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.label(fmt_fixed(*v, 1));
                    ui.label(egui::RichText::new(format!("({name})")).size(11.0).color(style::muted()));
                });
            }
        }
    });
}

fn foods_icons(ui: &mut egui::Ui, name: &str, lv: f64, foods: &[String]) {
    let p = BoxPokemon { name: name.to_string(), lv, food_list: foods.to_vec(), ..Default::default() };
    ui.horizontal(|ui| food_icon::food_list(ui, &p));
}

/// ポケモン1匹(本家 pokemon-info.vue): 「No：名前(Lv)」と食材、その下にサブスキル
fn pokemon_info(ui: &mut egui::Ui, p: &SimPokemon) {
    let Some(bx) = &p.box_pokemon else { return };
    ui.horizontal(|ui| {
        ui.label(format!("{}：{} (Lv{})", bx.index + 1, p.base().name, p.lv));
        let mut shown = bx.clone();
        shown.lv = p.lv;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.horizontal(|ui| food_icon::food_list(ui, &shown));
        });
    });
    team_result::sub_skill_cell(ui, p, true);
}
