//! 基準確認画面(本家 `pages/evaluate-table.vue`)
//!
//! 作成済みの厳選テーブルを、グラフ(タイプ×とくいごとの散布図)または一覧表で確認する。
//! 表の厳選度のセルを選ぶと、その個体(食材構成・サブスキル・せいかく)の厳選詳細を開く。
//! 未実装: 一覧表の列固定(fixColumn)。

use std::collections::BTreeMap;
use std::sync::Arc;

use eframe::egui;
use pokesle_core::data::{master, Pokemon};
use pokesle_core::evaluate_table::{evaluate_energy_detail, sub_skill_combination_list, EnergyDetail, EvaluateTable, SubSkillCombinationList};
use pokesle_core::sim_pokemon::BoxPokemon;

use super::evaluate_detail::{fmt_tick, nice_range, EvaluateDetail};
use super::{combo, fmt_num, food_icon, style};
use crate::app::Shared;
use crate::job::Job;

const ROW_HEIGHT: f32 = 30.0;
const NAME_W: f32 = 130.0;
const TYPE_W: f32 = 70.0;
const SPECIALTY_W: f32 = 60.0;
const SKILL_W: f32 = 140.0;
const FOOD_W: f32 = 96.0;
const PERCENTILE_W: f32 = 64.0;
/// グラフの最小幅(本家のモバイル表示と同じ。これより狭いときは横にスクロールする)
const GRAPH_MIN_WIDTH: f32 = 1680.0;
const GRAPH_MIN_HEIGHT: f32 = 500.0;
const SPECIALTY_LIST: [&str; 4] = ["きのみ", "食材", "スキル", "オール"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewTab {
    Graph,
    Table,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GraphAxis {
    Type,
    Specialty,
    All,
}

impl GraphAxis {
    const ALL: [(GraphAxis, &str); 3] = [(GraphAxis::Type, "タイプ別"), (GraphAxis::Specialty, "とくい別"), (GraphAxis::All, "全体")];

    fn label(self) -> &'static str {
        GraphAxis::ALL.iter().find(|(a, _)| *a == self).map_or("タイプ別", |(_, l)| *l)
    }
}

/// 一覧の1行(ポケモン × 食材構成)
struct Row<'a> {
    base: &'static Pokemon,
    /// 食材構成("012" など。各桁はポケモンの食材の添字)
    key: &'a str,
    /// エナジーのパーセンタイル(0〜100)
    energy: &'a [f64],
}

/// 厳選詳細を開く個体
struct Pending {
    name: String,
    lv: u32,
    food_key: String,
}

type DetailOutput = (EnergyDetail, Arc<SubSkillCombinationList>);

pub struct EvaluateTableView {
    tab: ViewTab,
    lv: Option<u32>,
    step: u32,
    graph_percentile: i32,
    graph_axis: GraphAxis,
    sort: Option<(usize, bool)>,
    job: Option<(Pending, Job<DetailOutput>)>,
    /// サブスキルの組み合わせ(銀種の設定ごと。作るのに少しかかるので使い回す)
    combinations: Option<(String, Arc<SubSkillCombinationList>)>,
    detail: Option<EvaluateDetail>,
    error: String,
}

impl Default for EvaluateTableView {
    fn default() -> Self {
        EvaluateTableView {
            tab: ViewTab::Graph,
            lv: None,
            step: 5,
            graph_percentile: 100,
            graph_axis: GraphAxis::Type,
            sort: None,
            job: None,
            combinations: None,
            detail: None,
            error: String::new(),
        }
    }
}

impl EvaluateTableView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        // 厳選詳細の計算結果の受け取り
        if let Some((_, job)) = &mut self.job
            && let Some(r) = job.poll()
        {
            let (pending, _) = self.job.take().expect("job");
            match r {
                Ok((detail, combinations)) => {
                    let key = silver_seed_key(&shared.config);
                    self.combinations = Some((key, combinations));
                    let food_index_list = pending.food_key.chars().filter_map(|c| c.to_digit(10)).map(|d| d as usize).collect();
                    self.detail = Some(EvaluateDetail::new(&pending.name, pending.lv, food_index_list, detail.sub_skill_list, detail.nature).closed_percentile());
                }
                Err(e) => self.error = e,
            }
        }

        let Some(table) = shared.usable_evaluate_table() else {
            style::danger(ui, "厳選テーブルが使えません。「基準生成」でテーブルを作成してください。");
            return;
        };

        let lv_list = shared.config.select_evaluate.enabled_levels();
        let lv = self.lv.filter(|l| lv_list.contains(l)).or_else(|| lv_list.last().copied());
        self.lv = lv;

        let labels = vec!["グラフ".to_string(), "一覧表".to_string()];
        if let Some(i) = style::tab_list(ui, &labels, if self.tab == ViewTab::Graph { 0 } else { 1 }) {
            self.tab = if i == 0 { ViewTab::Graph } else { ViewTab::Table };
        }
        ui.add_space(6.0);

        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Lv").color(style::th_color()).strong());
            let mut text = lv.map_or(String::new(), |l| l.to_string());
            if combo(ui, "et_lv", &mut text, lv_list.iter().map(|l| l.to_string()), 70.0) {
                self.lv = text.parse().ok();
            }
            ui.add_space(12.0);
            if self.tab == ViewTab::Table {
                ui.label(egui::RichText::new("ステップ").color(style::th_color()).strong());
                let mut text = self.step.to_string();
                if combo(ui, "et_step", &mut text, ["1", "2", "5", "10"].map(String::from), 70.0) {
                    self.step = text.parse().unwrap_or(5);
                    self.sort = None;
                }
            } else {
                ui.label(egui::RichText::new("横軸").color(style::th_color()).strong());
                let mut axis_text = self.graph_axis.label().to_string();
                if combo(ui, "et_axis", &mut axis_text, GraphAxis::ALL.iter().map(|(_, l)| l.to_string()), 100.0)
                    && let Some((axis, _)) = GraphAxis::ALL.iter().find(|(_, l)| *l == axis_text)
                {
                    self.graph_axis = *axis;
                }
                ui.add_space(12.0);
                ui.label(egui::RichText::new("表示する厳選度").color(style::th_color()).strong());
                ui.add(egui::DragValue::new(&mut self.graph_percentile).range(0..=100).speed(0.2));
                ui.label("%");
            }
        });
        ui.add_space(6.0);

        if self.job.is_some()
            && let Some((_, job)) = &self.job
        {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }

        let rows = rows(table, lv);
        if rows.is_empty() {
            ui.label("このレベルの厳選テーブルがありません。");
        } else {
            match self.tab {
                ViewTab::Graph => graph(ui, &rows, self.graph_percentile.clamp(0, 100) as usize, self.graph_axis),
                ViewTab::Table => {
                    if let Some(p) = self.table(ui, &rows) {
                        self.open_detail(ui.ctx(), shared, p.0, p.1);
                    }
                }
            }
        }

        // 厳選詳細(開いている間は閉じるまで表示)
        if let Some(d) = &mut self.detail
            && !d.ui(ui.ctx(), &shared.config, shared.usable_evaluate_table())
        {
            self.detail = None;
        }
    }

    /// 厳選詳細の計算を始める(本家 showDetail)。`row` は一覧の行の添字
    fn open_detail(&mut self, ctx: &egui::Context, shared: &Shared, name_key: (String, String), percentile: usize) {
        let Some(lv) = self.lv else { return };
        let Some(table) = shared.usable_evaluate_table() else { return };
        if self.job.is_some() {
            return;
        }
        self.error.clear();
        let config = shared.config.clone();
        let small = EvaluateTable { healer: table.healer.clone(), support: table.support.clone(), ..Default::default() };
        let key = silver_seed_key(&config);
        let cached = self.combinations.as_ref().filter(|(k, _)| *k == key).map(|(_, c)| c.clone());
        let (name, food_key) = name_key;
        let pending = Pending { name: name.clone(), lv, food_key: food_key.clone() };
        let job = Job::spawn(ctx, move |progress| {
            progress.set(0.0, "厳選詳細を計算しています…");
            let combinations = match cached {
                Some(c) => c,
                None => Arc::new(sub_skill_combination_list(&config.select_evaluate.silver_seed)),
            };
            let detail = evaluate_energy_detail(&config, &small, &combinations, &name, lv, &food_key, percentile)?;
            Ok((detail, combinations))
        });
        self.job = Some((pending, job));
    }

    /// 一覧表。厳選度のセルが押されたら (ポケモン名, 食材構成, 厳選度) を返す
    fn table(&mut self, ui: &mut egui::Ui, rows: &[Row]) -> Option<((String, String), usize)> {
        let lv = self.lv?;
        let m = master();
        let step = self.step.max(1) as usize;
        let percentiles: Vec<usize> = (0..=100 / step).map(|i| i * step).collect();
        let mut headers: Vec<(String, f32)> = vec![
            ("名前".into(), NAME_W),
            ("タイプ".into(), TYPE_W),
            ("とくい".into(), SPECIALTY_W),
            ("スキル".into(), SKILL_W),
            ("食材".into(), FOOD_W),
        ];
        headers.extend(percentiles.iter().map(|p| (format!("{p}%"), PERCENTILE_W)));
        let total_width: f32 = headers.iter().map(|h| h.1).sum();

        // 並べ替え(値が無いものは常に末尾)
        let mut order: Vec<usize> = (0..rows.len()).collect();
        if let Some((col, desc)) = self.sort {
            let text = |r: &Row| -> String {
                match col {
                    0 => r.base.name.clone(),
                    1 => r.base.type_name.clone(),
                    2 => r.base.specialty.name().to_string(),
                    _ => m.pokemon_skill(r.base).name.clone(),
                }
            };
            order.sort_by(|&a, &b| {
                use std::cmp::Ordering::*;
                let o = if col < 4 {
                    text(&rows[a]).cmp(&text(&rows[b]))
                } else {
                    let p = percentiles[col - 5];
                    let (x, y) = (rows[a].energy.get(p).copied(), rows[b].energy.get(p).copied());
                    match (x, y) {
                        (Some(x), Some(y)) => {
                            let o = x.partial_cmp(&y).unwrap_or(Equal);
                            return if desc { o.reverse() } else { o };
                        }
                        (None, None) => Equal,
                        (None, _) => return Greater,
                        (_, None) => return Less,
                    }
                };
                if desc { o.reverse() } else { o }
            });
        }

        let mut clicked = None;
        let height = (ui.available_height() - 4.0).max(200.0);
        egui::ScrollArea::horizontal().id_salt("et_scroll_h").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(total_width);
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            // 見出し
            let (rect, _) = ui.allocate_exact_size(egui::vec2(total_width, 34.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 0.0, style::primary_soft());
            let mut x = rect.left();
            for (ci, (text, w)) in headers.iter().enumerate() {
                let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(*w, 34.0));
                x += w;
                let mark = match self.sort {
                    Some((s, desc)) if s == ci => if desc { " ▼" } else { " ▲" },
                    _ => "",
                };
                let align = if ci >= 5 { egui::Align2::RIGHT_CENTER } else { egui::Align2::LEFT_CENTER };
                let pos = if ci >= 5 { cell.right_center() - egui::vec2(6.0, 0.0) } else { cell.left_center() + egui::vec2(6.0, 0.0) };
                ui.painter().text(pos, align, format!("{text}{mark}"), egui::FontId::proportional(13.0), style::th_color());
                if ci != 4 {
                    let r = ui.interact(cell, ui.id().with(("et_head", ci)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if r.clicked() {
                        self.sort = match self.sort {
                            Some((s, false)) if s == ci => Some((ci, true)),
                            Some((s, true)) if s == ci => None,
                            _ => Some((ci, false)),
                        };
                    }
                }
            }

            egui::ScrollArea::vertical().id_salt("et_scroll_v").max_height(height - 40.0).auto_shrink([false, false]).show_rows(ui, ROW_HEIGHT, order.len(), |ui, range| {
                ui.set_width(total_width);
                for ri in range {
                    let row = &rows[order[ri]];
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_width, ROW_HEIGHT), egui::Sense::hover());
                    if ri % 2 == 1 {
                        ui.painter().rect_filled(rect, 0.0, style::pick(0xf3f5fa, 0x252b38));
                    }
                    ui.painter().hline(rect.x_range(), rect.bottom(), egui::Stroke::new(1.0, style::line_color()));
                    let mut x = rect.left();
                    let skill = m.pokemon_skill(row.base).name.as_str();
                    for (ci, text) in [(0, row.base.name.as_str()), (1, row.base.type_name.as_str()), (2, row.base.specialty.name()), (3, skill)] {
                        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(headers[ci].1, ROW_HEIGHT));
                        ui.painter().text(cell.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(13.0), style::ink());
                        x += headers[ci].1;
                    }
                    // 食材
                    let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 2.0), egui::vec2(FOOD_W, ROW_HEIGHT - 4.0));
                    x += FOOD_W;
                    let food_list: Vec<String> =
                        row.key.chars().filter_map(|c| c.to_digit(10)).filter_map(|d| row.base.food_name_list.get(d as usize).cloned()).collect();
                    let shown = BoxPokemon { name: row.base.name.clone(), lv: lv as f64, food_list, ..Default::default() };
                    ui.scope_builder(egui::UiBuilder::new().max_rect(cell).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                        ui.add_space(4.0);
                        food_icon::food_list(ui, &shown);
                    });
                    // 厳選度(押すと詳細)
                    for &p in &percentiles {
                        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(PERCENTILE_W, ROW_HEIGHT));
                        x += PERCENTILE_W;
                        let Some(v) = row.energy.get(p) else { continue };
                        let r = ui.interact(cell, ui.id().with(("et_cell", ri, p)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        let link = style::pick(0x0044cc, 0x7fb0ff);
                        if r.hovered() {
                            ui.painter().rect_filled(cell, 0.0, style::pick(0xddeeff, 0x2f3d57));
                        }
                        let text = fmt_num(*v);
                        let font = egui::FontId::proportional(13.0);
                        let rect = ui.painter().text(cell.right_center() - egui::vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, text, font, link);
                        ui.painter().hline(rect.x_range(), rect.bottom() + 1.0, egui::Stroke::new(1.0, link));
                        if r.clicked() {
                            clicked = Some(((row.base.name.clone(), row.key.to_string()), p));
                        }
                    }
                }
            });
        });
        clicked
    }
}

fn silver_seed_key(config: &pokesle_core::config::Config) -> String {
    let sorted: BTreeMap<&String, &bool> = config.select_evaluate.silver_seed.iter().collect();
    format!("{sorted:?}")
}

/// 指定レベルの一覧の行(ポケモンはマスターの並び、食材構成は添字順)
fn rows<'a>(table: &'a EvaluateTable, lv: Option<u32>) -> Vec<Row<'a>> {
    let Some(lv) = lv else { return Vec::new() };
    let m = master();
    let mut names: Vec<&String> = table.pokemon.keys().collect();
    names.sort_by_key(|n| m.pokemon(n).map_or(usize::MAX, |p| p.index));
    let mut out = Vec::new();
    for name in names {
        let Some(base) = m.pokemon(name) else { continue };
        let Some(by_food) = table.pokemon[name].get(&lv) else { continue };
        let mut keys: Vec<&String> = by_food.keys().filter(|k| !k.is_empty()).collect();
        keys.sort();
        for key in keys {
            out.push(Row { base, key, energy: &by_food[key].energy });
        }
    }
    out
}

/// "#rrggbb" を色に
fn parse_color(s: &str) -> Option<egui::Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// グラフの点
struct Point {
    x: f64,
    y: f64,
    name: String,
    type_name: String,
    specialty: &'static str,
    color: egui::Color32,
}

/// 食材構成が複数あるポケモンは、指定厳選度で最も高い値を代表値とする。
/// 横軸の設定に応じてタイプ別・とくい別・スコア順に並べる。
fn graph_points(rows: &[Row], percentile: usize, axis: GraphAxis) -> Vec<Point> {
    let m = master();
    let mut best: Vec<(&'static Pokemon, f64)> = Vec::new();
    for r in rows {
        let Some(&v) = r.energy.get(percentile) else { continue };
        match best.iter_mut().find(|(b, _)| b.name == r.base.name) {
            Some(cur) => {
                if v > cur.1 {
                    cur.1 = v;
                }
            }
            None => best.push((r.base, v)),
        }
    }
    best.retain(|(base, _)| SPECIALTY_LIST.contains(&base.specialty.name()));
    let type_order = |ty: &str| m.berry_type_order.iter().position(|t| t == ty).unwrap_or(usize::MAX);
    let specialty_order = |sp: &str| SPECIALTY_LIST.iter().position(|s| *s == sp).unwrap_or(usize::MAX);
    best.sort_by(|(a, va), (b, vb)| {
        let primary = match axis {
            GraphAxis::Type => type_order(&a.type_name).cmp(&type_order(&b.type_name)),
            GraphAxis::Specialty => specialty_order(a.specialty.name()).cmp(&specialty_order(b.specialty.name())),
            GraphAxis::All => std::cmp::Ordering::Equal,
        };
        primary
            .then_with(|| va.partial_cmp(vb).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.name.cmp(&b.name))
    });
    best.into_iter()
        .enumerate()
        .map(|(i, (base, v))| Point {
            x: i as f64,
            y: v,
            name: base.name.clone(),
            type_name: base.type_name.clone(),
            specialty: base.specialty.name(),
            color: parse_color(&m.berries[base.berry].type_color).unwrap_or(egui::Color32::GRAY),
        })
        .collect()
}

fn graph(ui: &mut egui::Ui, rows: &[Row], percentile: usize, axis: GraphAxis) {
    let points = graph_points(rows, percentile, axis);
    let median = {
        let mut v: Vec<f64> = points.iter().map(|p| p.y).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        match v.len() {
            0 => None,
            n if n % 2 == 0 => Some((v[n / 2 - 1] + v[n / 2]) / 2.0),
            n => Some(v[n / 2]),
        }
    };

    ui.label(egui::RichText::new(format!("{percentile}%の値(グラフは横にスクロールできます)")).size(11.0).color(style::muted()));
    let height = ui.available_height().max(GRAPH_MIN_HEIGHT);
    egui::ScrollArea::horizontal().id_salt("et_graph_scroll").auto_shrink([false, false]).show(ui, |ui| {
        let width = ui.available_width().max(GRAPH_MIN_WIDTH);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let font = egui::FontId::proportional(11.0);
        let text_color = style::muted();
        let plot = egui::Rect::from_min_max(rect.min + egui::vec2(64.0, 28.0), rect.max - egui::vec2(10.0, 32.0));

        let max = points.iter().map(|p| p.y).fold(0.0, f64::max).max(1.0);
        let (_, y_max, step) = nice_range(0.0, max);
        let n = (points.len() as f64).max(1.0);
        let to_screen = |x: f64, y: f64| {
            egui::pos2(plot.left() + ((x + 0.5) / n) as f32 * plot.width(), plot.bottom() - (y / y_max) as f32 * plot.height())
        };

        // 目盛り・格子
        let grid = egui::Stroke::new(1.0, style::line_color());
        let mut y = 0.0;
        while y <= y_max + step * 0.01 {
            let pos = to_screen(0.0, y);
            painter.hline(plot.x_range(), pos.y, grid);
            painter.text(egui::pos2(plot.left() - 6.0, pos.y), egui::Align2::RIGHT_CENTER, fmt_tick(y, step), font.clone(), text_color);
            y += step;
        }
        painter.text(rect.left_top() + egui::vec2(4.0, 4.0), egui::Align2::LEFT_TOP, "期待値", font.clone(), text_color);
        for (i, p) in points.iter().enumerate() {
            let prev = i.checked_sub(1).and_then(|j| points.get(j));
            let label = match axis {
                GraphAxis::Type if prev.is_none_or(|pr| pr.type_name != p.type_name) => Some(p.type_name.as_str()),
                GraphAxis::Specialty if prev.is_none_or(|pr| pr.specialty != p.specialty) => Some(p.specialty),
                _ => None,
            };
            if let Some(label) = label {
                let x = to_screen(i as f64, 0.0).x;
                painter.vline(x, plot.y_range(), grid);
                painter.text(egui::pos2(x, plot.bottom() + 6.0), egui::Align2::CENTER_TOP, label, egui::FontId::proportional(11.0), style::ink());
            }
        }

        // 中央値の補助線
        let median_y = median.map(|v| to_screen(0.0, v).y);
        if let Some(y) = median_y {
            painter.hline(plot.x_range(), y, egui::Stroke::new(1.0, egui::Color32::from_rgb(0xee, 0, 0)));
        }

        // 点と名前(他のラベルと重ならない候補位置へ順に配置する)
        let halo = style::surface().gamma_multiply(0.9);
        let name_font = egui::FontId::proportional(11.0);
        let border = egui::Stroke::new(1.0, style::pick(0x555555, 0xcccccc));
        let screen: Vec<egui::Pos2> = points.iter().map(|p| to_screen(p.x, p.y)).collect();
        let offsets = [6.0_f32, 19.0, 32.0, 45.0, 58.0, 71.0, 84.0, 97.0];
        let mut occupied: Vec<egui::Rect> = Vec::new();
        for (p, pos) in points.iter().zip(&screen) {
            painter.circle(*pos, 4.0, p.color, border);
            let galley = painter.layout_no_wrap(p.name.clone(), name_font.clone(), style::ink());
            let w = galley.size().x;
            let h = 13.0_f32;
            let pad = 2.0_f32;
            let rect_for = |y: f32, above: bool| {
                let top = if above { y - h } else { y };
                egui::Rect::from_min_max(egui::pos2(pos.x - w / 2.0 - pad, top - pad), egui::pos2(pos.x + w / 2.0 + pad, top + h + pad))
            };
            let mut chosen = (pos.y - offsets[0], true);
            'find: for d in offsets {
                for (y, above) in [(pos.y - d, true), (pos.y + d, false)] {
                    let r = rect_for(y, above);
                    if r.left() >= plot.left()
                        && r.right() <= plot.right()
                        && r.top() >= rect.top()
                        && r.bottom() <= rect.bottom()
                        && !occupied.iter().any(|o| r.intersects(*o))
                    {
                        chosen = (y, above);
                        break 'find;
                    }
                }
            }
            occupied.push(rect_for(chosen.0, chosen.1));
            let align = if chosen.1 { egui::Align2::CENTER_BOTTOM } else { egui::Align2::CENTER_TOP };
            let at = egui::pos2(pos.x, chosen.0);
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                painter.text(at + egui::vec2(dx, dy), align, &p.name, name_font.clone(), halo);
            }
            painter.text(at, align, &p.name, name_font.clone(), style::ink());
        }

        // ホバー(点 → 中央値の順)
        if let Some(pointer) = response.hover_pos() {
            let near = screen.iter().enumerate().map(|(i, s)| (i, (*s - pointer).length())).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1));
            let text = if let Some((i, _)) = near {
                let p = &points[i];
                Some(format!("{}\n{}・{}: {}", p.name, p.type_name, p.specialty, fmt_num(p.y)))
            } else if plot.contains(pointer) && median_y.is_some_and(|y| (pointer.y - y).abs() <= 5.0) {
                Some("中央値".to_string())
            } else {
                None
            };
            if let Some(text) = text {
                response.on_hover_text_at_pointer(text);
            }
        }
    });
}
