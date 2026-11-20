//! 厳選値のクリックで開く厳選詳細(本家 components/evaluate-table-detail-popup.vue)

use eframe::egui;
use pokesle_core::config::Config;
use pokesle_core::cooking::get_chance_week_effect;
use pokesle_core::data::{master, Pokemon, Specialty};
use pokesle_core::evaluate_table::{evaluate_kaihou_percentile, EvaluateTable, PercentileSet};
use pokesle_core::help_rate::{HelpRate, HelpRateResult};
use pokesle_core::js::to_fixed;
use pokesle_core::sim_pokemon::SimPokemon;
use pokesle_core::simulator::{Mode, PokemonSimulator};

use super::{fmt_num, food_icon, nature_label, style};

/// 計算結果(本家の evaluateResult)
struct Computed {
    p: SimPokemon,
    score: f64,
    score_for_healer: f64,
    score_for_support: f64,
    healer_help_rate: HelpRateResult,
    default_help_rate: HelpRateResult,
    percentile: Option<PercentileSet>,
}

pub struct EvaluateDetail {
    name: String,
    lv: u32,
    /// 食材の組み合わせ(進化先の食材の添字。レベルで使える分だけ)
    food_index_list: Vec<usize>,
    sub_skill_list: Vec<String>,
    /// 性格(マスターの添字)
    nature: Option<usize>,
    computed: Option<Result<Computed, String>>,
    /// パーセンタイルのグラフを最初から開く(基準確認からは、その厳選度の個体なので閉じておく)
    open_percentile: bool,
}

impl EvaluateDetail {
    /// `food_index_list` は3枠分(本家の props.foodIndexList)
    pub fn new(name: &str, lv: u32, food_index_list: Vec<usize>, sub_skill_list: Vec<String>, nature: Option<usize>) -> EvaluateDetail {
        let food_num = if lv < 30 { 1 } else if lv < 60 { 2 } else { 3 };
        EvaluateDetail {
            name: name.to_string(),
            lv,
            food_index_list: food_index_list.into_iter().take(food_num).collect(),
            sub_skill_list,
            nature,
            computed: None,
            open_percentile: true,
        }
    }

    /// パーセンタイルのグラフを閉じた状態で開く(本家 evaluate-table.vue の `percentile: false`)
    pub fn closed_percentile(mut self) -> EvaluateDetail {
        self.open_percentile = false;
        self
    }

    fn compute(&self, config: &Config, table: Option<&EvaluateTable>) -> Result<Computed, String> {
        let m = master();
        let base = m.pokemon(&self.name).ok_or("ポケモンが見つかりません")?;
        let table = table.ok_or("厳選テーブルがありません")?;
        let lv = self.lv;
        let score_for_healer = *table.healer.get(&lv).ok_or("このレベルの厳選テーブルがありません")?;
        let score_for_support = *table.support.get(&lv).ok_or("このレベルの厳選テーブルがありません")?;

        let sub_skill_num = [10, 25, 50, 70, 80].iter().filter(|&&x| lv >= x).count();
        let sub_skill_list: Vec<String> =
            m.use_silver_seed(&self.sub_skill_list, Some(&config.select_evaluate.silver_seed)).into_iter().take(sub_skill_num).collect();
        let sub_skill_idx: Vec<Option<usize>> = sub_skill_list.iter().map(|n| m.sub_skills.iter().position(|s| s.name == *n)).collect();

        let mut sim = PokemonSimulator::new(config.clone(), Mode::Select);
        let foods: Vec<String> = self.food_index_list.iter().map(|&f| base.food_name_list.get(f).cloned().unwrap_or_default()).collect();
        let mut p = sim.from_evaluate(base, lv as f64, &foods)?;
        sim.select_evaluate(&mut p, &sub_skill_idx, self.nature, score_for_healer, score_for_support)?;
        let score = sim.select_evaluate_to_score(
            &p,
            sub_skill_list.iter().any(|x| x == "ゆめのかけらボーナス"),
            sub_skill_list.iter().any(|x| x == "リサーチEXPボーナス"),
            true,
        );
        let mut help_rate = HelpRate::new(config, None);
        let healer_help_rate = help_rate.get_help_rate(&p.other_heal_list);
        let default_help_rate = help_rate.get_help_rate(&[]);
        let food_key: String = self.food_index_list.iter().map(|x| x.to_string()).collect();
        let percentile = table
            .pokemon
            .get(&self.name)
            .and_then(|l| l.get(&lv))
            .and_then(|l| l.get(&food_key))
            .cloned()
            .or_else(|| evaluate_kaihou_percentile(config, table, &self.name, lv, &foods));
        Ok(Computed { p, score, score_for_healer, score_for_support, healer_help_rate, default_help_rate, percentile })
    }

    /// 閉じたら false
    pub fn ui(&mut self, ctx: &egui::Context, config: &Config, table: Option<&EvaluateTable>) -> bool {
        if self.computed.is_none() {
            self.computed = Some(self.compute(config, table));
        }
        let mut open = true;
        let title = format!("{}の厳選詳細", self.name);
        egui::Window::new(title)
            .id(egui::Id::new("evaluate_detail"))
            .open(&mut open)
            .collapsible(false)
            .default_size([900.0, 700.0])
            .vscroll(true)
            .show(ctx, |ui| {
                let m = master();
                let Some(base) = m.pokemon(&self.name) else { return };
                ui.horizontal(|ui| {
                    ui.label(format!("{}(", self.name));
                    for &f in &self.food_index_list {
                        if let Some(name) = base.food_name_list.get(f) {
                            food_icon::food_icon(ui, name, 22.0);
                        }
                    }
                    ui.label(format!(") Lv{}", self.lv));
                });
                match self.computed.as_ref() {
                    Some(Ok(c)) => detail(ui, config, base, c, self.open_percentile),
                    Some(Err(e)) => {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                    None => {}
                }
            });
        open
    }
}

fn detail(ui: &mut egui::Ui, config: &Config, base: &Pokemon, c: &Computed, open_percentile: bool) {
    ui.label(egui::RichText::new(format!("厳選スコア：{}", fmt_num(c.score))).size(20.0).strong());
    ui.add_space(10.0);

    // パーセンタイル(得意分野は開いておく)
    let mut graphs = vec![("energy", "総合スコア", true)];
    match base.specialty {
        Specialty::Berry => graphs.push(("berry", "きのみ", true)),
        Specialty::Food => graphs.push(("food", "食材", true)),
        Specialty::Skill => graphs.push(("skill", "スキル", true)),
        Specialty::All => {}
    }
    for (key, name) in [("berry", "きのみ"), ("food", "食材"), ("skill", "スキル")] {
        if graphs.iter().all(|g| g.0 != key) {
            graphs.push((key, name, false));
        }
    }
    let values = |key: &str| match key {
        "energy" => c.score,
        "berry" => c.p.berry_num_per_day,
        "food" => c.p.food_num_per_day,
        _ => c.p.skill_per_day,
    };
    let width = (ui.available_width() - 10.0) / 2.0;
    for pair in graphs.chunks(2) {
        ui.horizontal_top(|ui| {
            for &(key, name, open) in pair {
                ui.vertical(|ui| {
                    ui.set_width(width);
                    egui::CollapsingHeader::new(format!("{name}パーセンタイル")).id_salt(("evaluate_detail_graph", key)).default_open(open && open_percentile).show(ui, |ui| {
                        match c.percentile.as_ref() {
                            Some(p) => {
                                let border = (config.simulation.select_type == 1.0).then(|| p.get(key).get(config.simulation.select_border as usize).copied()).flatten();
                                percentile_chart(ui, p.get(key), values(key), &format!("{name}厳選度"), border);
                            }
                            None => {
                                ui.label("この食材の組み合わせの厳選テーブルがありません");
                            }
                        }
                    });
                });
            }
        });
    }

    ui.add_space(10.0);
    egui::CollapsingHeader::new("エナジー期待値計算").id_salt("evaluate_detail_table").default_open(true).show(ui, |ui| {
        energy_table(ui, config, base, c);
    });
}

/// パーセンタイルのグラフ(本家は chart.js の Line)。本個体は赤い点、select_type が 2段階評価ならボーダーの赤線
fn percentile_chart(ui: &mut egui::Ui, list: &[f64], score: f64, label: &str, border: Option<f64>) {
    if list.is_empty() {
        return;
    }
    // 本家と同じ求め方の、本個体のパーセンタイル
    let mut upper = list.iter().position(|&x| x >= score).map_or(100, |i| i.min(100));
    upper = upper.min(list.len() - 1);
    let lower = list.iter().rposition(|&x| x <= score).unwrap_or(0);
    let x_this = if list[lower] == list[upper] { upper as f64 } else { (score - list[lower]) / (list[upper] - list[lower]) + lower as f64 };

    let point = egui::Color32::from_rgb(0xee, 0x00, 0x00);
    let line = egui::Color32::from_rgb(0x88, 0xdd, 0xdd);
    ui.horizontal(|ui| {
        let legend = |ui: &mut egui::Ui, color: egui::Color32, text: &str| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 0.0, color);
            ui.label(egui::RichText::new(text).size(11.0));
        };
        legend(ui, point, "本個体");
        ui.add_space(8.0);
        legend(ui, line, label);
    });

    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 280.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let font = egui::FontId::proportional(10.0);
    let text_color = style::muted();
    let mut min = list.iter().copied().fold(f64::INFINITY, f64::min).min(score);
    let mut max = list.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(score);
    if let Some(b) = border {
        min = min.min(b);
        max = max.max(b);
    }
    if max <= min {
        max = min + 1.0;
    }
    let (min, max, step) = nice_range(min, max);
    let plot = egui::Rect::from_min_max(rect.min + egui::vec2(56.0, 6.0), rect.max - egui::vec2(8.0, 20.0));
    let to_screen = |x: f64, y: f64| {
        egui::pos2(plot.left() + (x / 100.0) as f32 * plot.width(), plot.bottom() - ((y - min) / (max - min)) as f32 * plot.height())
    };
    let grid = egui::Stroke::new(1.0, style::line_color());
    let mut y = min;
    while y <= max + step * 0.01 {
        let pos = to_screen(0.0, y);
        painter.hline(plot.x_range(), pos.y, grid);
        painter.text(egui::pos2(plot.left() - 4.0, pos.y), egui::Align2::RIGHT_CENTER, fmt_tick(y, step), font.clone(), text_color);
        y += step;
    }
    for x in (0..=100).step_by(10) {
        let pos = to_screen(x as f64, min);
        painter.vline(pos.x, plot.y_range(), grid);
        painter.text(egui::pos2(pos.x, plot.bottom() + 3.0), egui::Align2::CENTER_TOP, x.to_string(), font.clone(), text_color);
    }
    let points: Vec<egui::Pos2> = list.iter().enumerate().map(|(x, &y)| to_screen(x as f64, y)).collect();
    painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(2.0, line)));
    for p in &points {
        painter.circle_filled(*p, 2.0, line);
    }
    if let Some(b) = border {
        let pos = to_screen(0.0, b);
        painter.hline(plot.x_range(), pos.y, egui::Stroke::new(1.0, egui::Color32::RED));
    }
    let me = to_screen(x_this, score);
    painter.circle_filled(me, 4.0, point);

    // マウスの位置に近い点の値
    if let Some(pos) = response.hover_pos() {
        let x = (((pos.x - plot.left()) / plot.width()) * 100.0).round().clamp(0.0, (list.len() - 1) as f32) as usize;
        let text = if (me - pos).length() < 8.0 {
            format!("本個体: ({}, {})", to_fixed(x_this, 1), to_fixed(score, 1))
        } else {
            format!("{label}: ({x}, {})", to_fixed(list[x], 1))
        };
        response.on_hover_text_at_pointer(text);
    }
}

/// 目盛りの範囲と間隔(5〜10本ほど)
pub(super) fn nice_range(min: f64, max: f64) -> (f64, f64, f64) {
    let raw = (max - min) / 8.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0].iter().map(|k| k * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    ((min / step).floor() * step, (max / step).ceil() * step, step)
}

pub(super) fn fmt_tick(y: f64, step: f64) -> String {
    if step >= 1.0 { fmt_num(y) } else { to_fixed(y, (-step.log10().floor()) as usize) }
}

/// エナジー期待値計算の表(本家と同じ式の説明)
fn energy_table(ui: &mut egui::Ui, config: &Config, base: &Pokemon, c: &Computed) {
    let m = master();
    let p = &c.p;
    let se = &config.select_evaluate;
    let f = to_fixed;
    let n = style::js_num;
    let sub_names: Vec<&str> = p.sub_skill_list.iter().flatten().map(|&i| m.sub_skills[i].name.as_str()).collect();
    let has = |name: &str| sub_names.contains(&name);
    let nature = p.nature.map(|i| &m.natures[i]);
    let good = |s: &str| nature.and_then(|x| x.good.as_deref()) == Some(s);
    let weak = |s: &str| nature.and_then(|x| x.weak.as_deref()) == Some(s);
    let skill = m.pokemon_skill(base);
    let metronome = skill.name == "ゆびをふる";
    let metronome_num = m.metronome_target.len();
    let metronome_text = if metronome { format!("\n÷ {metronome_num}") } else { String::new() };
    let specialty = se.specialty.get(base.specialty.name());
    let berry_energy_rate = specialty.map_or(f64::NAN, |s| s.berry_energy_rate);
    let food_energy_rate = specialty.map_or(f64::NAN, |s| s.food_energy_rate);
    let food_get_rate = specialty.map_or(f64::NAN, |s| s.food_get_rate);
    let sleep = config.sleep_time;
    let base_food_rate = base.food_rate;

    // (グループ, 項目, 式, 値)。グループは最初の行だけ書く
    let mut rows: Vec<(String, String, String, String)> = Vec::new();
    let mut row = |group: &str, item: &str, formula: String, value: String| rows.push((group.into(), item.into(), formula, value));

    row("個体", "サブスキル", sub_names.join(" / "), String::new());
    row("", "せいかく", nature.map_or("無補正".to_string(), |x| x.name.clone()), String::new());

    let mut s = format!("{}%", f(base_food_rate * 100.0, 1));
    if has("食材確率アップS") || has("食材確率アップM") {
        s += &format!(
            "\n× (100%{}{}) ※サブスキル",
            if has("食材確率アップS") { " + 18%" } else { "" },
            if has("食材確率アップM") { " + 36%" } else { "" }
        );
    }
    if good("食材お手伝い確率") {
        s += "\n× 120% ※せいかく";
    }
    if weak("食材お手伝い確率") {
        s += "\n× 80% ※せいかく";
    }
    row("", "食材確率", s, format!("{}%", f(p.food_rate * 100.0, 1)));

    let mut s = n(base.bag);
    for (name, add) in [("最大所持数アップS", 6), ("最大所持数アップM", 12), ("最大所持数アップL", 18)] {
        if has(name) {
            s += &format!("\n+ {add} ※{name}");
        }
    }
    let st = p.sleep_time.unwrap_or(0.0);
    for (border, add) in [(2000.0, 8), (1000.0, 6), (500.0, 3), (200.0, 1)] {
        if st >= border {
            s += &format!("\n+ {add} ※睡眠{border}時間");
            break;
        }
    }
    row("", "所持数", s, n(p.bag));

    let food_nums: Vec<String> = p.food_list.iter().map(|x| n(x.num)).collect();
    row(
        "",
        "所持数最大まで\nおてつだい回数",
        format!(
            "{} ÷ (\n　{} × (100% - {}%) ※きのみ分\n　+ ({}) ÷ {} × {}% ※食材分\n)\n+ 4 (おてつだいキュー)",
            n(p.bag),
            n(p.berry_num),
            f(p.food_rate * 100.0, 1),
            food_nums.join(" + "),
            p.food_list.len(),
            f(p.food_rate * 100.0, 1)
        ),
        f(p.bag_full_help_num, 2),
    );

    let mut s = format!("{}\n× (100% - ({} - 1) × 0.2%) ※Lv補正\n× (100% - {}%)", n(base.help), n(p.lv), f(p.speed_bonus * 100.0, 0));
    if good("手伝いスピード") {
        s += "\n× 90% ※せいかく";
    }
    if weak("手伝いスピード") {
        s += "\n× 107.5% ※せいかく";
    }
    row("お手伝い", "おてつだいスピード", s, f(p.speed, 0));

    let heal: f64 = p.all_heal_list.iter().map(|x| x.effect).sum();
    let nature_heal = if good("げんき回復量") {
        "× 120% ※せいかく"
    } else if weak("げんき回復量") {
        "× 88% ※せいかく"
    } else {
        "× 100% ※せいかく"
    };
    row("", "げんき回復量", format!("{}\n{nature_heal}", f(heal, 1)), f(heal * p.nature_genki_multiplier, 1));
    row(
        "",
        "日中手伝い回数",
        format!("(24 - {}) × 3600 ÷ {}\n× {}% ※げんき補正", n(sleep), n(p.speed), f(p.day_help_num / (24.0 - sleep) / 3600.0 * p.speed * 100.0, 1)),
        f(p.day_help_num, 2),
    );
    row(
        "",
        "夜間手伝い回数",
        format!("{} × 3600 ÷ {}\n× {}% ※げんき補正", n(sleep), n(p.speed), f(p.night_help_num / sleep / 3600.0 * p.speed * 100.0, 1)),
        f(p.night_help_num, 2),
    );
    let check = n(config.check_freq - 1.0);
    row(
        "",
        "通常手伝い回数",
        format!(
            "min({} ÷ {check}, {}) × {check} ※{check}は日中チェック回数から朝イチの1回を引いた数\n+ min({}, {})",
            f(p.day_help_num, 2),
            f(p.bag_full_help_num, 2),
            f(p.night_help_num, 2),
            f(p.bag_full_help_num, 2)
        ),
        f(p.normal_help_num, 2),
    );
    row("", "いつ育回数", format!("{} + {} - {}", f(p.day_help_num, 2), f(p.night_help_num, 2), f(p.normal_help_num, 2)), f(p.berry_help_num, 2));

    let berry = &m.berries[base.berry];
    let mut s = format!(
        "max({e} + {lv} - 1, {e} × 102.5% ^ ({lv} - 1))\n× {} ※きのみの数",
        n(p.berry_num),
        e = n(berry.energy),
        lv = n(p.lv)
    );
    if berry_energy_rate != 100.0 {
        s += &format!("\n× {}% ※好物補正", n(berry_energy_rate));
    }
    s += &format!(
        "\n× ({} × (100% - {}%) + {}) ※通常手伝い×きのみ率＋いつ育",
        f(p.normal_help_num, 2),
        f(p.food_rate * 100.0, 1),
        f(p.berry_help_num, 2)
    );
    row("", "きのみエナジー", s, f(p.b_ep_d, 1));

    let mut blocks = Vec::new();
    for (i, food) in m.foods.iter().enumerate() {
        let count = p.food_count.get(i).copied().unwrap_or(0.0);
        if count == 0.0 || count.is_nan() {
            continue;
        }
        blocks.push(format!(
            "【{}】\n{}エナジー\n× {}個\n× (\n　(\n　　{}% ({})\n　　× {}% (最大レシピボーナス)\n　) × {}% ※最良の料理に使える割合\n　+ 100% × {}% ※隙間に詰めるだけの割合\n)\n× (110%×18 ＋ 160%×3) ÷ 21 ※週平均の大成功の倍率",
            food.name,
            n(food.energy),
            f(count, 1),
            f(food.best_rate * 100.0, 0),
            food.best_cooking.as_deref().unwrap_or(""),
            n(m.cooking_stats.max_recipe_bonus * 100.0),
            n(food_energy_rate),
            n(100.0 - food_energy_rate)
        ));
    }
    row("", "食材エナジー", blocks.join("\n\n"), f(p.f_ep_d, 1));

    // スキル
    let mut s = format!("{}%", f(base.skill_rate * 100.0, 2));
    if has("スキル確率アップS") || has("スキル確率アップM") {
        s += &format!(
            "\n× (100%{}{}) ※サブスキル",
            if has("スキル確率アップS") { " + 18%" } else { "" },
            if has("スキル確率アップM") { " + 36%" } else { "" }
        );
    }
    if good("メインスキル発生確率") {
        s += "\n× 120% ※せいかく";
    }
    if weak("メインスキル発生確率") {
        s += "\n× 80% ※せいかく";
    }
    row("スキル", "スキル確率", s, format!("{}%", f(p.skill_rate * 100.0, 2)));
    let s = if base.specialty == Specialty::Skill {
        format!("40 × 3600 ÷ {}\n※スキルタイプは40時間÷基礎おてつだい時間(小数点以下の扱い不明)", n(base.help))
    } else {
        "スキルタイプ以外は78回固定".to_string()
    };
    row("", "天井", s, f(p.skill_ceil, 2));
    row(
        "",
        "天井補正後\nスキル確率",
        format!("{r}% ÷ (100% - (100% - {r}%) ^ {})", n(p.skill_ceil), r = f(p.skill_rate * 100.0, 2)),
        format!("{}%", f(p.ceil_skill_rate * 100.0, 2)),
    );
    let s = if se.expect_type.get(&skill.name).copied() == Some(0.0) {
        format!("{}% × {}", f(p.ceil_skill_rate * 100.0, 2), f(p.normal_help_num, 2))
    } else {
        format!(
            "{}% を {} 回試行した時、{}%の確率で最低発生する回数",
            f(p.ceil_skill_rate * 100.0, 2),
            f(p.normal_help_num, 2),
            se.expect_type.get("border").map_or("undefined".to_string(), |x| n(*x))
        )
    };
    row("", "スキル回数", s, f(p.skill_per_day, 2));
    let mut s = p.skill_lv.map_or("undefined".to_string(), n);
    if has("スキルレベルアップS") {
        s += " + 1 (サブスキル)";
    }
    if has("スキルレベルアップM") {
        s += " + 2 (サブスキル)";
    }
    row("", "スキルLv", s, n(p.fixed_skill_lv));

    for sw in &p.skill_weight_list {
        let sk = &m.skills[sw.skill];
        let lv = (p.fixed_skill_lv as usize).min(sk.effect.len()).max(1);
        let e = &sk.effect[lv - 1];
        let v = |x: Option<f64>| x.map_or("undefined".to_string(), n);
        let mut parts: Vec<String> = Vec::new();
        match sk.name.as_str() {
            "エナジーチャージS" | "エナジーチャージS(ランダム)" | "エナジーチャージM" => parts.push(format!("{}{metronome_text}", v(e.value))),
            "食材ゲットS" | "プレゼント(食材ゲットS)" => {
                let mut s = "(".to_string();
                for (i, food) in m.foods.iter().enumerate() {
                    s += &format!(
                        "\n{}{} × (({}% × {}% - 100%) × {}% + 100%) ※{}",
                        if i > 0 { "+ " } else { "" },
                        n(food.energy),
                        f(food.best_rate * 100.0, 0),
                        n(m.cooking_stats.max_recipe_bonus * 100.0),
                        n(food_get_rate),
                        food.name
                    );
                }
                s += &format!("\n)\n÷ {}\n× {}{metronome_text}", m.foods.len(), v(e.value));
                parts.push(s);
            }
            "げんきチャージS" => parts.push("げんき回復量で計算済み".into()),
            "おてつだいサポートS" | "いやしのはどう(げんきエールS)" | "おてつだいブースト" => {
                let times = match sk.name.as_str() {
                    "おてつだいブースト" => format!("× {} × 5", v(e.max)),
                    "いやしのはどう(げんきエールS)" => format!("× {} × 5", v(e.help2)),
                    _ => format!("× {}", v(e.value)),
                };
                parts.push(format!(
                    "{} ※厳選度{}%の上位{}%のおてつだいあたりのエナジーの平均値\n{times}{metronome_text}",
                    f(c.score_for_support, 1),
                    n(se.support_border),
                    n(se.support_rank_num)
                ));
            }
            _ => {}
        }
        if sk.genki && sk.team {
            if !metronome || sk.name == "げんきエールS" {
                let other: f64 = p.other_heal_list.iter().map(|x| x.effect).sum();
                parts.push(format!(
                    "1日の1匹あたりの回復量：{}\n\n{} ※厳選度{}%の上位{}%のげんき補正なしエナジーの平均値\n× (\n　({}% * (24 - {s}) + {}% * {s}) ※ヒーラーありのおてつだい倍率\n　÷ ({}% * (24 - {s}) + {}% * {s}) ※ヒーラーなしのおてつだい倍率\n　- 1\n)\n× 4 ※4匹分\n÷ {} ※スキル発動回数で割って1回あたりの効果にする",
                    f(other, 1),
                    f(c.score_for_healer, 1),
                    n(se.support_border),
                    n(se.support_rank_num),
                    f(c.healer_help_rate.day * 100.0, 1),
                    f(c.healer_help_rate.night * 100.0, 1),
                    f(c.default_help_rate.day * 100.0, 1),
                    f(c.default_help_rate.night * 100.0, 1),
                    f(p.skill_per_day, 2),
                    s = n(sleep)
                ));
            } else {
                parts.push("げんきエールSに含む".into());
            }
        }
        match sk.name.as_str() {
            "ゆめのかけらゲットS" | "ゆめのかけらゲットS(ランダム)" => {
                let x = e.value.unwrap_or(f64::NAN);
                parts.push(format!("{}{metronome_text} ＝ {} (ゆめのかけら、総計内で計算)", v(e.value), f(x * sw.weight, 1)));
            }
            "はどうだん(ゆめのかけらゲットS)" => {
                let (shard, energy) = (e.shard.unwrap_or(f64::NAN), e.energy.unwrap_or(f64::NAN));
                parts.push(format!(
                    "{}{metronome_text} ＝ {} (ゆめのかけら、総計内で計算)\n{}{metronome_text} ＝ {}",
                    v(e.shard),
                    f(shard * sw.weight, 1),
                    v(e.energy),
                    f(energy * sw.weight, 1)
                ));
            }
            "料理パワーアップS" | "マイナス(料理パワーアップS)" => {
                let effect = if sk.name == "料理パワーアップS" { e.value } else { e.main }.unwrap_or(f64::NAN);
                let num = p.skill_per_day / if metronome { metronome_num as f64 } else { 1.0 };
                let cs = &m.cooking_stats;
                let per_energy = if se.cooking_power_up_type == 0.0 { cs.cooking_power_up_energy } else { cs.cooking_power_up_energy_average } * se.cooking_power_up_rate / 100.0;
                let average = m.food_stats.average_energy;
                let mut s = String::new();
                for (i, name) in ["朝", "昼", "晩"].iter().enumerate() {
                    let each = (num / 3.0).floor() + (num - (num / 3.0).floor() * 3.0 - i as f64).clamp(0.0, 1.0);
                    let limit = cs.max_food_num - cs.pot_max;
                    let main_effect = per_energy * limit.min(effect * each);
                    let remain = average * (effect * each - limit).max(0.0);
                    if i > 0 {
                        s += "\n\n";
                    }
                    s += &format!(
                        "{name}：{} × {}回＝{}\n有効範囲エナジー：{} × min({}, {})＝{}\n　余剰分エナジー：{} × max({} - {}, 0)＝{}",
                        n(effect),
                        f(each, 2),
                        f(effect * each, 1),
                        f(per_energy, 1),
                        n(limit),
                        f(effect * each, 1),
                        f(main_effect, 1),
                        f(average, 1),
                        f(effect * each, 1),
                        n(limit),
                        f(remain, 1)
                    );
                }
                s += &format!(
                    "\n\n計：{}\n1回あたりのエナジー：{} ÷ {}＝{}\n\n※{}は料理3種ごとの最大エナジーと食材数{}以下の最大エナジーの差を食材数の差で割った値{}に、{}%をかけたもの",
                    f(p.skill_energy_per_day, 1),
                    f(p.skill_energy_per_day, 1),
                    f(num, 2),
                    f(p.skill_energy_per_day / num, 1),
                    f(per_energy, 1),
                    n(cs.pot_max),
                    if se.cooking_power_up_type == 0.0 { "の最大" } else { "の平均値" },
                    n(se.cooking_power_up_rate)
                );
                parts.push(s);
            }
            "料理チャンスS" => {
                let x = e.value.unwrap_or(f64::NAN);
                let week = x * p.skill_per_day * 7.0 / if metronome { metronome_num as f64 } else { 1.0 };
                parts.push(format!(
                    "週の効果量\n{} × {} × 7{metronome_text}\n＝{}\n\n({}\n- 24.6) ※週の効果量から料理倍率の期待値を計算し、通常時からの増分を計算\n× {} ※今一番いい料理\n÷ ({} × 7) ※週の発動回数で割って1回あたりの効果にする",
                    v(e.value),
                    f(p.skill_per_day, 2),
                    f(week, 1),
                    f(get_chance_week_effect(week / 100.0, None, 0.0).total, 2),
                    f(m.cooking_stats.max_energy, 0),
                    f(p.skill_per_day, 2)
                ));
            }
            _ => {}
        }
        let value = p.skill_energy_map.iter().find(|(s, _)| *s == sw.skill).map(|(_, x)| *x).filter(|x| *x != 0.0 && !x.is_nan());
        row("", &sk.name, parts.join("\n\n"), value.map_or(String::new(), |x| f(x, 0)));
    }
    row("", "スキルエナジー", format!("{} × {}", f(p.skill_energy, 0), f(p.skill_per_day, 2)), f(p.skill_energy_per_day, 1));

    let otetsudai_bonus = if has("おてつだいボーナス") {
        let thb = se.team_help_bonus;
        let base = se.team_help_bonus_base;
        let bonus = (p.b_ep_d + p.f_ep_d + p.skill_energy_per_day)
            * (1.0 - p.speed_bonus)
            * (1.0 / (1.0 - ((thb + 1.0) * 0.05 + base / 100.0).min(0.35)) - 1.0 / (1.0 - base / 100.0))
            / (thb + 1.0)
            * 4.0;
        row(
            "",
            "おてつだいボーナス",
            format!(
                concat!(
                    "({} + {} + {}) ※きのみ、食材、スキルの合計\n",
                    "× {}% ※おてつだいスピード補正を除いて素の速度のスコアにする\n",
                    "× (\n",
                    "　100% ÷ (100% - min({}% + {}%, 35%)) ※おてボ{}匹編成時の効率アップ率\n",
                    "　- 100% ÷ (100% - {}%) ※おてボなしの値を引いて増分だけにする\n",
                    ")\n",
                    "÷ {}匹 ※おてボ編成数で割って1匹あたりの効果量にする\n",
                    "× 4 ※自分以外の4匹分",
                ),
                f(p.b_ep_d, 1),
                f(p.f_ep_d, 1),
                f(p.skill_energy_per_day, 1),
                f((1.0 - p.speed_bonus) * 100.0, 1),
                n((thb + 1.0) * 5.0),
                n(base),
                n(thb + 1.0),
                n(base),
                n(thb + 1.0),
            ),
            f(bonus, 1),
        );
        bonus
    } else {
        0.0
    };

    let subtotal = p.b_ep_d + p.f_ep_d + p.skill_energy_per_day + otetsudai_bonus;
    let mut subtotal_formula = format!("{} + {} + {}", f(p.b_ep_d, 1), f(p.f_ep_d, 1), f(p.skill_energy_per_day, 1));
    if otetsudai_bonus > 0.0 {
        subtotal_formula += &format!(" + {}", f(otetsudai_bonus, 1));
    }
    row("小計", "", subtotal_formula, f(subtotal, 1));
    let mut s = f(subtotal, 1);
    if has("ゆめのかけらボーナス") {
        s += &format!("\n× (100% + 6% × {}%) ※ゆめのかけらボーナス", n(se.shard_bonus));
    }
    if has("リサーチEXPボーナス") {
        s += &format!("\n× (100% + 9% × {}% ÷ 2) ※リサーチEXPボーナス", n(se.shard_bonus));
    }
    if p.shard != 0.0 && !p.shard.is_nan() {
        s += &format!("\n+ {} × {} × {}% ※ゆめのかけらゲット", f(p.shard, 1), n(se.shard_energy), n(se.shard_bonus));
    }
    row("総計", "", s, f(c.score, 0));

    egui::Grid::new("evaluate_detail_grid").num_columns(4).striped(true).spacing([14.0, 8.0]).show(ui, |ui| {
        for (group, item, formula, value) in &rows {
            ui.label(egui::RichText::new(group).strong().color(style::th_color()));
            ui.label(egui::RichText::new(item).strong().color(style::th_color()));
            ui.scope(|ui| {
                ui.set_max_width(460.0);
                if item == "せいかく" && nature.is_some() {
                    ui.horizontal(|ui| nature_label(ui, formula));
                } else {
                    ui.label(formula);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.label(egui::RichText::new(value).strong());
            });
            ui.end_row();
        }
    });
}
