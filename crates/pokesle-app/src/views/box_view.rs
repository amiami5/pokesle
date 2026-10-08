//! ボックス画面: 一覧・追加/編集・TSV取り込み/書き出し・概算

use std::cmp::Ordering;
use std::collections::HashMap;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use pokesle_core::box_io::{self, TsvColumns};
use pokesle_core::box_sim::BoxResult;
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::SimPokemon;
use pokesle_core::evaluate_table::EvalLv;
use pokesle_core::team_sim::simulate_box;

use super::pokemon_edit::{EditResult, PokemonEdit};
use super::{common_setting, fmt_fixed, fmt_num, food_icon, nature_label, style};
use crate::app::Shared;
use crate::job::Job;

/// サブスキルの解放レベル(本家 pokemon-simulator.ts)
const SUB_SKILL_LV: [u32; 5] = [10, 25, 50, 70, 80];

/// 数値の表示形式(本家 sortable-table.vue)
#[derive(Clone, Copy)]
enum Fmt {
    /// 小数点以下の桁数
    Num(usize),
    /// ×100 して % 表示(小数点以下の桁数)
    Percent(usize),
}

/// 「シミュ詳細」の列(本家 index.vue の `config.pokemonList.simulatedInfo`)
const DETAIL_COLUMNS: [(&str, &str, Fmt, fn(&SimPokemon) -> f64); 25] = [
    ("speed", "おてつだい
時間", Fmt::Num(0), |p| p.speed),
    ("dayHelpNum", "日中おてつ
だい回数", Fmt::Num(2), |p| p.day_help_num),
    ("nightHelpNum", "夜間おてつ
だい回数", Fmt::Num(2), |p| p.night_help_num),
    ("berryNum", "きのみ
個数", Fmt::Num(0), |p| p.berry_num),
    ("bEpH", "きのみ
/手伝い", Fmt::Num(1), |p| p.b_ep_h),
    ("bEpD", "きのみ
/日", Fmt::Num(1), |p| p.b_ep_d),
    ("foodRate", "食材
率", Fmt::Percent(1), |p| p.food_rate),
    ("foodNum", "食材
個数", Fmt::Num(1), |p| p.food_num),
    ("fEpH", "食材
/手伝い", Fmt::Num(1), |p| p.f_ep_h),
    ("fEpD", "食材
/日", Fmt::Num(1), |p| p.f_ep_d),
    ("pickupEnergyPerHelp", "きのみ+食材
/手伝い", Fmt::Num(1), |p| p.pickup_energy_per_help),
    ("pickupEnergyPerDay", "きのみ+食材
/日", Fmt::Num(1), |p| p.pickup_energy_per_day),
    ("bag", "所持
数", Fmt::Num(0), |p| p.bag),
    ("bagFullHelpNum", "いつ育
到達回数", Fmt::Num(1), |p| p.bag_full_help_num),
    ("normalHelpNum", "通常おてつ
だい回数", Fmt::Num(2), |p| p.normal_help_num),
    ("berryHelpNum", "いつ育おてつ
だい回数", Fmt::Num(2), |p| p.berry_help_num),
    ("fixedSkillLv", "補正後
スキルLv", Fmt::Num(0), |p| p.fixed_skill_lv),
    ("skillRate", "スキル確率", Fmt::Percent(2), |p| p.skill_rate),
    ("ceilSkillRate", "天井補正
スキル確率", Fmt::Percent(2), |p| p.ceil_skill_rate),
    ("skillPerDay", "スキル
回数/日", Fmt::Num(2), |p| p.skill_per_day),
    ("skillEnergyPerDay", "スキル
エナジー/日", Fmt::Num(1), |p| p.skill_energy_per_day),
    ("shard", "スキル
ゆめかけ/日", Fmt::Num(1), |p| p.shard),
    ("energyPerDay", "エナジー
/日", Fmt::Num(1), |p| p.energy_per_day),
    ("supportEnergyPerDay", "サポート
エナジー/日", Fmt::Num(1), |p| p.support_energy_per_day),
    ("supportShardPerDay", "サポート
ゆめかけ/日", Fmt::Num(1), |p| p.support_shard_per_day),
];

/// 列見出しの説明(本家 index.vue の help-button)
fn detail_help(title: &str) -> Option<&'static str> {
    match title {
        "エナジー
/日" => Some("おてつだいボーナスや他ポケモンに影響を与えるスキルの効果を抜きにして、自分自身だけで稼げるエナジーです。"),
        "サポート
エナジー/日" => Some("おてつだいボーナスやげんき回復系スキル、おてつだいサポートなどによる他ポケモンの効率上昇によって、追加で得られるであろうエナジーです。"),
        "サポート
ゆめかけ/日" => Some("おてつだいボーナスやげんき回復系スキルによる他ポケモンの効率上昇、もしくはゆめのかけらボーナスやリサーチEXPボーナスによって、得られるであろうゆめのかけらの量です。"),
        _ => None,
    }
}

fn fmt_value(v: f64, fmt: Fmt) -> String {
    match fmt {
        Fmt::Num(d) => fmt_fixed(v, d),
        Fmt::Percent(d) => format!("{:.*}%", d, v * 100.0),
    }
}

/// キーワード絞り込み(名前・食材・スキル・サブスキル・性格・メモ)
fn keyword_match(p: &pokesle_core::sim_pokemon::BoxPokemon, query: &str) -> bool {
    let m = master();
    let skill = m.pokemon(&p.name).map(|b| m.pokemon_skill(b).name.as_str()).unwrap_or("");
    p.name.contains(query)
        || p.food_list.iter().any(|x| x.contains(query))
        || skill.contains(query)
        || p.sub_skill_list.iter().any(|x| x.contains(query))
        || p.nature.contains(query)
        || p.memo.as_deref().is_some_and(|x| x.contains(query))
}

/// 概算をやり直す必要があるかの判定に使う指紋
fn simulate_key(shared: &Shared) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut v = shared.config.to_value();
    if let Some(m) = v.as_object_mut() {
        // 表示だけの設定は除く
        m.remove("pokemonList");
        m.remove("summary");
    }
    v.to_string().hash(&mut h);
    serde_json::to_string(&shared.box_file.list).unwrap_or_default().hash(&mut h);
    if let Some(t) = shared.usable_evaluate_table() {
        format!("{:?}{:?}", t.healer, t.support).hash(&mut h);
        t.pokemon.len().hash(&mut h);
    }
    h.finish()
}

/// 設定 `pokemonList.simulatedInfo`(シミュ詳細)
fn simulated_info(shared: &Shared) -> bool {
    shared.config.extra.get("pokemonList").and_then(|v| v.get("simulatedInfo")).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn set_simulated_info(shared: &mut Shared, on: bool) {
    let list = shared.config.extra.entry("pokemonList").or_insert_with(|| serde_json::json!({}));
    if let Some(map) = list.as_object_mut() {
        map.insert("simulatedInfo".into(), on.into());
    }
    shared.save_config();
}

/// 並べ替えに使う値(本家 sortable-table.vue の getSortKey)
#[derive(PartialEq, PartialOrd)]
enum SortValue {
    Num(f64),
    Str(String),
}

/// 文字列の並べ替え用に濁点・半濁点を外し、長音を直前の母音に置き換える(本家 convertKatakanaForSort)
fn kana_sort_key(text: &str) -> String {
    const VOICED: &str = "ガギグゲゴザジズゼゾダヂヅデドバビブベボパピプペポヴがぎぐげござじずぜぞだぢづでどばびぶべぼぱぴぷぺぽゔ";
    const PLAIN: &str = "カキクケコサシスセソタチツテトハヒフヘホハヒフヘホウかきくけこさしすせそたちつてとはひふへほはひふへほう";
    const VOWELS: [(&str, char); 5] = [
        ("アァカガサザタダナハバパマヤャラワヮヵ", 'ア'),
        ("イィキギシジチヂニヒビピミリヰ", 'イ'),
        ("ウゥクグスズツヅヌフブプムユュルヴ", 'ウ'),
        ("エェケゲセゼテデネヘベペメレヱヶ", 'エ'),
        ("オォコゴソゾトドノホボポモヨョロヲ", 'オ'),
    ];
    let mut result = String::new();
    for c in text.chars() {
        let c = VOICED.chars().position(|v| v == c).and_then(|i| PLAIN.chars().nth(i)).unwrap_or(c);
        if c == 'ー'
            && let Some(prev) = result.chars().last()
            && let Some((_, v)) = VOWELS.iter().find(|(list, _)| list.contains(prev))
        {
            result.push(*v);
        } else {
            result.push(c);
        }
    }
    result
}

/// 設定 `sortableTable.pokemonList2.sort`(本家と同じ場所): 並べ替えの条件(列のキー, 1=昇順 / -1=降順)
fn sort_info(shared: &Shared) -> Vec<(String, i8)> {
    let list = shared.config.extra.get("sortableTable").and_then(|v| v.get("pokemonList2")).and_then(|v| v.get("sort")).and_then(|v| v.as_array());
    list.into_iter()
        .flatten()
        .filter_map(|x| Some((x.get("key")?.as_str()?.to_string(), if x.get("direction")?.as_i64()? < 0 { -1 } else { 1 })))
        .collect()
}

fn set_sort_info(shared: &mut Shared, sort: &[(String, i8)]) {
    let table = shared.config.extra.entry("sortableTable").or_insert_with(|| serde_json::json!({}));
    if let Some(table) = table.as_object_mut()
        && let Some(map) = table.entry("pokemonList2").or_insert_with(|| serde_json::json!({ "hiddenColumn": [] })).as_object_mut()
    {
        let value = sort.iter().map(|(key, dir)| serde_json::json!({ "key": key, "direction": dir })).collect();
        map.insert("sort".into(), serde_json::Value::Array(value));
    }
    shared.save_config();
}

/// 見出しのクリックで並べ替えを切り替える(本家 setSort)
/// クリック: 昇順 → 降順 → 解除。Shift+クリック: 並べ替えの条件を後ろに追加する
fn toggle_sort(sort: &mut Vec<(String, i8)>, key: &str, shift: bool) {
    if shift {
        match sort.last_mut() {
            Some((k, dir)) if k == key => {
                if *dir == 1 {
                    *dir = -1;
                } else {
                    sort.pop();
                }
            }
            _ => {
                sort.retain(|(k, _)| k != key);
                sort.push((key.to_string(), 1));
            }
        }
    } else {
        match sort.first() {
            Some((k, 1)) if k == key => *sort = vec![(key.to_string(), -1)],
            Some((k, _)) if k == key => sort.clear(),
            _ => *sort = vec![(key.to_string(), 1)],
        }
    }
}

/// ボックス一覧の列(編集・削除の列を除く)
#[derive(Clone, Copy, PartialEq)]
enum Col {
    Index,
    Name,
    Lv,
    SkillLv,
    Food,
    Sub(usize),
    Nature,
    Score,
    Evaluate,
    Detail(usize),
}

impl Col {
    /// 最初の並び
    fn all() -> Vec<Col> {
        let mut list = vec![Col::Index, Col::Name, Col::Lv, Col::SkillLv, Col::Food];
        list.extend((0..SUB_SKILL_LV.len()).map(Col::Sub));
        list.extend([Col::Nature, Col::Score, Col::Evaluate]);
        list.extend((0..DETAIL_COLUMNS.len()).map(Col::Detail));
        list
    }

    /// 並べ替え・列の並びの保存に使うキー
    fn key(self) -> String {
        match self {
            Col::Index => "index".into(),
            Col::Name => "name".into(),
            Col::Lv => "lv".into(),
            Col::SkillLv => "skillLv".into(),
            Col::Food => "foodNameList".into(),
            Col::Sub(slot) => format!("subSkill{}", SUB_SKILL_LV[slot]),
            Col::Nature => "natureName".into(),
            Col::Score => "score".into(),
            Col::Evaluate => "evaluate".into(),
            Col::Detail(n) => DETAIL_COLUMNS[n].0.into(),
        }
    }

    fn title(self) -> String {
        match self {
            Col::Index => "No".into(),
            Col::Name => "ポケモン".into(),
            Col::Lv => "Lv".into(),
            Col::SkillLv => "スキルLv".into(),
            Col::Food => "食材".into(),
            Col::Sub(slot) => format!("サブ{}", SUB_SKILL_LV[slot]),
            Col::Nature => "せいかく".into(),
            Col::Score => "スコア".into(),
            Col::Evaluate => "厳選".into(),
            Col::Detail(n) => DETAIL_COLUMNS[n].1.into(),
        }
    }

    /// 列の幅(width は覚えている幅。無ければ最初の幅)
    fn width(self, width: Option<f32>) -> Column {
        let (initial, min) = match self {
            Col::Index => (36.0, 24.0),
            Col::Name => (150.0, 80.0),
            Col::Lv => (40.0, 24.0),
            Col::SkillLv => (72.0, 24.0),
            Col::Food => (84.0, 40.0),
            Col::Sub(_) => (72.0, 32.0),
            Col::Nature => (80.0, 24.0),
            Col::Score => (90.0, 24.0),
            Col::Evaluate => (70.0, 24.0),
            Col::Detail(_) => (84.0, 32.0),
        };
        Column::initial(width.unwrap_or(initial).max(min)).at_least(min)
    }
}

/// 列の並びと幅の保存場所(本家に無い項目なので、設定ではなくウインドウ位置と同じ window.ron に入れる)
fn column_order_id() -> egui::Id {
    egui::Id::new("box_column_order")
}

fn column_width_id() -> egui::Id {
    egui::Id::new("box_column_width")
}

/// 保存された列の並び(保存に無い列は最初の並びのまま後ろに付ける)
fn column_order(ctx: &egui::Context) -> Vec<Col> {
    let all = Col::all();
    let saved: Vec<String> = ctx.data_mut(|d| d.get_persisted(column_order_id())).unwrap_or_default();
    let mut order: Vec<Col> = saved.iter().filter_map(|k| all.iter().copied().find(|c| c.key() == *k)).collect();
    for c in all {
        if !order.contains(&c) {
            order.push(c);
        }
    }
    order
}

/// 列の並びを保存する(None は最初の並びに戻す)
fn set_column_order(ctx: &egui::Context, order: Option<&[Col]>) {
    match order {
        Some(order) => ctx.data_mut(|d| d.insert_persisted(column_order_id(), order.iter().map(|c| c.key()).collect::<Vec<_>>())),
        None => ctx.data_mut(|d| d.remove::<Vec<String>>(column_order_id())),
    }
}

/// 列のキー → 幅
fn column_widths(ctx: &egui::Context) -> HashMap<String, f32> {
    ctx.data_mut(|d| d.get_persisted(column_width_id())).unwrap_or_default()
}

fn set_column_widths(ctx: &egui::Context, widths: HashMap<String, f32>) {
    ctx.data_mut(|d| d.insert_persisted(column_width_id(), widths));
}

#[derive(Default)]
pub struct BoxView {
    search: String,
    edit: Option<PokemonEdit>,
    import_open: bool,
    import_text: String,
    import_message: String,
    export_text: String,
    simulate_job: Option<Job<Vec<BoxResult>>>,
    /// 概算に使った設定・ボックス・厳選テーブルの指紋
    simulate_key: Option<u64>,
    /// ボックス位置 → 概算結果(いつ育モードを除く)
    results: Vec<Option<BoxResult>>,
    error: String,
}

impl BoxView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        self.poll_jobs(shared);

        // 1段目: 共通設定と表示設定(本家 index.vue)
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            changed |= common_setting::ui(ui, &mut shared.config);
            let mut detail = simulated_info(shared);
            style::setting(ui, "box_display", "表示設定", &style::plain("表示設定"), false, |ui| {
                egui::Grid::new("box_display_grid").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                    style::th(ui, "シミュ詳細");
                    if ui.checkbox(&mut detail, "シミュ詳細").changed() {
                        set_simulated_info(shared, detail);
                    }
                    ui.end_row();
                });
            });
        });
        if changed {
            shared.save_config();
        }
        // 2段目: キーワード
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            style::th(ui, "キーワード");
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("名前、食材名、スキル名など").desired_width(200.0));
        });

        // 設定・ボックス・厳選テーブルが変わったら概算し直す(本家は自動で再計算する)
        let key = simulate_key(shared);
        if self.simulate_job.is_none() && self.simulate_key != Some(key) {
            self.simulate_key = Some(key);
            if shared.box_file.list.is_empty() {
                self.results.clear();
            } else {
                let config = shared.config.clone();
                let list = shared.box_file.list.clone();
                let table = shared.usable_evaluate_table().cloned();
                self.simulate_job = Some(Job::spawn(ui.ctx(), move |p| {
                    p.set(0.0, "ボックスを概算しています…");
                    simulate_box(&config, &list, table.as_ref(), true)
                }));
            }
        }
        if let Some(job) = &self.simulate_job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        ui.add_space(5.0);

        // 下段: 追加・TSV
        egui::Panel::bottom("box_actions").frame(egui::Frame::new().inner_margin(egui::Margin { top: 6, ..Default::default() })).show(ui, |ui| {
            ui.horizontal(|ui| {
                if style::form_button(ui, "ポケモン新規追加", style::ButtonKind::Execute).clicked() {
                    self.edit = Some(PokemonEdit::new(None, None));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if style::form_button(ui, "TSVインポート/エクスポート", style::ButtonKind::Normal).clicked() {
                        self.import_open = true;
                    }
                });
            });
        });

        self.table(ui, shared);
        self.import_window(ui.ctx(), shared);

        if let Some(edit) = &mut self.edit {
            match edit.ui(ui.ctx()) {
                EditResult::None => {}
                EditResult::Cancel => self.edit = None,
                EditResult::Save(index, pokemon) => {
                    match index {
                        Some(i) if i < shared.box_file.list.len() => shared.box_file.list[i] = pokemon,
                        _ => shared.box_file.list.push(pokemon),
                    }
                    shared.save_box();
                    self.results.clear();
                    self.edit = None;
                }
            }
        }
    }

    fn poll_jobs(&mut self, shared: &mut Shared) {
        if let Some(job) = &mut self.simulate_job
            && let Some(r) = job.poll()
        {
            self.simulate_job = None;
            match r {
                Ok(list) => {
                    self.results = vec![None; shared.box_file.list.len()];
                    for r in list {
                        if r.pokemon.bag_over_operation {
                            continue;
                        }
                        if let Some(i) = r.pokemon.box_index()
                            && let Some(slot) = self.results.get_mut(i)
                            && slot.as_ref().is_none_or(|x| x.pokemon.score < r.pokemon.score)
                        {
                            *slot = Some(r);
                        }
                    }
                    self.error.clear();
                }
                Err(e) => self.error = e,
            }
        }
    }

    fn table(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let m = master();
        let query = self.search.trim().to_string();
        let mut rows: Vec<usize> = (0..shared.box_file.list.len())
            .filter(|&i| query.is_empty() || keyword_match(&shared.box_file.list[i], &query))
            .collect();
        let mut sort = sort_info(shared);
        if !sort.is_empty() {
            let keys: Vec<Vec<SortValue>> = rows.iter().map(|&i| sort.iter().map(|(k, _)| self.sort_value(shared, i, k)).collect()).collect();
            // 本家と同じく安定ソート(同じ値は元の順のまま)
            let mut order: Vec<usize> = (0..rows.len()).collect();
            order.sort_by(|&a, &b| {
                for (n, (_, dir)) in sort.iter().enumerate() {
                    if keys[a][n] > keys[b][n] {
                        return if *dir > 0 { Ordering::Greater } else { Ordering::Less };
                    }
                    if keys[a][n] < keys[b][n] {
                        return if *dir > 0 { Ordering::Less } else { Ordering::Greater };
                    }
                }
                Ordering::Equal
            });
            rows = order.into_iter().map(|n| rows[n]).collect();
        }
        let mut sort_changed = false;
        let mut action: Option<(usize, bool)> = None; // (index, delete?)
        let detail = simulated_info(shared);
        let mut order = column_order(ui.ctx());
        let columns: Vec<Col> = order.iter().copied().filter(|c| detail || !matches!(c, Col::Detail(_))).collect();
        // 列の入れ替え(動かす列, 入れる先の列, 先の列の後ろか)、並びを最初に戻す
        let mut moved: Option<(Col, Col, bool)> = None;
        let mut reset_order = false;
        // 表は列の幅を位置ごとに覚えるので、並びが変わったら別の表として作り直し、
        // 測っておいた各列の幅を最初の幅として渡す
        let mut widths = column_widths(ui.ctx());
        let order_salt: Vec<String> = columns.iter().map(|c| c.key()).collect();

        // 列が画面に収まらないときは横にスクロールする
        egui::ScrollArea::horizontal().show(ui, |ui| {
            let mut table = TableBuilder::new(ui)
                .id_salt(("box_table", order_salt))
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            for c in &columns {
                table = table.column(c.width(widths.get(&c.key()).copied()));
            }
            table
                .column(Column::remainder())
                // シミュ詳細の見出しは2行
                .header(if detail { 40.0 } else { 22.0 }, |mut header| {
                    for &col in &columns {
                        // header.col が返すのは中身の大きさなので、マスの幅はマスの中で測る
                        let mut cell_width = None;
                        header.col(|ui| {
                            cell_width = Some(ui.max_rect().width());
                            let key = col.key();
                            let title = col.title();
                            // 並べ替え中の列には ▲(昇順)/▼(降順) と、条件が複数なら優先順を付ける
                            let mark = match sort.iter().position(|(k, _)| *k == key) {
                                Some(n) => {
                                    let arrow = if sort[n].1 > 0 { "▲" } else { "▼" };
                                    if sort.len() > 1 { format!("{arrow}{}", n + 1) } else { arrow.to_string() }
                                }
                                None => String::new(),
                            };
                            ui.add(egui::Label::new(egui::RichText::new(format!("{title}{mark}")).strong()).selectable(false));
                            // 文字の横の空いたところも含めて、見出しのマス全体をクリック・ドラッグできるようにする
                            let rect = ui.max_rect();
                            let mut r = ui.interact(rect, egui::Id::new(("box_sort", &key)), egui::Sense::click_and_drag());
                            r = if r.dragged() { r.on_hover_cursor(egui::CursorIcon::Grabbing) } else { r.on_hover_cursor(egui::CursorIcon::PointingHand) };
                            if let Some(help) = detail_help(&title) {
                                r = r.on_hover_text(help);
                            }
                            r = r.on_hover_text("クリックで並べ替え(昇順→降順→解除)\nShift+クリックで条件を追加\nドラッグで列を入れ替え");
                            if r.clicked() {
                                toggle_sort(&mut sort, &key, ui.input(|i| i.modifiers.shift));
                                sort_changed = true;
                            }
                            r.context_menu(|ui| {
                                if ui.button("列の並びを最初に戻す").clicked() {
                                    reset_order = true;
                                    ui.close();
                                }
                            });

                            // ドラッグ中は列名をマウスの横に出す
                            r.dnd_set_drag_payload(col);
                            if r.dragged()
                                && let Some(pos) = ui.ctx().pointer_interact_pos()
                            {
                                let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("box_col_drag")));
                                let galley = painter.layout_no_wrap(title.replace('\n', ""), egui::TextStyle::Body.resolve(ui.style()), ui.visuals().strong_text_color());
                                let text_rect = egui::Rect::from_min_size(pos + egui::vec2(12.0, 8.0), galley.size()).expand(4.0);
                                painter.rect_filled(text_rect, 4.0, ui.visuals().selection.bg_fill);
                                painter.galley(text_rect.min + egui::vec2(4.0, 4.0), galley, ui.visuals().strong_text_color());
                            }
                            // 落とす先: マスの右半分なら後ろ、左半分なら前に入れる線を出す
                            if let Some(pos) = ui.ctx().pointer_interact_pos()
                                && rect.contains(pos)
                                && let Some(src) = egui::DragAndDrop::payload::<Col>(ui.ctx())
                                && *src != col
                            {
                                let after = pos.x > rect.center().x;
                                let x = if after { rect.right() } else { rect.left() };
                                ui.painter().with_clip_rect(rect.expand(4.0)).vline(x, rect.y_range(), egui::Stroke::new(3.0, ui.visuals().selection.stroke.color));
                                if ui.input(|i| i.pointer.any_released())
                                    && let Some(src) = egui::DragAndDrop::take_payload::<Col>(ui.ctx())
                                {
                                    moved = Some((*src, col, after));
                                }
                            }
                        });
                        if let Some(w) = cell_width {
                            widths.insert(col.key(), w);
                        }
                    }
                    header.col(|_| {});
                })
                .body(|body| {
                    body.rows(26.0, rows.len(), |mut row| {
                        let i = rows[row.index()];
                        let p = &shared.box_file.list[i];
                        let result = self.results.get(i).and_then(|r| r.as_ref());
                        for &col in &columns {
                            row.col(|ui| match col {
                                Col::Index => {
                                    ui.label(format!("{}", i + 1));
                                }
                                Col::Name => {
                                    let mark = if p.fix == Some(1.0) { "📌" } else { "" };
                                    ui.label(format!("{mark}{}", p.name));
                                }
                                Col::Lv => {
                                    ui.label(format!("{}", p.lv));
                                }
                                Col::SkillLv => {
                                    ui.label(p.skill_lv.map(|x| x.to_string()).unwrap_or_default());
                                }
                                Col::Food => food_icon::food_list(ui, p),
                                Col::Sub(slot) => {
                                    let Some(name) = p.sub_skill_list.get(slot) else { return };
                                    let short = m.sub_skill(name).map(|x| x.short.as_str()).unwrap_or("?");
                                    // まだ解放されていないサブスキルは薄く表示する
                                    let text = if p.lv >= SUB_SKILL_LV[slot] as f64 { egui::RichText::new(short) } else { egui::RichText::new(short).weak() };
                                    ui.label(text).on_hover_text(name);
                                }
                                Col::Nature => nature_label(ui, &p.nature),
                                Col::Score => {
                                    if let Some(r) = result {
                                        ui.label(fmt_num(r.pokemon.score));
                                    }
                                }
                                Col::Evaluate => {
                                    let best = result
                                        .and_then(|r| r.evaluate_result.as_ref())
                                        .and_then(|e| e.best.get(&EvalLv::Max))
                                        .and_then(|b| b.energy.as_ref());
                                    if let Some(b) = best {
                                        ui.label(format!("{:.0}%", b.score * 100.0)).on_hover_text(b.name.clone().unwrap_or_default());
                                    }
                                }
                                Col::Detail(n) => {
                                    let (_, _, fmt, get) = DETAIL_COLUMNS[n];
                                    if let Some(r) = result {
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(fmt_value(get(&r.pokemon), fmt));
                                        });
                                    }
                                }
                            });
                        }
                        row.col(|ui| {
                            if ui.small_button("編集").clicked() {
                                action = Some((i, false));
                            }
                            if ui.small_button("削除").clicked() {
                                action = Some((i, true));
                            }
                        });
                    });
                });
        });

        if let Some((src, dst, after)) = moved {
            order.retain(|c| *c != src);
            if let Some(n) = order.iter().position(|c| *c == dst) {
                order.insert(if after { n + 1 } else { n }, src);
                set_column_order(ui.ctx(), Some(&order));
            }
        }
        if reset_order {
            set_column_order(ui.ctx(), None);
            widths.clear();
        }
        set_column_widths(ui.ctx(), widths);
        if sort_changed {
            set_sort_info(shared, &sort);
        }

        match action {
            Some((i, false)) => self.edit = Some(PokemonEdit::new(Some(i), Some(shared.box_file.list[i].clone()))),
            Some((i, true)) => {
                shared.box_file.list.remove(i);
                shared.save_box();
                self.results.clear();
            }
            None => {}
        }
    }

    /// 並べ替えの値。数値が無いときは 0(本家 getSortKey の `value || 0`)
    fn sort_value(&self, shared: &Shared, i: usize, key: &str) -> SortValue {
        let m = master();
        let p = &shared.box_file.list[i];
        let result = self.results.get(i).and_then(|r| r.as_ref());
        let num = |v: Option<f64>| SortValue::Num(v.filter(|x| !x.is_nan()).unwrap_or(0.0));
        let text = |v: &str| SortValue::Str(kana_sort_key(v));
        match key {
            "index" => num(Some(i as f64)),
            "name" => text(&p.name),
            "lv" => num(Some(p.lv)),
            "skillLv" => num(p.skill_lv),
            "foodNameList" => text(&p.food_list.join(",")),
            "natureName" => text(&p.nature),
            "score" => num(result.map(|r| r.pokemon.score)),
            "evaluate" => num(result
                .and_then(|r| r.evaluate_result.as_ref())
                .and_then(|e| e.best.get(&EvalLv::Max))
                .and_then(|b| b.energy.as_ref())
                .map(|b| b.score)),
            _ => {
                if let Some(slot) = SUB_SKILL_LV.iter().position(|lv| key == format!("subSkill{lv}")) {
                    let name = p.sub_skill_list.get(slot).map(|n| m.sub_skill(n).map(|x| x.short.as_str()).unwrap_or(n)).unwrap_or("");
                    return text(name);
                }
                match DETAIL_COLUMNS.iter().find(|(k, _, _, _)| *k == key) {
                    Some((_, _, _, get)) => num(result.map(|r| get(&r.pokemon))),
                    None => num(None),
                }
            }
        }
    }

    fn import_window(&mut self, ctx: &egui::Context, shared: &mut Shared) {
        if !self.import_open {
            return;
        }
        let mut open = true;
        egui::Window::new("TSVの取り込み・書き出し").id(egui::Id::new("tsv_window")).open(&mut open).default_width(900.0).show(ctx, |ui| {
            let mut cols = TsvColumns::from_config(&shared.config);
            egui::CollapsingHeader::new("列番号設定").default_open(true).show(ui, |ui| {
                ui.label("列番号を空にすると取り込み・書き出しの対象外になります。取り込み時はその情報が欠けるので注意してください。");
                if tsv_columns_ui(ui, &mut cols) {
                    cols.save_to_config(&mut shared.config);
                    shared.save_config();
                }
            });
            ui.separator();

            ui.columns(2, |c| {
                let ui = &mut c[0];
                ui.horizontal(|ui| {
                    if ui.button("書き出す").clicked() {
                        self.export_text = box_io::export_tsv(&shared.box_file.list, &cols);
                    }
                });
                egui::ScrollArea::vertical().id_salt("export").max_height(360.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.export_text).desired_width(f32::INFINITY).desired_rows(16).code_editor());
                });

                let ui = &mut c[1];
                // 貼り付けた内容を今の列設定で読んだときの件数
                let lines = self.import_text.lines().filter(|l| !l.trim().is_empty()).count();
                let readable = if lines == 0 { 0 } else { box_io::import_tsv(&self.import_text, &cols).len() };
                ui.horizontal(|ui| {
                    if ui.add_enabled(readable > 0, egui::Button::new("取り込む")).on_hover_text("今のボックスは置き換えられます").clicked() {
                        let list = box_io::import_tsv(&self.import_text, &cols);
                        self.import_message = format!("{}匹を取り込みました", list.len());
                        shared.box_file.list = list;
                        shared.save_box();
                        self.results.clear();
                        self.import_text.clear();
                    } else if lines > 0 {
                        let text = format!("{lines}行中 {readable}匹を読み込めます");
                        if readable < lines {
                            ui.colored_label(ui.visuals().warn_fg_color, text).on_hover_text("読めない行は名前・Lv・食材・サブスキル・せいかくを確認するか、列番号設定を見直してください");
                        } else {
                            ui.label(text);
                        }
                    } else {
                        ui.label(&self.import_message);
                    }
                });
                egui::ScrollArea::vertical().id_salt("import").max_height(360.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.import_text).hint_text("スプレッドシート等からコピーした TSV を貼り付け").desired_width(f32::INFINITY).desired_rows(16).code_editor());
                });
            });
        });
        if !open {
            self.import_open = false;
        }
    }
}

/// 列番号の入力欄(本家と同じ項目)。変更があれば true
fn tsv_columns_ui(ui: &mut egui::Ui, cols: &mut TsvColumns) -> bool {
    cols.food_list.resize(3, None);
    cols.sub_skill_list.resize(5, None);
    let [f1, f2, f3] = &mut cols.food_list[..] else { unreachable!() };
    let [s1, s2, s3, s4, s5] = &mut cols.sub_skill_list[..] else { unreachable!() };
    let fields: [(&str, &mut Option<usize>); 20] = [
        ("名前", &mut cols.name),
        ("Lv", &mut cols.lv),
        ("スキルLv", &mut cols.skill_lv),
        ("食材ABC", &mut cols.food_abc),
        ("食材1", f1),
        ("食材2", f2),
        ("食材3", f3),
        ("サブスキル1", s1),
        ("サブスキル2", s2),
        ("サブスキル3", s3),
        ("サブスキル4", s4),
        ("サブスキル5", s5),
        ("せいかく", &mut cols.nature),
        ("色違い", &mut cols.shiny),
        ("固定", &mut cols.fix),
        ("睡眠時間", &mut cols.sleep_time),
        ("目標Lv", &mut cols.training),
        ("次Lv迄のExp", &mut cols.next_exp),
        ("メモ", &mut cols.memo),
        ("お気に入り", &mut cols.favorite),
    ];
    let mut changed = false;
    egui::Grid::new("tsv_columns").num_columns(10).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, (label, value)) in fields.into_iter().enumerate() {
            ui.label(label);
            let mut text = value.map(|v| v.to_string()).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut text).hint_text("なし").desired_width(36.0)).changed() {
                let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
                *value = digits.parse().ok().filter(|&v| v > 0);
                changed = true;
            }
            if i % 5 == 4 {
                ui.end_row();
            }
        }
    });
    changed
}
