//! データ画面(本家 `pages/data.vue` と `pages/data/*.vue`)

use std::collections::HashMap;

use eframe::egui;
use pokesle_core::box_sim::BoxResult;
use pokesle_core::data::{master, CookingType};
use pokesle_core::data_view::{expect_simulate, food_rows, pokemon_derived, ExpectKind, ExpectParam};
use pokesle_core::sim_pokemon::BoxPokemon;

use super::{common_setting, fmt_fixed, food_icon, style};
use crate::app::Shared;
use crate::job::Job;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DataTab {
    Pokemon,
    PokemonFood,
    PokemonSkill,
    Food,
    Cooking,
}

const TABS: [(DataTab, &str); 5] = [
    (DataTab::Pokemon, "ポケモン"),
    (DataTab::PokemonFood, "ポケモン(食材期待値)"),
    (DataTab::PokemonSkill, "ポケモン(スキル期待値)"),
    (DataTab::Food, "食材"),
    (DataTab::Cooking, "料理"),
];

enum SortKey {
    None,
    Num(f64),
    Str(String),
}

fn num_key(v: f64) -> SortKey {
    if v.is_nan() { SortKey::None } else { SortKey::Num(v) }
}

/// 見出し(食材の列はアイコンで表示する)
struct Header {
    text: String,
    width: f32,
    food: Option<String>,
}

fn h(text: &str, width: f32) -> Header {
    Header { text: text.to_string(), width, food: None }
}

/// 見出しクリックで並べ替えできる表。`keys` は行×列の並べ替え値、`cell` が各セルを描く
fn sortable_table(
    ui: &mut egui::Ui,
    id: &str,
    sort: &mut Option<(usize, bool)>,
    headers: &[Header],
    keys: &[Vec<SortKey>],
    mut cell: impl FnMut(&mut egui::Ui, usize, usize),
) {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    if let Some((col, desc)) = *sort {
        order.sort_by(|&a, &b| {
            use std::cmp::Ordering::*;
            let o = match (&keys[a][col], &keys[b][col]) {
                (SortKey::Num(x), SortKey::Num(y)) => x.partial_cmp(y).unwrap_or(Equal),
                (SortKey::Str(x), SortKey::Str(y)) => x.cmp(y),
                (SortKey::None, SortKey::None) => return Equal,
                (SortKey::None, _) => return Greater,
                (_, SortKey::None) => return Less,
                _ => Equal,
            };
            if desc { o.reverse() } else { o }
        });
    }
    egui::ScrollArea::both().id_salt(("data_scroll", id)).auto_shrink(false).show(ui, |ui| {
        egui::Grid::new(("data_grid", id)).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
            for (ci, header) in headers.iter().enumerate() {
                let mark = match *sort {
                    Some((s, desc)) if s == ci => if desc { " ▼" } else { " ▲" },
                    _ => "",
                };
                let r = match &header.food {
                    Some(name) => ui
                        .horizontal(|ui| {
                            ui.set_min_width(header.width);
                            food_icon::food_icon(ui, name, 22.0);
                            ui.label(mark);
                        })
                        .response
                        .interact(egui::Sense::click()),
                    None => {
                        let text = egui::RichText::new(format!("{}{mark}", header.text)).strong().color(style::th_color());
                        ui.add_sized([header.width, 34.0], egui::Label::new(text).sense(egui::Sense::click()))
                    }
                };
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
                for ci in 0..headers.len() {
                    cell(ui, ri, ci);
                }
                ui.end_row();
            }
        });
    });
}

/// 右寄せの数値(小数桁を固定)
fn number(ui: &mut egui::Ui, v: Option<f64>, digits: usize) {
    if let Some(v) = v.filter(|v| !v.is_nan()) {
        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| ui.label(fmt_fixed(v, digits)));
    }
}

fn percent(ui: &mut egui::Ui, v: Option<f64>, digits: usize) {
    if let Some(v) = v.filter(|v| !v.is_nan()) {
        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| ui.label(format!("{}%", fmt_fixed(v * 100.0, digits))));
    }
}

fn opt_key(v: Option<f64>) -> SortKey {
    v.map_or(SortKey::None, num_key)
}

pub struct DataPageView {
    tab: DataTab,
    sort: [Option<(usize, bool)>; 5],
    last_only: bool,
    /// 食材期待値・スキル期待値の条件と結果
    expect: [ExpectParam; 2],
    results: [Option<Vec<BoxResult>>; 2],
    jobs: [Option<Job<Vec<BoxResult>>>; 2],
    errors: [String; 2],
    started: [bool; 2],
    /// 料理の作成数(保存する)
    cooking_num: HashMap<String, f64>,
    cooking_num_loaded: bool,
    require_mode: u8,
}

impl Default for DataPageView {
    fn default() -> Self {
        DataPageView {
            tab: DataTab::Pokemon,
            sort: [None; 5],
            last_only: false,
            expect: [ExpectParam::default_for(ExpectKind::Food), ExpectParam::default_for(ExpectKind::Skill)],
            results: [None, None],
            jobs: [None, None],
            errors: [String::new(), String::new()],
            started: [false; 2],
            cooking_num: HashMap::new(),
            cooking_num_loaded: false,
            require_mode: 0,
        }
    }
}

impl DataPageView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        if !self.cooking_num_loaded {
            self.cooking_num = crate::storage::load_data_cooking_num();
            self.cooking_num_loaded = true;
        }

        // 見た目の確認用(デバッグ版のみ): 最初のフレームだけタブを指定する
        #[cfg(debug_assertions)]
        if !self.started[0] && !self.started[1] {
            self.tab = match std::env::var("POKESLE_DEBUG_DATA").as_deref() {
                Ok("pokemon-food") => DataTab::PokemonFood,
                Ok("pokemon-skill") => DataTab::PokemonSkill,
                Ok("food") => DataTab::Food,
                Ok("cooking") => DataTab::Cooking,
                _ => self.tab,
            };
        }

        let labels: Vec<String> = TABS.iter().map(|(_, l)| l.to_string()).collect();
        let active = TABS.iter().position(|(t, _)| *t == self.tab).unwrap_or(0);
        if let Some(i) = style::tab_list(ui, &labels, active) {
            self.tab = TABS[i].0;
        }
        ui.add_space(6.0);

        match self.tab {
            DataTab::Pokemon => self.pokemon_tab(ui),
            DataTab::PokemonFood => self.expect_tab(ui, shared, 0),
            DataTab::PokemonSkill => self.expect_tab(ui, shared, 1),
            DataTab::Food => self.food_tab(ui, shared),
            DataTab::Cooking => self.cooking_tab(ui),
        }
    }

    // ---- ポケモン ----

    fn pokemon_tab(&mut self, ui: &mut egui::Ui) {
        let m = master();
        ui.checkbox(&mut self.last_only, "最終進化のみ");
        let list: Vec<_> = m.pokemon.iter().filter(|p| !self.last_only || (p.after_list.len() == 1 && p.after_list[0] == p.name)).collect();
        let headers = [
            h("名前", 150.0), h("とくい", 50.0), h("きのみ", 90.0), h("タイプ", 60.0),
            h("A", 30.0), h("B", 30.0), h("C", 30.0),
            h("A1", 36.0), h("A2", 36.0), h("A3", 36.0), h("B2", 36.0), h("B3", 36.0), h("C3", 36.0),
            h("食材確率", 60.0), h("スキル", 130.0), h("スキル\n確率", 60.0), h("スキル\n天井", 50.0), h("スキル確率\n(天井込)", 70.0),
            h("所持数", 50.0), h("おてつだい\n時間", 70.0), h("スキル/日\n(げんき0)", 70.0), h("最終進化", 200.0),
        ];
        let num_at = |p: &pokesle_core::data::Pokemon, f: usize, s: usize| p.food_list.get(f).and_then(|x| x.num_list.get(s).copied().flatten());
        let food_at = |p: &pokesle_core::data::Pokemon, f: usize| p.food_list.get(f).map(|x| x.name.clone());
        let keys: Vec<Vec<SortKey>> = list
            .iter()
            .map(|p| {
                let d = pokemon_derived(p);
                vec![
                    SortKey::Str(p.name.clone()), SortKey::Str(p.specialty.name().to_string()), SortKey::Str(m.berries[p.berry].name.clone()), SortKey::Str(p.type_name.clone()),
                    food_at(p, 0).map_or(SortKey::None, SortKey::Str), food_at(p, 1).map_or(SortKey::None, SortKey::Str), food_at(p, 2).map_or(SortKey::None, SortKey::Str),
                    opt_key(num_at(p, 0, 0)), opt_key(num_at(p, 0, 1)), opt_key(num_at(p, 0, 2)), opt_key(num_at(p, 1, 1)), opt_key(num_at(p, 1, 2)), opt_key(num_at(p, 2, 2)),
                    num_key(p.food_rate), SortKey::Str(m.skills[p.skill].name.clone()), num_key(p.skill_rate), num_key(d.ceil), num_key(d.skill_rate_with_ceil),
                    num_key(p.bag), num_key(p.help), num_key(d.skill_num), SortKey::Str(p.after_list.join("/")),
                ]
            })
            .collect();
        sortable_table(ui, "pokemon", &mut self.sort[0], &headers, &keys, |ui, r, c| {
            let p = list[r];
            let d = pokemon_derived(p);
            match c {
                0 => { ui.label(&p.name); }
                1 => { ui.label(p.specialty.name()); }
                2 => { ui.label(&m.berries[p.berry].name); }
                3 => { ui.label(&p.type_name); }
                4..=6 => {
                    if let Some(name) = food_at(p, c - 4) {
                        food_icon::food_icon(ui, &name, 22.0);
                    }
                }
                7 => number(ui, num_at(p, 0, 0), 0),
                8 => number(ui, num_at(p, 0, 1), 0),
                9 => number(ui, num_at(p, 0, 2), 0),
                10 => number(ui, num_at(p, 1, 1), 0),
                11 => number(ui, num_at(p, 1, 2), 0),
                12 => number(ui, num_at(p, 2, 2), 0),
                13 => percent(ui, Some(p.food_rate), 1),
                14 => { ui.label(&m.skills[p.skill].name); }
                15 => percent(ui, Some(p.skill_rate), 2),
                16 => number(ui, Some(d.ceil), 0),
                17 => percent(ui, Some(d.skill_rate_with_ceil), 2),
                18 => number(ui, Some(p.bag), 0),
                19 => number(ui, Some(p.help), 0),
                20 => number(ui, Some(d.skill_num), 2),
                _ => { ui.label(p.after_list.join("/")); }
            }
        });
    }

    // ---- 食材期待値・スキル期待値 ----

    fn expect_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared, which: usize) {
        let m = master();
        let mut changed = false;
        let skill = which == 1;
        let (sub_s, sub_m) = if skill { ("スキル確率アップS", "スキル確率アップM") } else { ("食材確率アップS", "食材確率アップM") };
        egui::Frame::new().fill(style::surface()).stroke(egui::Stroke::new(1.0, style::line_color())).corner_radius(6).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let p = &mut self.expect[which];
            egui::Grid::new(("expect_param", which)).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                style::th(ui, "Lv");
                changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut p.lv).range(1.0..=100.0)).changed();
                ui.end_row();
                style::th(ui, "せいかく");
                let label = |n: &pokesle_core::data::Nature| match (&n.good, &n.weak) {
                    (Some(g), Some(w)) => format!("{}({g}↑ / {w}↓)", n.name),
                    _ => n.name.clone(),
                };
                let current = m.nature(&p.nature).map(label).unwrap_or_else(|| p.nature.clone());
                egui::ComboBox::from_id_salt(("expect_nature", which)).selected_text(current).height(400.0).show_ui(ui, |ui| {
                    for n in &m.natures {
                        changed |= ui.selectable_value(&mut p.nature, n.name.clone(), label(n)).changed();
                    }
                });
                ui.end_row();
                style::th(ui, "サブスキル");
                let subs = [(0u8, "-".to_string()), (1, sub_s.to_string()), (2, sub_m.to_string()), (3, format!("{sub_s}&{sub_m}"))];
                egui::ComboBox::from_id_salt(("expect_sub", which)).selected_text(subs[p.sub_skill as usize].1.clone()).show_ui(ui, |ui| {
                    for (v, l) in &subs {
                        changed |= ui.selectable_value(&mut p.sub_skill, *v, l).changed();
                    }
                });
                ui.end_row();
                style::th(ui, "おてスピ短縮");
                ui.horizontal(|ui| {
                    changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut p.speed).range(0.0..=35.0)).changed();
                    ui.label("%");
                });
                ui.end_row();
                style::th(ui, "げんき");
                changed |= ui.checkbox(&mut p.genki_full, "常時80%以上").changed();
                ui.end_row();
            });
        });
        ui.add_space(6.0);

        // 結果の受け取りと計算開始(本家は表示時と条件変更時に計算する)
        if let Some(job) = &mut self.jobs[which]
            && let Some(r) = job.poll()
        {
            self.jobs[which] = None;
            match r {
                Ok(r) => {
                    self.results[which] = Some(r);
                    self.errors[which].clear();
                }
                Err(e) => self.errors[which] = e,
            }
        }
        if (changed || !self.started[which]) && self.jobs[which].is_none() {
            self.started[which] = true;
            let config = shared.config.clone();
            let param = self.expect[which].clone();
            self.jobs[which] = Some(Job::spawn(ui.ctx(), move |p| {
                p.set(0.0, "計算しています…");
                expect_simulate(&config, &param)
            }));
        } else if changed {
            // 計算中の変更は終わってから反映する
            self.started[which] = false;
        }
        if let Some(job) = &self.jobs[which] {
            job.show_progress(ui);
        }
        if !self.errors[which].is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.errors[which]);
        }

        let Some(list) = &self.results[which] else { return };
        let lv = self.expect[which].lv;
        let mut headers = vec![h("名前", 150.0), h("食材", 90.0)];
        if skill {
            headers.extend([h("スキル", 130.0), h("スキル確率", 70.0), h("スキル確率\n(天井補正)", 80.0), h("天井", 50.0), h("天井到達\n確率", 70.0), h("スキル回数/日", 90.0)]);
        } else {
            for f in &m.foods {
                headers.push(Header { text: f.name.clone(), width: 48.0, food: Some(f.name.clone()) });
            }
        }
        let keys: Vec<Vec<SortKey>> = list
            .iter()
            .map(|r| {
                let p = &r.pokemon;
                let mut k = vec![SortKey::Str(p.base().name.clone()), SortKey::None];
                if skill {
                    k.extend([
                        SortKey::Str(m.skills[p.base().skill].name.clone()),
                        num_key(p.skill_rate),
                        num_key(p.ceil_skill_rate),
                        num_key(p.skill_ceil),
                        num_key((1.0 - p.skill_rate).powf(p.skill_ceil)),
                        num_key(p.skill_per_day),
                    ]);
                } else {
                    k.extend(p.food_count.iter().map(|&v| num_key(v)));
                }
                k
            })
            .collect();
        sortable_table(ui, if skill { "pokemon-skill" } else { "pokemon-food" }, &mut self.sort[which + 1], &headers, &keys, |ui, row, col| {
            let p = &list[row].pokemon;
            match col {
                0 => { ui.label(&p.base().name); }
                1 => {
                    if let Some(bx) = &p.box_pokemon {
                        let shown = BoxPokemon { lv, ..bx.clone() };
                        ui.horizontal(|ui| food_icon::food_list(ui, &shown));
                    }
                }
                _ if skill => match col {
                    2 => { ui.label(&m.skills[p.base().skill].name); }
                    3 => percent(ui, Some(p.skill_rate), 1),
                    4 => percent(ui, Some(p.ceil_skill_rate), 1),
                    5 => number(ui, Some(p.skill_ceil), 1),
                    6 => percent(ui, Some((1.0 - p.skill_rate).powf(p.skill_ceil)), 1),
                    _ => number(ui, Some(p.skill_per_day), 2),
                },
                _ => number(ui, p.food_count.get(col - 2).copied(), 1),
            }
        });
    }

    // ---- 食材 ----

    fn food_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let m = master();
        style::alert(
            ui,
            "「最大補正」や「最大料理エナジー」でソートすることで、隙間を埋める食材の判断に使うことができます。\nどちらもほぼ同じ意味ですが、めいそうスイートサラダのように補正は高いがエナジーが低い料理をどう扱うかによって使い分けてください。",
        );
        ui.add_space(6.0);
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
            for (v, l) in [(0.0, "通常"), (1.0, "料理育成(カンスト除外のみ)"), (2.0, "料理育成(料理以外無視)")] {
                changed |= ui.radio_value(&mut shared.config.simulation.mode, v, l).changed();
            }
            ui.add_space(8.0);
            changed |= common_setting::cooking_setting(ui, &mut shared.config);
        });
        if changed {
            shared.save_config();
        }

        // 列の表示(本家 config.sortableTable.food.columnVisibility)
        let names = [("require", "必要最大数"), ("rate", "最大補正"), ("cookingEnergy", "最大料理エナジー"), ("energy", "最大単品エナジー")];
        let mut vis = [true; 4];
        {
            let table = shared.config.extra.entry("sortableTable").or_insert_with(|| serde_json::json!({}));
            let cv = &table["food"]["columnVisibility"];
            for (i, (k, _)) in names.iter().enumerate() {
                vis[i] = cv.get(*k).and_then(|v| v.as_bool()).unwrap_or(true);
            }
        }
        let mut vis_changed = false;
        ui.horizontal(|ui| {
            for (i, (_, label)) in names.iter().enumerate() {
                vis_changed |= ui.checkbox(&mut vis[i], *label).changed();
            }
        });
        if vis_changed {
            let table = shared.config.extra.entry("sortableTable").or_insert_with(|| serde_json::json!({}));
            if !table["food"]["columnVisibility"].is_object() {
                table["food"] = serde_json::json!({ "columnVisibility": {} });
            }
            for (i, (k, _)) in names.iter().enumerate() {
                table["food"]["columnVisibility"][*k] = serde_json::json!(vis[i]);
            }
            shared.save_config();
        }
        ui.add_space(6.0);

        let rows = food_rows(&shared.config);
        // 列の定義: (見出し, 幅, 種類インデックス, 項目)
        #[derive(Clone, Copy)]
        enum Col {
            Name,
            Energy,
            Require(usize),
            Rate(usize),
            CookingEnergy(usize),
            MaxEnergy(usize),
            Average,
            GeoMean,
            MaxAll,
        }
        let mut cols: Vec<(String, f32, Col)> = vec![("名前".into(), 140.0, Col::Name), ("基礎\nエナジー".into(), 60.0, Col::Energy)];
        for (t, ty) in CookingType::ALL.iter().enumerate() {
            let n = ty.name();
            if vis[0] {
                cols.push((format!("{n}\n必要最大数"), 80.0, Col::Require(t)));
            }
            if vis[1] {
                cols.push((format!("{n}\n最大補正"), 80.0, Col::Rate(t)));
            }
            if vis[2] {
                cols.push((format!("{n}\n最大料理エナジー"), 110.0, Col::CookingEnergy(t)));
            }
            if vis[3] {
                cols.push((format!("{n}\n最大単品エナジー\n(レシピLv込)"), 120.0, Col::MaxEnergy(t)));
            }
        }
        cols.push(("平均\n最大料理エナジー".into(), 110.0, Col::Average));
        cols.push(("相乗平均\n最大料理エナジー".into(), 120.0, Col::GeoMean));
        cols.push(("総合\n最大単品エナジー\n(レシピLv込)".into(), 130.0, Col::MaxAll));

        let value = |r: &pokesle_core::data_view::FoodRow, c: Col| -> f64 {
            match c {
                Col::Name => f64::NAN,
                Col::Energy => m.foods[r.food].energy,
                Col::Require(t) => r.require[t],
                Col::Rate(t) => r.best_type_rate[t],
                Col::CookingEnergy(t) => r.max_cooking_energy[t],
                Col::MaxEnergy(t) => r.max_energy[t],
                Col::Average => r.max_cooking_energy_average,
                Col::GeoMean => r.max_cooking_energy_geometric_mean,
                Col::MaxAll => r.max_energy_all,
            }
        };
        let headers: Vec<Header> = cols.iter().map(|(t, w, _)| h(t, *w)).collect();
        let keys: Vec<Vec<SortKey>> = rows
            .iter()
            .map(|r| {
                cols.iter()
                    .map(|(_, _, c)| match c {
                        Col::Name => SortKey::Str(m.foods[r.food].name.clone()),
                        c => num_key(value(r, *c)),
                    })
                    .collect()
            })
            .collect();
        sortable_table(ui, "food", &mut self.sort[3], &headers, &keys, |ui, ri, ci| {
            let r = &rows[ri];
            match cols[ci].2 {
                Col::Name => {
                    ui.horizontal(|ui| {
                        food_icon::food_icon(ui, &m.foods[r.food].name, 20.0);
                        ui.label(&m.foods[r.food].name);
                    });
                }
                Col::Rate(_) => percent(ui, Some(value(r, cols[ci].2)), 0),
                c => number(ui, Some(value(r, c)), 0),
            }
        });
    }

    // ---- 料理 ----

    fn cooking_tab(&mut self, ui: &mut egui::Ui) {
        let m = master();
        // 種類順(カレー→サラダ→デザート)、同じ種類の中ではエナジーの高い順
        let mut list: Vec<usize> = (0..m.cookings.len()).collect();
        list.sort_by(|&a, &b| m.cookings[b].energy.partial_cmp(&m.cookings[a].energy).unwrap_or(std::cmp::Ordering::Equal));
        list.sort_by_key(|&i| m.cookings[i].cooking_type.name());

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 270.0).max(500.0));
                let headers = [
                    h("タイプ", 60.0), h("名前", 170.0), h("レシピ", 190.0), h("ボーナス", 60.0), h("エナジー", 70.0),
                    h("エナジー(最大)", 100.0), h("食材数", 50.0), h("作成数", 60.0),
                ];
                let keys: Vec<Vec<SortKey>> = list
                    .iter()
                    .map(|&i| {
                        let c = &m.cookings[i];
                        vec![
                            SortKey::Str(c.cooking_type.name().to_string()),
                            SortKey::Str(c.name.clone()),
                            SortKey::None,
                            num_key(c.rate),
                            num_key(c.energy),
                            num_key(c.max_energy),
                            num_key(c.food_num),
                            num_key(self.cooking_num.get(&c.name).copied().unwrap_or(0.0)),
                        ]
                    })
                    .collect();
                let mut nums_changed = false;
                let nums = &mut self.cooking_num;
                sortable_table(ui, "cooking", &mut self.sort[4], &headers, &keys, |ui, r, c| {
                    let cooking = &m.cookings[list[r]];
                    match c {
                        0 => { ui.label(cooking.cooking_type.name()); }
                        1 => { ui.label(&cooking.name); }
                        2 => {
                            ui.vertical(|ui| {
                                for f in &cooking.food_list {
                                    ui.label(format!("{}×{}", f.name, f.num));
                                }
                            });
                        }
                        3 => percent(ui, Some(cooking.rate), 0),
                        4 => number(ui, Some(cooking.energy), 0),
                        5 => number(ui, Some(cooking.max_energy), 0),
                        6 => number(ui, Some(cooking.food_num), 0),
                        _ => {
                            let v = nums.entry(cooking.name.clone()).or_insert(0.0);
                            nums_changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(v).range(0.0..=9999.0)).changed();
                        }
                    }
                });
                if nums_changed {
                    let _ = crate::storage::save_data_cooking_num(&self.cooking_num);
                }
            });

            // 必要な食材
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(240.0);
                ui.label(egui::RichText::new("必要な食材").size(16.0).strong());
                ui.radio_value(&mut self.require_mode, 0, "そのまま合計");
                ui.radio_value(&mut self.require_mode, 1, "異なるタイプの重複は省く");
                let groups: Vec<Vec<usize>> = if self.require_mode == 0 {
                    vec![list.clone()]
                } else {
                    CookingType::ALL.iter().map(|t| list.iter().copied().filter(|&i| m.cookings[i].cooking_type == *t).collect()).collect()
                };
                let mut need: HashMap<&str, f64> = HashMap::new();
                for g in &groups {
                    let mut this: HashMap<&str, f64> = HashMap::new();
                    for &i in g {
                        let c = &m.cookings[i];
                        let n = self.cooking_num.get(&c.name).copied().unwrap_or(0.0);
                        if n == 0.0 {
                            continue;
                        }
                        for f in &c.food_list {
                            *this.entry(f.name.as_str()).or_insert(0.0) += f.num * n;
                        }
                    }
                    for (k, v) in this {
                        let e = need.entry(k).or_insert(0.0);
                        *e = e.max(v);
                    }
                }
                let rows: Vec<(&str, f64)> = m.foods.iter().filter_map(|f| need.get(f.name.as_str()).filter(|v| **v != 0.0).map(|v| (f.name.as_str(), *v))).collect();
                if rows.is_empty() {
                    ui.label(egui::RichText::new("作成数を入力すると必要な食材が表示されます").color(style::muted()));
                } else {
                    egui::Grid::new("require_food").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                        style::th(ui, "名前");
                        style::th(ui, "必要数");
                        ui.end_row();
                        for (name, n) in &rows {
                            ui.horizontal(|ui| {
                                food_icon::food_icon(ui, name, 20.0);
                                ui.label(*name);
                            });
                            number(ui, Some(*n), 0);
                            ui.end_row();
                        }
                        ui.label("合計");
                        number(ui, Some(rows.iter().map(|r| r.1).sum()), 0);
                        ui.end_row();
                    });
                }
            });
        });
    }
}
