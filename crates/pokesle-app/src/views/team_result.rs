//! チーム編成結果の表(本家 pages/simulation.vue の結果部分)
//!
//! egui の Grid には行・列の結合が無いので、列幅を固定した行を積み重ねて本家と同じ形の表を描く。

use eframe::egui;
use pokesle_core::config::Config;
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::SimPokemon;
use pokesle_core::team_sim::{TeamMember, TeamResult};

use super::{fmt_num, food_icon, nature_label, style};

/// 見出し列・ポケモン列・合計列の幅(本家 --result-header-column-width / .result-pokemon-column)
const HEAD_W: f32 = 100.0;
const MEMBER_W: f32 = 120.0;
const TOTAL_W: f32 = 110.0;
const PAD: f32 = 4.0;
const BORDER: egui::Color32 = egui::Color32::from_rgb(0x99, 0x99, 0x99);

/// 結果の表示設定(本家 `config.teamSimulation.result`)
#[derive(Clone, Copy, Default)]
pub struct ResultSetting {
    /// エナジー内訳
    pub detail: bool,
    /// 食材情報
    pub food: bool,
}

impl ResultSetting {
    pub fn load(c: &Config) -> ResultSetting {
        let r = c.team_simulation.extra.get("result");
        let get = |k: &str| r.and_then(|v| v.get(k)).and_then(|v| v.as_bool()).unwrap_or(false);
        ResultSetting { detail: get("detail"), food: get("food") }
    }

    pub fn save(self, c: &mut Config) {
        let r = c.team_simulation.extra.entry("result").or_insert_with(|| serde_json::json!({}));
        if let Some(m) = r.as_object_mut() {
            m.insert("detail".into(), self.detail.into());
            m.insert("food".into(), self.food.into());
        }
    }
}

/// 表のマス1つ
struct Cell<'a> {
    /// 何列ぶんか
    span: usize,
    /// 見出し(灰色の背景・太字)
    header: bool,
    right: bool,
    add: Box<dyn FnOnce(&mut egui::Ui) + 'a>,
}

fn th<'a>(span: usize, text: impl Into<String>) -> Cell<'a> {
    let text = text.into();
    Cell { span, header: true, right: false, add: Box::new(move |ui| {
        ui.label(egui::RichText::new(text).strong());
    }) }
}

fn td<'a>(span: usize, add: impl FnOnce(&mut egui::Ui) + 'a) -> Cell<'a> {
    Cell { span, header: false, right: false, add: Box::new(add) }
}

fn num<'a>(span: usize, text: impl Into<String>) -> Cell<'a> {
    let text = text.into();
    Cell { span, header: false, right: true, add: Box::new(move |ui| {
        ui.label(text);
    }) }
}

fn empty<'a>(span: usize) -> Cell<'a> {
    td(span, |_| {})
}

/// 表の描画状態(列幅と、縦の結合中の見出し)
struct Table {
    widths: Vec<f32>,
    left: f32,
}

impl Table {
    /// 1行描く。`group` は1列目の縦結合の見出し(Some(text) で開始、Some("") で継続、None で結合なし)
    fn row(&self, ui: &mut egui::Ui, group: Option<(&str, bool)>, cells: Vec<Cell<'_>>) {
        let top = ui.cursor().min.y;
        let mut x = self.left;
        let mut col = 0;
        let mut rects: Vec<(egui::Rect, bool)> = Vec::new();
        let mut bottom = top;
        let all: Vec<Cell> = match group {
            Some((text, _)) => {
                let text = text.to_string();
                std::iter::once(th(1, text)).chain(cells).collect()
            }
            None => cells,
        };
        // 中身を描いてから背景と罫線を下に敷く
        let bg = ui.painter().add(egui::Shape::Noop);
        let mut bg_shapes = Vec::new();
        for cell in all {
            let w: f32 = self.widths[col..col + cell.span].iter().sum();
            let rect = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(w, 0.0));
            let layout = if cell.right { egui::Layout::top_down(egui::Align::Max) } else { egui::Layout::top_down(egui::Align::Min) };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(PAD, 0.0)).translate(egui::vec2(0.0, PAD)).with_max_y(f32::INFINITY)).layout(layout));
            child.set_width(w - PAD * 2.0);
            if cell.header {
                // 見出しは折り返さない(本家の th は nowrap)
                child.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            }
            child.spacing_mut().item_spacing.y = 2.0;
            (cell.add)(&mut child);
            bottom = bottom.max(child.min_rect().bottom() + PAD);
            rects.push((rect, cell.header));
            x += w;
            col += cell.span;
        }
        let first_in_group = group.is_some_and(|(_, start)| start);
        let painter = ui.painter();
        for (i, (rect, header)) in rects.iter().enumerate() {
            let r = egui::Rect::from_x_y_ranges(rect.x_range(), top..=bottom);
            let group_cell = i == 0 && group.is_some();
            if *header {
                bg_shapes.push(egui::Shape::rect_filled(r, 0.0, style::SURFACE_SUBTLE));
            } else {
                bg_shapes.push(egui::Shape::rect_filled(r, 0.0, egui::Color32::WHITE));
            }
            // 縦に結合した見出しは、結合の途中で上の罫線を引かない
            if !group_cell || first_in_group {
                bg_shapes.push(egui::Shape::hline(r.x_range(), r.top(), egui::Stroke::new(1.0, BORDER)));
            }
            bg_shapes.push(egui::Shape::vline(r.right(), r.y_range(), egui::Stroke::new(1.0, BORDER)));
            if i == 0 {
                bg_shapes.push(egui::Shape::vline(r.left(), r.y_range(), egui::Stroke::new(1.0, BORDER)));
            }
        }
        painter.set(bg, egui::Shape::Vec(bg_shapes));
        let total: f32 = self.widths.iter().sum();
        ui.allocate_rect(egui::Rect::from_min_max(egui::pos2(self.left, top), egui::pos2(self.left + total, bottom)), egui::Sense::hover());
    }

    /// 表の下端の罫線
    fn close(&self, ui: &mut egui::Ui) {
        let y = ui.cursor().min.y - ui.spacing().item_spacing.y;
        let total: f32 = self.widths.iter().sum();
        ui.painter().hline(self.left..=self.left + total, y, egui::Stroke::new(1.0, BORDER));
    }
}

fn round_str(x: f64) -> String {
    fmt_num(x)
}

/// 名前(本家 name-label.vue + 固定表示)
fn name_cell(ui: &mut egui::Ui, p: &SimPokemon) {
    let base = p.base();
    let bx = p.box_pokemon.as_ref();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if let Some(before) = p.before_name.as_deref().filter(|b| *b != base.name) {
            ui.label(egui::RichText::new(format!("{before}→")).size(10.0));
        }
        let mut text = egui::RichText::new(&base.name);
        if bx.is_some_and(|b| b.shiny) {
            text = text.strong().color(egui::Color32::from_rgb(0xee, 0x55, 0x22));
        }
        ui.label(text);
        if bx.is_some_and(|b| b.favorite) {
            ui.label("⭐");
        }
        if p.bag_over_operation {
            ui.label("(いつ育)");
        }
        if bx.and_then(|b| b.fix) == Some(1.0) {
            ui.label("(固定)");
        }
    });
    if let Some(memo) = bx.and_then(|b| b.memo.as_deref()).filter(|m| !m.is_empty()) {
        ui.label(egui::RichText::new(memo).size(10.0));
    }
}

/// サブスキル(本家 sub-skill-label-list.vue)
fn sub_skill_cell(ui: &mut egui::Ui, p: &SimPokemon, short: bool) {
    let m = master();
    let Some(bx) = p.box_pokemon.as_ref() else { return };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        for (i, name) in bx.sub_skill_list.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            let sub = p.sub_skill_list.get(i).copied().flatten().map(|x| &m.sub_skills[x]).or_else(|| m.sub_skill(name));
            let disabled = i >= p.sub_skill_list.len();
            let fixed = sub.is_some_and(|s| s.name != *name);
            let silver = p.next_sub_skill_list.get(i).copied().unwrap_or(false);
            sub_skill_label(ui, sub.map(|s| (s.name.as_str(), s.short.as_str(), s.rarity)), short, disabled, fixed, silver);
        }
    });
}

/// 本家 sub-skill-label.vue
fn sub_skill_label(ui: &mut egui::Ui, sub: Option<(&str, &str, u32)>, short: bool, disabled: bool, fixed: bool, silver: bool) {
    let hex = |rgb: u32| egui::Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    let (bg, border, color) = match sub.map_or(1, |s| s.2) {
        2 => (hex(0xeaf5ff), hex(0x9dc9ef), hex(0x2c5277)),
        3 => (hex(0xfff4d9), hex(0xe5bd70), hex(0x76511f)),
        _ => (hex(0xf6f7fc), hex(0xbfc8de), hex(0x3b4663)),
    };
    let color = if fixed { egui::Color32::RED } else { color };
    let alpha = if disabled { 0.5 } else { 1.0 };
    let text = sub.map_or("-", |s| if short { s.1 } else { s.0 });
    let font = egui::FontId::proportional(11.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color.gamma_multiply(alpha));
    let width = if short { 52.0_f32 } else { 112.0_f32 }.max(galley.size().x + 6.0);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, galley.size().y + 4.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect(rect, 5.0, bg.gamma_multiply(alpha), egui::Stroke::new(1.0, border.gamma_multiply(alpha)), egui::StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, color);
    if silver {
        p.circle_filled(rect.right_top() + egui::vec2(-1.0, 1.0), 3.5, egui::Color32::from_rgb(0x9a, 0xa4, 0xb4));
    }
    if let Some(s) = sub {
        r.on_hover_text(if silver { format!("{}(銀種が使える)", s.0) } else { s.0.to_string() });
    }
}

fn member_pokemon(t: &TeamMember) -> Option<&SimPokemon> {
    match t {
        TeamMember::Pokemon { pokemon, .. } => Some(pokemon.as_ref()),
        TeamMember::NightCapPikachu(_) => None,
    }
}

/// 1チーム分の表
pub fn team_table(ui: &mut egui::Ui, team: &TeamResult, config: &Config, result_config: &Config, setting: ResultSetting, sub_skill_short: bool) {
    let m = master();
    let n = team.pokemon_list.len();
    // 料理
    let cooking_text = if !setting.detail {
        // 料理名ごとの回数(多い順、同数は出てきた順)
        let mut count: Vec<(&str, usize)> = Vec::new();
        for c in &team.cooking_list {
            let name = m.cookings[c.cooking].name.as_str();
            match count.iter_mut().find(|x| x.0 == name) {
                Some(x) => x.1 += 1,
                None => count.push((name, 1)),
            }
        }
        count.sort_by_key(|a| std::cmp::Reverse(a.1));
        count.iter().map(|(name, n)| format!("{name}x{n}")).collect::<Vec<_>>().join(" / ")
    } else {
        team.cooking_list
            .iter()
            .map(|c| {
                let cooking = &m.cookings[c.cooking];
                let bonus = c.fix_energy / cooking.energy;
                let add = if c.add_energy > 0.0 { format!(" ＋ {}(追加)", round_str(c.add_energy)) } else { String::new() };
                format!(
                    "{} 鍋{} ( {} × {}%(Lv) × {}%(大成功){add} ) × {}%(設定) ＝ {}エナジー",
                    cooking.name,
                    c.pot_size,
                    round_str(cooking.energy),
                    pokesle_core::js::round(bonus * 100.0),
                    pokesle_core::js::round(c.success_power * 100.0),
                    pokesle_core::js::round(config.simulation.cooking_weight * 100.0),
                    round_str(c.energy)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    // 料理の内訳が1行に収まるようにポケモン列を広げる(本家は表が中身に合わせて広がる)
    let cooking_size = if setting.detail { 11.0 } else { 13.0 };
    let line_w = cooking_text
        .lines()
        .map(|l| ui.painter().layout_no_wrap(l.to_string(), egui::FontId::proportional(cooking_size), egui::Color32::BLACK).size().x)
        .fold(0.0_f32, f32::max);
    let member_w = MEMBER_W.max((line_w + PAD * 2.0 + 2.0) / n.max(1) as f32);
    let mut widths = vec![HEAD_W, HEAD_W];
    widths.extend(std::iter::repeat_n(member_w, n));
    // 食材情報の合計(既定 + 追加 - 使用 = 残り)は折り返さない幅にする
    widths.push(if setting.food { 230.0 } else { TOTAL_W });
    let table = Table { widths, left: ui.cursor().min.x };
    let members: Vec<&TeamMember> = team.pokemon_list.iter().collect();
    let pokemon = member_pokemon;
    ui.spacing_mut().item_spacing.y = 0.0;

    // 見出し
    let mut head = vec![empty(1), empty(1)];
    head.extend((1..=n).map(|i| th(1, i.to_string())));
    head.push(th(1, "合計"));
    table.row(ui, None, head);

    // 編成
    let mut group_started = false;
    let mut group = |ui: &mut egui::Ui, title: &str, cells: Vec<Cell>| {
        let start = !group_started;
        group_started = true;
        let mut all = vec![th(1, title)];
        all.extend(cells);
        table.row(ui, Some((if start { "編成" } else { "" }, start)), all);
    };
    let per = |f: &dyn Fn(&TeamMember) -> Cell<'static>| -> Vec<Cell<'static>> { members.iter().map(|t| f(t)).collect() };

    let mut cells: Vec<Cell> = members
        .iter()
        .map(|t| match t {
            TeamMember::Pokemon { pokemon, .. } => {
                let p = pokemon.as_ref();
                td(1, move |ui| name_cell(ui, p))
            }
            TeamMember::NightCapPikachu(_) => td(1, |ui| {
                ui.label("ナイトキャップピカチュウ");
            }),
        })
        .collect();
    cells.push(empty(1));
    group(ui, "名前", cells);

    let mut cells: Vec<Cell> = members
        .iter()
        .map(|t| {
            let text = match t {
                TeamMember::Pokemon { pokemon, .. } => match pokemon.box_pokemon.as_ref().map(|b| b.lv).filter(|lv| *lv != pokemon.lv) {
                    Some(before) => format!("{before}→{}", pokemon.lv),
                    None => pokemon.lv.to_string(),
                },
                TeamMember::NightCapPikachu(p) => p.lv.to_string(),
            };
            td(1, move |ui| {
                ui.label(text);
            })
        })
        .collect();
    cells.push(empty(1));
    group(ui, "Lv", cells);

    if result_config.simulation.fix && result_config.simulation.fix_resource_mode != 0.0 {
        let mut cells = per(&|t| num(1, pokemon(t).map(|p| fmt_num(p.use_candy)).unwrap_or_default()));
        cells.push(empty(1));
        group(ui, "使用アメ", cells);
        let mut cells = per(&|t| num(1, pokemon(t).map(|p| fmt_num(p.use_shard)).unwrap_or_default()));
        cells.push(empty(1));
        group(ui, "使用ゆめのかけら", cells);
    }

    let mut cells: Vec<Cell> = members
        .iter()
        .map(|t| match pokemon(t).and_then(|p| p.box_pokemon.as_ref()) {
            Some(b) => td(1, move |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(24.0);
                    food_icon::food_list(ui, b);
                });
            }),
            None => empty(1),
        })
        .collect();
    cells.push(empty(1));
    group(ui, "食材", cells);

    let mut cells: Vec<Cell> = members
        .iter()
        .map(|t| match pokemon(t) {
            Some(p) => td(1, move |ui| sub_skill_cell(ui, p, sub_skill_short)),
            None => empty(1),
        })
        .collect();
    cells.push(empty(1));
    group(ui, "サブスキル", cells);

    let mut cells: Vec<Cell> = members
        .iter()
        .map(|t| match pokemon(t).and_then(|p| p.nature) {
            Some(nature) => td(1, move |ui| {
                ui.horizontal(|ui| nature_label(ui, &m.natures[nature].name));
            }),
            None => empty(1),
        })
        .collect();
    cells.push(empty(1));
    group(ui, "せいかく", cells);

    // 0 や未定義なら空欄(本家の v-if)
    let pct = |x: f64| if x != 0.0 && !x.is_nan() { format!("{}%", fmt_num(x * 100.0)) } else { String::new() };
    if setting.detail {
        for (title, get) in [
            ("おてスピ短縮", (|p: &SimPokemon| p.speed_bonus) as fn(&SimPokemon) -> f64),
            ("日中手伝い倍率", |p: &SimPokemon| p.day_help_rate),
            ("夜間手伝い倍率", |p: &SimPokemon| p.night_help_rate),
        ] {
            let mut cells = per(&|t| num(1, pokemon(t).map(|p| pct(get(p))).unwrap_or_default()));
            cells.push(empty(1));
            group(ui, title, cells);
        }
    }

    let b_ep_d = |t: &TeamMember| match t {
        TeamMember::Pokemon { pokemon, .. } => pokemon.b_ep_d,
        TeamMember::NightCapPikachu(p) => p.b_ep_d,
    };
    let mut cells = per(&|t| num(1, round_str(b_ep_d(t))));
    cells.push(num(1, round_str(members.iter().map(|t| b_ep_d(t)).sum())));
    group(ui, "きのみ", cells);

    let mut cells = per(&|t| {
        num(1, pokemon(t).map(|p| format!("{}(Lv{})", m.pokemon_skill(p.base()).name, p.fixed_skill_lv)).unwrap_or_default())
    });
    cells.push(empty(1));
    group(ui, "スキル", cells);

    let nonzero = |x: f64| x != 0.0 && !x.is_nan();
    let mut cells = per(&|t| num(1, pokemon(t).map(|p| p.skill_per_day).filter(|x| nonzero(*x)).map(|x| format!("{x:.1}")).unwrap_or_default()));
    cells.push(empty(1));
    group(ui, "スキル回数", cells);

    let skill_energy = |t: &TeamMember| pokemon(t).map_or(0.0, |p| p.skill_energy_per_day);
    let mut cells = per(&|t| num(1, Some(skill_energy(t)).filter(|x| nonzero(*x)).map(round_str).unwrap_or_default()));
    cells.push(num(1, round_str(members.iter().map(|t| skill_energy(t)).sum())));
    group(ui, "スキルエナジー", cells);

    let mut cells = per(&|t| {
        let v = match t {
            TeamMember::Pokemon { shard_bonus, .. } => *shard_bonus,
            TeamMember::NightCapPikachu(_) => 0.0,
        };
        num(1, if nonzero(v) { round_str(v) } else { String::new() })
    });
    cells.push(num(1, round_str(members.iter().map(|t| pokemon(t).map_or(0.0, |p| p.shard)).sum())));
    group(ui, "ゆめのかけら", cells);

    table.row(
        ui,
        None,
        vec![
            th(2, "料理"),
            td(n, move |ui| {
                ui.label(egui::RichText::new(cooking_text).size(cooking_size));
            }),
            num(1, round_str(team.cooking_list.iter().map(|c| c.energy).sum())),
        ],
    );
    table.row(ui, None, vec![th(2, "残り食材"), num(n + 1, pokesle_core::js::to_fixed(team.remain_food_energy, 1))]);

    let span = n + 1;
    let fb = 1.0 + config.simulation.field_bonus / 100.0;
    table.row(ui, Some(("エナジー", true)), vec![th(1, "エナジー"), num(span, round_str(team.raw_energy))]);
    table.row(ui, Some(("", false)), vec![th(1, "FB込み"), num(span, round_str(team.raw_energy * fb))]);
    table.row(ui, Some(("ゆめのかけら", true)), vec![th(1, "エナジー"), num(span, round_str(team.energy_shard))]);
    table.row(ui, Some(("", false)), vec![th(1, "ボーナス"), num(span, round_str(team.bonus_shard))]);
    table.row(ui, Some(("", false)), vec![th(1, "スキル"), num(span, round_str(team.skill_shard))]);
    table.row(ui, None, vec![th(2, "スコア"), num(span, round_str(team.score))]);

    if setting.food {
        for (fi, food) in m.foods.iter().enumerate() {
            let mut cells: Vec<Cell> = vec![td(1, move |ui| {
                food_icon::food_icon(ui, &food.name, 24.0);
            })];
            cells.extend(members.iter().map(|t| {
                let v = t.food_count().get(fi).copied().unwrap_or(0.0);
                num(1, if nonzero(v) { format!("{v:.1}") } else { String::new() })
            }));
            let get = |l: &[f64]| l.get(fi).copied().unwrap_or(0.0);
            let (default, add, use_, total) = (get(&team.default_food_num), get(&team.add_food_num), get(&team.use_food_num), get(&team.food_num));
            cells.push(td(1, move |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.label(format!("{default:.1}"));
                    if nonzero(add) {
                        ui.label(egui::RichText::new(format!("+ {add:.1}")).color(egui::Color32::from_rgb(0xd4, 0x00, 0x63)));
                    }
                    if nonzero(use_) {
                        ui.label(egui::RichText::new(format!("- {use_:.1}")).color(egui::Color32::from_rgb(0x00, 0x0f, 0x9b)));
                    }
                    ui.label(format!("= {total:.1}"));
                });
            }));
            table.row(ui, Some((if fi == 0 { "食材" } else { "" }, fi == 0)), cells);
        }
    }
    table.close(ui);
}
