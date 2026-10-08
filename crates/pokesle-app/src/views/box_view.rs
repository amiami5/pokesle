//! ボックス画面: 一覧・追加/編集・TSV取り込み/書き出し・概算

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use pokesle_core::box_io::{self, TsvColumns};
use pokesle_core::box_sim::BoxResult;
use pokesle_core::check_list::CheckHit;
use pokesle_core::data::{master, Pokemon};
use pokesle_core::evaluate_table::EvalLv;
use pokesle_core::exp::{calc_require_info, RequireInfo};
use pokesle_core::js::to_fixed;
use pokesle_core::sim_pokemon::{BoxPokemon, SimPokemon};
use pokesle_core::team_sim::simulate_box;
use serde::{Deserialize, Serialize};

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
    ("speed", "おてつだい\n時間", Fmt::Num(0), |p| p.speed),
    ("dayHelpNum", "日中おてつ\nだい回数", Fmt::Num(2), |p| p.day_help_num),
    ("nightHelpNum", "夜間おてつ\nだい回数", Fmt::Num(2), |p| p.night_help_num),
    ("berryNum", "きのみ\n個数", Fmt::Num(0), |p| p.berry_num),
    ("bEpH", "きのみ\n/手伝い", Fmt::Num(1), |p| p.b_ep_h),
    ("bEpD", "きのみ\n/日", Fmt::Num(1), |p| p.b_ep_d),
    ("foodRate", "食材\n率", Fmt::Percent(1), |p| p.food_rate),
    ("foodNum", "食材\n個数", Fmt::Num(1), |p| p.food_num),
    ("fEpH", "食材\n/手伝い", Fmt::Num(1), |p| p.f_ep_h),
    ("fEpD", "食材\n/日", Fmt::Num(1), |p| p.f_ep_d),
    ("pickupEnergyPerHelp", "きのみ+食材\n/手伝い", Fmt::Num(1), |p| p.pickup_energy_per_help),
    ("pickupEnergyPerDay", "きのみ+食材\n/日", Fmt::Num(1), |p| p.pickup_energy_per_day),
    ("bag", "所持\n数", Fmt::Num(0), |p| p.bag),
    ("bagFullHelpNum", "いつ育\n到達回数", Fmt::Num(1), |p| p.bag_full_help_num),
    ("normalHelpNum", "通常おてつ\nだい回数", Fmt::Num(2), |p| p.normal_help_num),
    ("berryHelpNum", "いつ育おてつ\nだい回数", Fmt::Num(2), |p| p.berry_help_num),
    ("fixedSkillLv", "補正後\nスキルLv", Fmt::Num(0), |p| p.fixed_skill_lv),
    ("skillRate", "スキル確率", Fmt::Percent(2), |p| p.skill_rate),
    ("ceilSkillRate", "天井補正\nスキル確率", Fmt::Percent(2), |p| p.ceil_skill_rate),
    ("skillPerDay", "スキル\n回数/日", Fmt::Num(2), |p| p.skill_per_day),
    ("skillEnergyPerDay", "スキル\nエナジー/日", Fmt::Num(1), |p| p.skill_energy_per_day),
    ("shard", "スキル\nゆめかけ/日", Fmt::Num(1), |p| p.shard),
    ("energyPerDay", "エナジー\n/日", Fmt::Num(1), |p| p.energy_per_day),
    ("supportEnergyPerDay", "サポート\nエナジー/日", Fmt::Num(1), |p| p.support_energy_per_day),
    ("supportShardPerDay", "サポート\nゆめかけ/日", Fmt::Num(1), |p| p.support_shard_per_day),
];

/// 厳選の列の種類(キー, 見出し)。本家 index.vue の evaluateList
const EVAL_KINDS: [(&str, &str); 5] = [("energy", ""), ("specialty", "得意"), ("berry", "きのみ"), ("food", "食材"), ("skill", "スキル")];

/// 表示モード(本家 index.vue の mode)。本家と同じく保存しない
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum Mode {
    #[default]
    Normal,
    /// ボックス整理(簡易)
    Cleaning,
    /// ボックス整理(詳細)
    CleaningDetail,
    /// まっしろミント
    PureMint,
}

impl Mode {
    const ALL: [(Mode, &str); 4] =
        [(Mode::Normal, "通常"), (Mode::Cleaning, "ボックス整理(簡易)"), (Mode::CleaningDetail, "ボックス整理(詳細)"), (Mode::PureMint, "まっしろミント")];

    fn cleaning(self) -> bool {
        matches!(self, Mode::Cleaning | Mode::CleaningDetail)
    }
}

/// 概算で使ったポケモン(進化を仮定していれば進化後)
fn base_of(p: &BoxPokemon, result: Option<&BoxResult>) -> Option<&'static Pokemon> {
    let m = master();
    result.map(|r| &m.pokemon[r.pokemon.base]).or_else(|| m.pokemon(&p.name))
}

/// 種ポケモン
fn seed_of(base: &Pokemon) -> Option<&'static Pokemon> {
    base.seed.as_deref().and_then(|s| master().pokemon(s))
}

/// 概算結果のポケモンの種ポケ名
fn result_seed(r: &BoxResult) -> Option<&'static str> {
    master().pokemon[r.pokemon.base].seed.as_deref()
}

/// 「アメ情報」の列(キー, 見出し)。先頭3つは入力欄
const CANDY_COLUMNS: [(&str, &str); 11] = [
    ("candy", "所持ｱﾒ"),
    ("training", "目標Lv"),
    ("nextExp", "次Lv迄\nのExp"),
    ("normalCandyNum", "通常\nｱﾒ"),
    ("normalCandyShard", "通常\nゆめかけ"),
    ("boostCandyNum", "ﾌﾞｰｽﾄ\nｱﾒ"),
    ("boostCandyShard", "ﾌﾞｰｽﾄ\nゆめかけ"),
    ("bestBoostCandyNum", "最適\nﾌﾞｰｽﾄ"),
    ("bestBoostCandyShard", "最適ﾌﾞｰｽﾄ\nゆめかけ"),
    ("bestNormalCandyNum", "最適\n通常"),
    ("bestNormalCandyShard", "最適通常\nゆめかけ"),
];

fn require_value(r: &RequireInfo, n: usize) -> Option<f64> {
    match n {
        3 => r.normal_candy_num,
        4 => r.normal_candy_shard,
        5 => r.boost_candy_num,
        6 => r.boost_candy_shard,
        7 => r.best_boost_candy_num,
        8 => r.best_boost_candy_shard,
        9 => r.best_normal_candy_num,
        10 => r.best_normal_candy_shard,
        _ => None,
    }
}

/// 「基礎情報」の列(キー, 見出し)
const BASE_COLUMNS: [(&str, &str); 4] = [("no", "図鑑\nNo"), ("afterList", "最終進化"), ("berryName", "きのみ"), ("skillName", "スキル")];

/// 列見出しの説明(本家 index.vue の help-button)
fn detail_help(title: &str) -> Option<&'static str> {
    match title {
        "エナジー\n/日" => Some("おてつだいボーナスや他ポケモンに影響を与えるスキルの効果を抜きにして、自分自身だけで稼げるエナジーです。"),
        "サポート\nエナジー/日" => Some("おてつだいボーナスやげんき回復系スキル、おてつだいサポートなどによる他ポケモンの効率上昇によって、追加で得られるであろうエナジーです。"),
        "サポート\nゆめかけ/日" => Some("おてつだいボーナスやげんき回復系スキルによる他ポケモンの効率上昇、もしくはゆめのかけらボーナスやリサーチEXPボーナスによって、得られるであろうゆめのかけらの量です。\n現状の実装ではリサーチEXPボーナスもゆめのかけらとして扱ってこの値に含めています。"),
        "整理備考" => Some("✅️がついている項目は「チェックリスト」に該当しているポケモンです。"),
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
fn keyword_match(p: &BoxPokemon, query: &str) -> bool {
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
fn simulate_key(shared: &Shared, mode: Mode) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    mode.hash(&mut h);
    let mut v = shared.config.to_value();
    if let Some(m) = v.as_object_mut() {
        // 表示だけの設定と、本家でも再計算しないアメの設定は除く
        m.remove("pokemonList");
        m.remove("summary");
        m.remove("sortableTable");
        m.remove("candy");
    }
    v.to_string().hash(&mut h);
    // 目標Lv・次Lv迄のExp は必要アメ数の計算だけに使う
    let list: Vec<BoxPokemon> = shared.box_file.list.iter().map(|p| BoxPokemon { training: None, next_exp: None, ..p.clone() }).collect();
    serde_json::to_string(&list).unwrap_or_default().hash(&mut h);
    if let Some(t) = shared.usable_evaluate_table() {
        format!("{:?}{:?}", t.healer, t.support).hash(&mut h);
        t.pokemon.len().hash(&mut h);
    }
    h.finish()
}

/// ボックス一覧の表示設定(本家 `config.pokemonList`)
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ListSetting {
    sub_skill_short: bool,
    memo: bool,
    /// 0: 非表示、1: 総合スコアのみ、2: 総合スコア＆とくい、3: 総合スコア＆きのみ＆食材＆スキル
    evaluate_list: f64,
    select_energy: bool,
    select_detail: bool,
    candy: bool,
    base_info: bool,
    food_info: bool,
    simulated_info: bool,
    fix_score: bool,
    page_unit: Option<f64>,
    cleaning: CleaningSetting,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

/// 本家 `config.pokemonList.cleaning`
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CleaningSetting {
    /// 色違いも整理備考に表示する
    shiny_lock: bool,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for CleaningSetting {
    fn default() -> Self {
        CleaningSetting { shiny_lock: true, extra: Default::default() }
    }
}

impl Default for ListSetting {
    /// 本家 default-config.ts の初期値
    fn default() -> Self {
        ListSetting {
            sub_skill_short: true,
            memo: false,
            evaluate_list: 2.0,
            select_energy: false,
            select_detail: false,
            candy: false,
            base_info: true,
            food_info: false,
            simulated_info: false,
            fix_score: false,
            page_unit: Some(50.0),
            cleaning: CleaningSetting::default(),
            extra: Default::default(),
        }
    }
}

impl ListSetting {
    fn load(shared: &Shared) -> ListSetting {
        shared.config.extra.get("pokemonList").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }

    fn save(&self, shared: &mut Shared) {
        shared.config.extra.insert("pokemonList".into(), serde_json::to_value(self).expect("表示設定"));
        shared.save_config();
    }

    /// 1ページの件数(0 以下・未設定ならページ分けしない)
    fn page_unit(&self) -> Option<usize> {
        self.page_unit.filter(|x| *x >= 1.0).map(|x| x as usize)
    }
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

/// ボックス一覧の列
#[derive(Clone, Copy, PartialEq)]
enum Col {
    /// 選択・編集・削除・固定/除外の列(いつも先頭で、入れ替えない)
    Op,
    /// 種ポケの図鑑No(ボックス整理)
    SeedNo,
    /// 図鑑No(ボックス整理)
    PokemonNo,
    Index,
    Name,
    /// 整理備考(ボックス整理)
    HitCheck,
    Memo,
    Lv,
    Food,
    SkillLv,
    Sub(usize),
    Nature,
    Score,
    /// 厳選(EVAL_KINDS の位置, レベル, 元値なら true)
    Eval(usize, EvalLv, bool),
    Candy(usize),
    Base(usize),
    /// 食材ごとの1日の個数(食材インデックス)
    FoodNum(usize),
    Detail(usize),
}

impl Col {
    /// 表示設定で出す列(最初の並び)
    fn all(setting: &ListSetting, levels: &[u32], mode: Mode) -> Vec<Col> {
        let mut list = Vec::new();
        if mode.cleaning() {
            list.extend([Col::SeedNo, Col::PokemonNo]);
        }
        list.extend([Col::Index, Col::Name]);
        if mode.cleaning() {
            list.push(Col::HitCheck);
        }
        if setting.memo {
            list.push(Col::Memo);
        }
        list.extend([Col::Lv, Col::Food, Col::SkillLv]);
        list.extend((0..SUB_SKILL_LV.len()).map(Col::Sub));
        list.extend([Col::Nature, Col::Score]);
        let kinds: &[usize] = match setting.evaluate_list as i32 {
            1 => &[0],
            2 => &[0, 1],
            3 => &[0, 2, 3, 4],
            _ => &[],
        };
        for &k in kinds {
            list.push(Col::Eval(k, EvalLv::Max, false));
            for &lv in levels {
                list.push(Col::Eval(k, EvalLv::Lv(lv), false));
                if setting.select_energy {
                    list.push(Col::Eval(k, EvalLv::Lv(lv), true));
                }
            }
        }
        if setting.candy {
            list.extend((0..CANDY_COLUMNS.len()).map(Col::Candy));
        }
        if setting.base_info {
            list.extend((0..BASE_COLUMNS.len()).map(Col::Base));
        }
        if setting.food_info {
            list.extend((0..master().foods.len()).map(Col::FoodNum));
        }
        if setting.simulated_info {
            list.extend((0..DETAIL_COLUMNS.len()).map(Col::Detail));
        }
        list
    }

    /// 並べ替え・列の並びの保存に使うキー(本家と同じ)
    fn key(self) -> String {
        match self {
            Col::Op => "edit".into(),
            Col::SeedNo => "pokemonSeedNo".into(),
            Col::PokemonNo => "pokemonNo".into(),
            Col::HitCheck => "hitCheckList".into(),
            Col::Index => "index".into(),
            Col::Name => "name".into(),
            Col::Memo => "memo".into(),
            Col::Lv => "lv".into(),
            Col::Food => "foodNameList".into(),
            Col::SkillLv => "skillLv".into(),
            Col::Sub(slot) => format!("subSkill{}", SUB_SKILL_LV[slot]),
            Col::Nature => "natureName".into(),
            Col::Score => "score".into(),
            Col::Eval(k, lv, value) => {
                let lv = match lv {
                    EvalLv::Max => "max".to_string(),
                    EvalLv::Lv(lv) => lv.to_string(),
                };
                format!("{}_{}_{lv}", EVAL_KINDS[k].0, if value { "value" } else { "percentile" })
            }
            Col::Candy(n) => CANDY_COLUMNS[n].0.into(),
            Col::Base(n) => BASE_COLUMNS[n].0.into(),
            Col::FoodNum(n) => master().foods[n].name.clone(),
            Col::Detail(n) => DETAIL_COLUMNS[n].0.into(),
        }
    }

    fn title(self, setting: &ListSetting) -> String {
        match self {
            Col::Op => String::new(),
            Col::SeedNo => "種ポケ\n図鑑No".into(),
            Col::PokemonNo => "図鑑\nNo".into(),
            Col::HitCheck => "整理備考".into(),
            Col::Index => "No".into(),
            Col::Name => "名前".into(),
            Col::Memo => "メモ".into(),
            Col::Lv => "Lv".into(),
            Col::Food => "食材".into(),
            Col::SkillLv => "ｽｷﾙ\nLv".into(),
            Col::Sub(slot) => if setting.sub_skill_short { format!("サブ{}", SUB_SKILL_LV[slot]) } else { format!("サブスキル{}", SUB_SKILL_LV[slot]) },
            Col::Nature => "性格".into(),
            Col::Score => "スコア".into(),
            Col::Eval(k, lv, value) => {
                let (key, name) = EVAL_KINDS[k];
                let lv = match lv {
                    EvalLv::Max => "最大".to_string(),
                    EvalLv::Lv(lv) => lv.to_string(),
                };
                if value {
                    format!("{name}{}\n({lv})", if key == "energy" { "エナジー" } else { "数量" })
                } else {
                    format!("{name}厳選\n({lv})")
                }
            }
            Col::Candy(n) => CANDY_COLUMNS[n].1.into(),
            Col::Base(n) => BASE_COLUMNS[n].1.into(),
            Col::FoodNum(n) => master().foods[n].name.clone(),
            Col::Detail(n) => DETAIL_COLUMNS[n].1.into(),
        }
    }

    /// 列の幅(width は覚えている幅。無ければ最初の幅)
    fn width(self, width: Option<f32>, setting: &ListSetting) -> Column {
        let (initial, min) = match self {
            Col::Op => (86.0, 86.0),
            Col::SeedNo | Col::PokemonNo => (60.0, 32.0),
            Col::HitCheck => (150.0, 60.0),
            Col::Index => (36.0, 24.0),
            Col::Name => (150.0, 80.0),
            Col::Memo => (120.0, 40.0),
            Col::Lv => (40.0, 24.0),
            Col::SkillLv => (40.0, 24.0),
            Col::Food => (84.0, 40.0),
            Col::Sub(_) => (if setting.sub_skill_short { 72.0 } else { 150.0 }, 32.0),
            Col::Nature => (80.0, 24.0),
            Col::Score => (90.0, 24.0),
            Col::Eval(..) => (if setting.select_detail { 180.0 } else { 90.0 }, 40.0),
            Col::Candy(_) => (64.0, 32.0),
            Col::Base(1) => (130.0, 40.0),
            Col::Base(_) => (90.0, 32.0),
            Col::FoodNum(_) => (60.0, 32.0),
            Col::Detail(_) => (84.0, 32.0),
        };
        Column::initial(width.unwrap_or(initial).max(min)).at_least(min)
    }

    /// 2行の見出しか
    fn tall_header(self, setting: &ListSetting) -> bool {
        self.title(setting).contains('\n') || matches!(self, Col::FoodNum(_))
    }
}

/// 本家 sortable-table の fixColumn(操作列を除く列の数 n)を当てはめた、左に固定する列の数。
/// 本家はサブスキル5つで1列なので、続けて並んだサブスキルの列は1列と数える
fn fixed_count(columns: &[Col], n: usize) -> usize {
    let mut count = 0;
    for (k, c) in columns.iter().enumerate() {
        let joined = matches!(c, Col::Sub(_)) && k > 0 && matches!(columns[k - 1], Col::Sub(_));
        if !joined {
            if count == n {
                return k;
            }
            count += 1;
        }
    }
    columns.len()
}

/// 表の中での操作(描画後にまとめて反映する)
enum TableEvent {
    /// 並べ替え(列のキー, Shift)
    Sort(String, bool),
    /// 測った列の幅
    Width(String, f32),
    /// 列の入れ替え(動かす列, 入れる先の列, 先の列の後ろか)
    Move(Col, Col, bool),
    ResetOrder,
    SelectAll(bool),
    Toggle(usize),
    Action(RowAction),
    Cell(CellEdit),
}

/// 列の並びと幅の保存場所(本家に無い項目なので、設定ではなくウインドウ位置と同じ window.ron に入れる)
fn column_order_id() -> egui::Id {
    egui::Id::new("box_column_order")
}

fn column_width_id() -> egui::Id {
    egui::Id::new("box_column_width")
}

/// 保存された列の並び。保存に無い列は、最初の並びで直前にある列の後ろに入れる
fn column_order(ctx: &egui::Context, all: &[Col]) -> Vec<Col> {
    let saved: Vec<String> = ctx.data_mut(|d| d.get_persisted(column_order_id())).unwrap_or_default();
    let mut order: Vec<Col> = saved.iter().filter_map(|k| all.iter().copied().find(|c| c.key() == *k)).collect();
    let mut last = None;
    for &c in all {
        match order.iter().position(|x| *x == c) {
            Some(n) => last = Some(n),
            None => {
                let n = last.map_or(0, |n| n + 1);
                order.insert(n, c);
                last = Some(n);
            }
        }
    }
    order
}

/// 列の並びを保存する(None は最初の並びに戻す)。表示していない列の位置も残す
fn set_column_order(ctx: &egui::Context, order: Option<&[Col]>) {
    match order {
        Some(order) => {
            let mut keys: Vec<String> = order.iter().map(|c| c.key()).collect();
            let saved: Vec<String> = ctx.data_mut(|d| d.get_persisted(column_order_id())).unwrap_or_default();
            for (i, k) in saved.iter().enumerate() {
                if !keys.contains(k) {
                    // 保存済みの並びで直前にあった列の後ろへ
                    let pos = saved[..i].iter().rev().find_map(|p| keys.iter().position(|x| x == p)).map_or(0, |n| n + 1);
                    keys.insert(pos, k.clone());
                }
            }
            ctx.data_mut(|d| d.insert_persisted(column_order_id(), keys))
        }
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

/// 表の中で入力した値(描画後にまとめて反映する)
enum CellEdit {
    Candy(String, Option<f64>),
    Training(usize, Option<f64>),
    NextExp(usize, Option<f64>),
}

/// 厳選の値の表示(本家 evaluate-result.vue)
fn eval_text(v: f64, key: &str, value: bool) -> String {
    if !value {
        format!("{}%", to_fixed(v * 100.0, 1))
    } else if key != "energy" {
        to_fixed(v, 1)
    } else {
        fmt_num(v)
    }
}

/// まっしろミントでの元の性格との差(本家 evaluate-result.vue)
fn diff_text(d: f64, key: &str, value: bool) -> String {
    let sign = if d > 0.0 { "+" } else { "" };
    if !value {
        format!("{sign}{}%", to_fixed(d * 100.0, 1))
    } else if key != "energy" {
        format!("{sign}{}", to_fixed(d, 1))
    } else {
        format!("{sign}{}", fmt_num(pokesle_core::js::round(d)))
    }
}

/// 厳選の値の進化先一覧(進化先名, 値, 最良か, まっしろミントでの差)。表示設定「最適でない進化先の厳選値も表示」なら全進化先
fn eval_rows(result: Option<&BoxResult>, k: usize, lv: EvalLv, value: bool, detail: bool) -> Vec<(String, Option<f64>, bool, Option<f64>)> {
    let m = master();
    let key = EVAL_KINDS[k].0;
    let Some(r) = result else { return Vec::new() };
    let Some(e) = r.evaluate_result.as_ref() else { return Vec::new() };
    let best = e.best.get(&lv).and_then(|b| b.get(key)).and_then(|x| x.name.clone());
    let after: Vec<String> = if detail { m.pokemon[r.pokemon.base].after_list.clone() } else { best.clone().into_iter().collect() };
    let multi = after.len() > 1;
    after
        .into_iter()
        .map(|a| {
            let info = e.get(lv, &a).and_then(|x| x.get(key));
            let pick = |x: &pokesle_core::evaluate_table::EvalInfo| if value { x.value } else { x.score };
            let original = info.map(pick).filter(|x| !x.is_nan());
            // まっしろミントの評価があればそちらを出し、元の性格との差を添える
            let (v, diff) = match info.and_then(|x| x.pure_mint.as_deref()) {
                Some(pm) => {
                    let v = Some(pick(pm)).filter(|x| !x.is_nan());
                    (v, v.zip(original).map(|(a, b)| a - b))
                }
                None => (original, None),
            };
            let is_best = multi && best.as_deref() == Some(a.as_str());
            (a, v, is_best, diff)
        })
        .collect()
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
    /// 表示中のページ(0 始まり)
    page: usize,
    /// 選択中のポケモン(ボックス位置)。まとめて削除する(本家 selectedPokemonList)
    selected: HashSet<usize>,
    /// 削除の確認(メッセージ, 消すボックス位置)
    confirm_delete: Option<(String, Vec<usize>)>,
    /// 一覧の縦のスクロール位置(左右の表で揃える)
    scroll_offset: f32,
    /// マウスが乗っている行(左右の表で揃える)
    hover_row: Option<usize>,
    mode: Mode,
    /// ボックス整理(詳細)のタブ(0: サマリー情報、1: 詳細)
    detail_tab: usize,
    /// ボックス整理(詳細)で確認中の最終進化ポケモン
    detail_pokemon: Option<String>,
    /// ボックス整理(詳細)で絞り込み中の種ポケ
    detail_seed: Option<String>,
    /// サマリー情報の並べ替え(列のキー, 1=昇順 / -1=降順)
    summary_sort: Vec<(String, i8)>,
}

/// 一覧の操作列でのクリック
enum RowAction {
    Edit(usize),
    Delete(usize),
    /// 固定・除外の切り替え(None なら 未設定→固定→除外→未設定 の順に進める)
    Fix(usize, Option<Option<f64>>),
    /// お気に入りの切り替え(ボックス整理)
    Favorite(usize),
}

impl BoxView {
    /// 見た目の確認用(デバッグ版のみ): 表示モードを指定して起動する
    #[cfg(debug_assertions)]
    pub fn debug_mode(&mut self, mode: &str) {
        self.mode = match mode {
            "cleaning" => Mode::Cleaning,
            "cleaning_detail" | "cleaning_detail1" => Mode::CleaningDetail,
            "pure_mint" => Mode::PureMint,
            _ => Mode::Normal,
        };
        self.detail_tab = (mode == "cleaning_detail1") as usize;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        self.poll_jobs(shared);

        // 1段目: 共通設定と表示設定(本家 index.vue)
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            changed |= common_setting::ui(ui, &mut shared.config, &shared.box_file.list);
            display_setting(ui, shared);
        });
        if changed {
            shared.save_config();
        }
        // 2段目: キーワード
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            style::th(ui, "表示モード");
            let label = Mode::ALL.iter().find(|(m, _)| *m == self.mode).map_or("", |(_, l)| *l);
            egui::ComboBox::from_id_salt("box_mode").selected_text(label).show_ui(ui, |ui| {
                for (m, l) in Mode::ALL {
                    if ui.selectable_value(&mut self.mode, m, l).changed() {
                        self.page = 0;
                    }
                }
            });
            ui.add_space(10.0);
            style::th(ui, "キーワード");
            if ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("名前、食材名、スキル名など").desired_width(200.0)).changed() {
                self.page = 0;
            }
            ui.add_space(10.0);
            style::th(ui, "ボックス整理");
            // お気に入りは選べない(本家 selectedPokemonList)
            self.selected.retain(|&i| shared.box_file.list.get(i).is_some_and(|p| !p.favorite));
            let n = self.selected.len();
            let r = ui.add_enabled_ui(n > 0, |ui| style::form_button(ui, &format!("選択したポケモン({n}匹)を削除"), style::ButtonKind::Important)).inner;
            if r.clicked() {
                let mut list: Vec<usize> = self.selected.iter().copied().collect();
                list.sort_unstable();
                self.confirm_delete = Some(("選択したポケモンを削除します。よろしいですか？".into(), list));
            }
            if self.mode.cleaning() {
                let mut s = ListSetting::load(shared);
                if ui.checkbox(&mut s.cleaning.shiny_lock, "色違いも整理備考に表示する").changed() {
                    s.save(shared);
                }
            }
        });
        if self.mode == Mode::CleaningDetail {
            ui.add_space(5.0);
            let detail = match self.detail_seed.as_ref().or(self.detail_pokemon.as_ref()) {
                Some(x) => format!("詳細({x})"),
                None => "詳細".to_string(),
            };
            if let Some(t) = style::tab_list(ui, &["サマリー情報".to_string(), detail], self.detail_tab) {
                self.detail_tab = t;
            }
        }

        // 設定・ボックス・厳選テーブルが変わったら概算し直す(本家は自動で再計算する)
        let key = simulate_key(shared, self.mode);
        if self.simulate_job.is_none() && self.simulate_key != Some(key) {
            self.simulate_key = Some(key);
            if shared.box_file.list.is_empty() {
                self.results.clear();
            } else {
                let mut config = shared.config.clone();
                config.cleaning = self.mode.cleaning();
                config.pure_mint = self.mode == Mode::PureMint;
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

        let setting = ListSetting::load(shared);
        // ボックス整理(詳細)のサマリー情報(本家は一覧の代わりに出す)
        let show_list = self.mode != Mode::CleaningDetail || self.detail_tab == 1;
        let rows = if show_list { self.sorted_rows(shared) } else { Vec::new() };
        // ページ分け(本家 sortable-table の pager)
        let page_max = setting.page_unit().map(|u| rows.len().div_ceil(u).max(1));
        if let Some(max) = page_max {
            self.page = self.page.min(max - 1);
        }

        // 下段: ページ送り・追加・TSV
        egui::Panel::bottom("box_actions").frame(egui::Frame::new().inner_margin(egui::Margin { top: 6, ..Default::default() })).show(ui, |ui| {
            if let Some(max) = page_max.filter(|_| show_list) {
                self.pager(ui, max);
                ui.add_space(4.0);
            }
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

        let rows = match setting.page_unit() {
            Some(u) => rows.into_iter().skip(u * self.page).take(u).collect(),
            None => rows,
        };
        if show_list {
            self.table(ui, shared, &setting, rows);
        } else if self.simulate_job.is_none() {
            self.summary(ui);
        }
        self.import_window(ui.ctx(), shared);
        self.confirm_delete_window(ui.ctx(), shared);

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

    /// 本家 sortable-table の pager(Prev 1 2 3 … Next)
    fn pager(&mut self, ui: &mut egui::Ui, max: usize) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            if ui.add(egui::Button::new("‹ Prev").frame(false)).clicked() {
                self.page = self.page.saturating_sub(1);
            }
            for p in 0..max {
                let text = egui::RichText::new((p + 1).to_string());
                let text = if p == self.page { text.strong().color(egui::Color32::WHITE) } else { text };
                let button = egui::Button::new(text).min_size(egui::vec2(26.0, 22.0)).fill(if p == self.page { style::PRIMARY } else { egui::Color32::TRANSPARENT });
                if ui.add(button).clicked() {
                    self.page = p;
                }
            }
            if ui.add(egui::Button::new("Next ›").frame(false)).clicked() {
                self.page = (self.page + 1).min(max - 1);
            }
        });
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

    /// キーワードで絞り込み、並べ替えたボックス位置の一覧
    fn sorted_rows(&self, shared: &Shared) -> Vec<usize> {
        let query = self.search.trim().to_string();
        let result = |i: usize| self.results.get(i).and_then(|r| r.as_ref());
        let mut rows: Vec<usize> = (0..shared.box_file.list.len())
            .filter(|&i| {
                if self.mode != Mode::CleaningDetail {
                    return query.is_empty() || keyword_match(&shared.box_file.list[i], &query);
                }
                // ボックス整理(詳細)はキーワードではなく、サマリー情報で選んだポケモン・種ポケで絞り込む
                match (&self.detail_seed, &self.detail_pokemon) {
                    (Some(seed), _) => result(i).and_then(result_seed).is_some_and(|s| s == seed),
                    (None, Some(name)) => result(i).is_some_and(|r| r.pokemon.after_list.contains(name)),
                    (None, None) => true,
                }
            })
            .collect();
        let sort = sort_info(shared);
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
        rows
    }

    fn table(&mut self, ui: &mut egui::Ui, shared: &mut Shared, setting: &ListSetting, rows: Vec<usize>) {
        let m = master();
        let mut sort = sort_info(shared);
        let selectable: Vec<usize> = rows.iter().copied().filter(|&i| !shared.box_file.list[i].favorite).collect();
        let levels = shared.config.select_evaluate.enabled_levels();
        let all = Col::all(setting, &levels, self.mode);
        let mut order = column_order(ui.ctx(), &all);
        let columns = order.clone();
        // 表は列の幅を位置ごとに覚えるので、並びが変わったら別の表として作り直し、
        // 測っておいた各列の幅を最初の幅として渡す
        let mut widths = column_widths(ui.ctx());
        let order_salt: Vec<String> = columns.iter().map(|c| c.key()).collect();
        let tall_header = columns.iter().any(|c| c.tall_header(setting));
        let has_eval = columns.iter().any(|c| matches!(c, Col::Eval(..)));
        let has_hit = columns.contains(&Col::HitCheck);
        let shiny_lock = setting.cleaning.shiny_lock;
        let pure_mint = self.mode == Mode::PureMint;
        // 行の高さ: 厳選の列は進化先名と値の2段(全進化先を出すときは進化先の数だけ)
        let heights: Vec<f32> = rows
            .iter()
            .map(|&i| {
                let result = self.results.get(i).and_then(|r| r.as_ref());
                // 整理備考は項目の数だけ
                let hit = if has_hit {
                    let p = &shared.box_file.list[i];
                    let n = p.favorite as usize + (p.shiny && shiny_lock) as usize + result.map_or(0, |r| r.hit_check_list.len());
                    n as f32 * 22.0 + 4.0
                } else {
                    0.0
                };
                let eval = if !has_eval {
                    26.0
                } else if !setting.select_detail {
                    // まっしろミントは差分の段が増える
                    if pure_mint { 50.0 } else { 36.0 }
                } else {
                    let n = result.map_or(1, |r| m.pokemon[r.pokemon.base].after_list.len().max(1));
                    (n as f32 * 18.0 + 6.0).max(26.0)
                };
                eval.max(hit)
            })
            .collect();

        // 本家 sortable-table の fixColumn: 左の列は固定し、残りの列だけ横にスクロールする。
        // egui の表には列の固定が無いので、左右2つの表を並べて縦のスクロール位置を揃える
        let fixed = fixed_count(&columns, if setting.fix_score { 9 } else { 3 });
        let mut left = vec![Col::Op];
        left.extend_from_slice(&columns[..fixed]);
        let scrolled = &columns[fixed..];

        let sort_mark = |key: &str| match sort.iter().position(|(k, _)| k == key) {
            // 並べ替え中の列には ▲(昇順)/▼(降順) と、条件が複数なら優先順を付ける
            Some(n) => {
                let arrow = if sort[n].1 > 0 { "▲" } else { "▼" };
                if sort.len() > 1 { format!("{arrow}{}", n + 1) } else { arrow.to_string() }
            }
            None => String::new(),
        };
        let selected = &self.selected;
        let results = &self.results;
        let cleaning = self.mode.cleaning();
        // 本家はボックス整理でお気に入りの星が増える
        let op_width = if cleaning { 106.0 } else { 86.0 };
        let list = &shared.box_file.list;
        let config = &shared.config;

        let header_cell = |ui: &mut egui::Ui, col: Col| -> Vec<TableEvent> {
            let mut ev = Vec::new();
            let key = col.key();
            if col == Col::Op {
                // 並べ替えのクリックはマス全体で受け、上に置く全選択のチェックボックスを優先する
                let r = ui
                    .interact(ui.max_rect(), egui::Id::new(("box_sort", &key)), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("この列でソートするとPTシミュへの固定/除外でソートできます。");
                if r.clicked() {
                    ev.push(TableEvent::Sort(key.clone(), ui.input(|i| i.modifiers.shift)));
                }
                // 表示中の行(お気に入りを除く)をまとめて選ぶ。本家にはない
                let n = selectable.iter().filter(|i| selected.contains(i)).count();
                let mut all = n > 0 && n == selectable.len();
                let checkbox = egui::Checkbox::without_text(&mut all).indeterminate(n > 0 && n < selectable.len());
                if ui.add_enabled(!selectable.is_empty(), checkbox).on_hover_text("表示中のポケモンをすべて選択/解除").clicked() {
                    ev.push(TableEvent::SelectAll(n < selectable.len()));
                }
                ui.add(egui::Label::new(egui::RichText::new(sort_mark(&key)).strong()).selectable(false));
                return ev;
            }
            let title = col.title(setting);
            let mark = sort_mark(&key);
            if let Col::FoodNum(n) = col {
                ui.vertical(|ui| {
                    food_icon::food_icon(ui, &m.foods[n].name, 20.0);
                    ui.add(egui::Label::new(egui::RichText::new(mark.clone()).strong()).selectable(false));
                });
            } else {
                ui.add(egui::Label::new(egui::RichText::new(format!("{title}{mark}")).strong()).selectable(false));
            }
            // 文字の横の空いたところも含めて、見出しのマス全体をクリック・ドラッグできるようにする
            let rect = ui.max_rect();
            let mut r = ui.interact(rect, egui::Id::new(("box_sort", &key)), egui::Sense::click_and_drag());
            r = if r.dragged() { r.on_hover_cursor(egui::CursorIcon::Grabbing) } else { r.on_hover_cursor(egui::CursorIcon::PointingHand) };
            if let Some(help) = detail_help(&title) {
                r = r.on_hover_text(help);
            }
            r = r.on_hover_text("クリックで並べ替え(昇順→降順→解除)\nShift+クリックで条件を追加\nドラッグで列を入れ替え");
            if r.clicked() {
                ev.push(TableEvent::Sort(key.clone(), ui.input(|i| i.modifiers.shift)));
            }
            r.context_menu(|ui| {
                if ui.button("列の並びを最初に戻す").clicked() {
                    ev.push(TableEvent::ResetOrder);
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
                    ev.push(TableEvent::Move(*src, col, after));
                }
            }
            ev
        };

        let body_cell = |ui: &mut egui::Ui, col: Col, i: usize, result: Option<&BoxResult>, require: Option<&RequireInfo>| -> Option<TableEvent> {
            let p = &list[i];
            match col {
                Col::Op => {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let mut checked = selected.contains(&i);
                    if ui.add_enabled(!p.favorite, egui::Checkbox::without_text(&mut checked)).clicked() {
                        return Some(TableEvent::Toggle(i));
                    }
                    if icon_button(ui, Icon::Edit, egui::Color32::from_gray(0x88)).on_hover_text("編集").clicked() {
                        return Some(TableEvent::Action(RowAction::Edit(i)));
                    }
                    let delete_color = if p.favorite { egui::Color32::from_gray(0xcc) } else { egui::Color32::from_gray(0x88) };
                    if icon_button(ui, Icon::Delete, delete_color).on_hover_text("削除").clicked() && !p.favorite {
                        return Some(TableEvent::Action(RowAction::Delete(i)));
                    }
                    let (icon, color) = match p.fix {
                        Some(1.0) => (Icon::Fix, egui::Color32::from_rgb(0x66, 0xcc, 0x44)),
                        Some(-1.0) => (Icon::Exclude, egui::Color32::from_rgb(0xee, 0x44, 0x00)),
                        _ => (Icon::Fix, egui::Color32::from_gray(0x88)),
                    };
                    let r = icon_button(ui, icon, color).on_hover_text("チームのシミュレーションで固定・除外する設定\nクリックで 固定→除外→解除、右クリックで解除");
                    if r.clicked() {
                        return Some(TableEvent::Action(RowAction::Fix(i, None)));
                    }
                    if r.secondary_clicked() {
                        return Some(TableEvent::Action(RowAction::Fix(i, Some(None))));
                    }
                    if cleaning {
                        let color = if p.favorite { egui::Color32::from_rgb(0xff, 0xd7, 0x00) } else { egui::Color32::from_gray(0x88) };
                        if icon_button(ui, Icon::Star, color).on_hover_text("お気に入り").clicked() {
                            return Some(TableEvent::Action(RowAction::Favorite(i)));
                        }
                    }
                }
                Col::SeedNo => {
                    if let Some(b) = base_of(p, result).and_then(seed_of) {
                        ui.label(b.no.to_string());
                    }
                }
                Col::PokemonNo => {
                    if let Some(b) = base_of(p, result) {
                        ui.label(b.no.to_string());
                    }
                }
                Col::HitCheck => hit_check_cell(ui, p, result, shiny_lock),
                Col::Index => {
                    ui.label(format!("{}", i + 1));
                }
                Col::Name => name_cell(ui, p, result, setting.memo),
                Col::Memo => {
                    ui.label(egui::RichText::new(p.memo.clone().unwrap_or_default()).size(11.0));
                }
                Col::Lv => {
                    ui.label(format!("{}", p.lv));
                }
                Col::SkillLv => {
                    // 本家と同じく補正後のスキルLv
                    if let Some(r) = result {
                        ui.label(r.pokemon.fixed_skill_lv.to_string());
                    }
                }
                Col::Food => food_icon::food_list(ui, p),
                Col::Sub(slot) => {
                    let Some(name) = p.sub_skill_list.get(slot) else { return None };
                    let text = if setting.sub_skill_short { m.sub_skill(name).map(|x| x.short.as_str()).unwrap_or("?") } else { name.as_str() };
                    // まだ解放されていないサブスキルは薄く表示する
                    let text = if p.lv >= SUB_SKILL_LV[slot] as f64 { egui::RichText::new(text) } else { egui::RichText::new(text).weak() };
                    ui.label(text).on_hover_text(name);
                }
                Col::Nature => nature_label(ui, &p.nature),
                Col::Score => {
                    if let Some(r) = result {
                        ui.label(fmt_fixed(r.pokemon.score, 1));
                    }
                }
                Col::Eval(k, lv, value) => eval_cell(ui, result, k, lv, value, setting.select_detail, pure_mint),
                Col::Candy(0) => {
                    if let Some(b) = m.pokemon(&p.name) {
                        let mut v = config.candy.bag.get(&b.candy_name).copied();
                        if style::opt_number(ui, &format!("box_candy_{i}"), &mut v, false, 50.0, "") {
                            return Some(TableEvent::Cell(CellEdit::Candy(b.candy_name.clone(), v)));
                        }
                    }
                }
                Col::Candy(1) => {
                    let mut v = p.training;
                    if style::opt_number(ui, &format!("box_training_{i}"), &mut v, false, 50.0, "") {
                        return Some(TableEvent::Cell(CellEdit::Training(i, v)));
                    }
                }
                Col::Candy(2) => {
                    let mut v = p.next_exp;
                    if style::opt_number(ui, &format!("box_next_exp_{i}"), &mut v, false, 50.0, "") {
                        return Some(TableEvent::Cell(CellEdit::NextExp(i, v)));
                    }
                }
                Col::Candy(n) => {
                    if let Some(v) = require.and_then(|r| require_value(r, n)) {
                        right(ui, fmt_fixed(v, 0));
                    }
                }
                Col::Base(n) => {
                    let b = m.pokemon(&p.name)?;
                    match n {
                        0 => {
                            ui.label(b.no.to_string());
                        }
                        1 => {
                            let list = result.map_or(&b.after_list, |r| &r.pokemon.after_list);
                            let text = match list.len() {
                                0 => String::new(),
                                1 => list[0].clone(),
                                _ => format!("{}等", list[0]),
                            };
                            ui.label(egui::RichText::new(text).size(11.0)).on_hover_text(list.join("、"));
                        }
                        2 => {
                            ui.label(&m.berries[b.berry].name);
                        }
                        _ => {
                            ui.label(&m.pokemon_skill(b).name);
                        }
                    }
                }
                Col::FoodNum(n) => {
                    if let Some(r) = result {
                        right(ui, fmt_fixed(r.pokemon.food_count.get(n).copied().unwrap_or(0.0), 1));
                    }
                }
                Col::Detail(n) => {
                    let (_, _, fmt, get) = DETAIL_COLUMNS[n];
                    if let Some(r) = result {
                        right(ui, fmt_value(get(&r.pokemon), fmt));
                    }
                }
            }
            None
        };

        let mut events: Vec<TableEvent> = Vec::new();
        let hover = self.hover_row;
        let mut new_hover = None;
        let offset = self.scroll_offset;
        // 1つの表を描き、縦のスクロール位置を返す。last は右側(残りの幅を埋め、縦のスクロールバーを出す)
        let mut draw = |ui: &mut egui::Ui, salt: &str, cols: &[Col], last: bool| -> f32 {
            let mut table = TableBuilder::new(ui)
                .id_salt((salt, &order_salt))
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .vertical_scroll_offset(offset)
                .scroll_bar_visibility(if last { egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded } else { egui::scroll_area::ScrollBarVisibility::AlwaysHidden });
            for &c in cols {
                // 本家と同じく先頭は選択・編集・削除・固定/除外の列
                table = table.column(if c == Col::Op { Column::exact(op_width).resizable(false) } else { c.width(widths.get(&c.key()).copied(), setting) });
            }
            if last {
                table = table.column(Column::remainder());
            }
            let out = table
                .header(if tall_header { 40.0 } else { 22.0 }, |mut header| {
                    for &col in cols {
                        // header.col が返すのは中身の大きさなので、マスの幅はマスの中で測る
                        let mut cell_width = None;
                        header.col(|ui| {
                            cell_width = Some(ui.max_rect().width());
                            events.extend(header_cell(ui, col));
                        });
                        if let Some(w) = cell_width
                            && col != Col::Op
                        {
                            events.push(TableEvent::Width(col.key(), w));
                        }
                    }
                    if last {
                        header.col(|_| {});
                    }
                })
                .body(|body| {
                    body.heterogeneous_rows(heights.iter().copied(), |mut row| {
                        let n = row.index();
                        let i = rows[n];
                        let result = results.get(i).and_then(|r| r.as_ref());
                        let require = setting.candy.then(|| calc_require_info(&list[i], result.and_then(|r| r.pokemon.nature).map(|n| &m.natures[n]), &config.candy));
                        row.set_selected(selected.contains(&i));
                        // もう片方の表でマウスが乗っている行も強調する
                        if hover == Some(n) {
                            row.set_hovered(true);
                        }
                        for &col in cols {
                            row.col(|ui| events.extend(body_cell(ui, col, i, result, require.as_ref())));
                        }
                        if last {
                            row.col(|_| {});
                        }
                        let r = row.response();
                        if r.hovered() {
                            new_hover = Some(n);
                        }
                        // 行のクリックで選択を切り替える(本家 clickRow)
                        if r.clicked() && !list[i].favorite {
                            events.push(TableEvent::Toggle(i));
                        }
                    });
                });
            out.state.offset.y
        };

        // 文字の上をクリックしても行を選べるよう、文字の選択はしない
        ui.style_mut().interaction.selectable_labels = false;
        let (left_offset, right_offset) = ui
            .horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let l = ui.vertical(|ui| draw(ui, "box_table_left", &left, false)).inner;
                // 列が画面に収まらないときは横にスクロールする
                let r = egui::ScrollArea::horizontal().id_salt("box_table_right_scroll").show(ui, |ui| ui.vertical(|ui| draw(ui, "box_table_right", scrolled, true)).inner).inner;
                (l, r)
            })
            .inner;
        // 動かした方の表に合わせる(もう片方は次の描画で追いつく)
        let new_offset = if right_offset != offset { right_offset } else { left_offset };
        if new_offset != offset || new_hover != self.hover_row {
            ui.ctx().request_repaint();
        }
        self.scroll_offset = new_offset;
        self.hover_row = new_hover;

        let mut sort_changed = false;
        let mut moved: Option<(Col, Col, bool)> = None;
        let mut reset_order = false;
        let mut action: Option<RowAction> = None;
        let mut toggle_select: Option<usize> = None;
        let mut select_all: Option<bool> = None;
        let mut edits: Vec<CellEdit> = Vec::new();
        for e in events {
            match e {
                TableEvent::Sort(key, shift) => {
                    toggle_sort(&mut sort, &key, shift);
                    sort_changed = true;
                }
                TableEvent::Width(key, w) => {
                    widths.insert(key, w);
                }
                TableEvent::Move(src, dst, after) => moved = Some((src, dst, after)),
                TableEvent::ResetOrder => reset_order = true,
                TableEvent::SelectAll(v) => select_all = Some(v),
                TableEvent::Toggle(i) => toggle_select = Some(i),
                TableEvent::Action(a) => action = Some(a),
                TableEvent::Cell(c) => edits.push(c),
            }
        }

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

        if !edits.is_empty() {
            let mut box_changed = false;
            for e in edits {
                match e {
                    // 本家は空欄を null で保存するが、計算上は 0 と同じ
                    CellEdit::Candy(name, v) => {
                        shared.config.candy.bag.insert(name, v.unwrap_or(0.0));
                    }
                    CellEdit::Training(i, v) => {
                        shared.box_file.list[i].training = v;
                        box_changed = true;
                    }
                    CellEdit::NextExp(i, v) => {
                        shared.box_file.list[i].next_exp = v;
                        box_changed = true;
                    }
                }
            }
            shared.save_config();
            if box_changed {
                shared.save_box();
            }
        }

        if let Some(i) = toggle_select
            && !self.selected.remove(&i)
        {
            self.selected.insert(i);
        }
        match select_all {
            Some(true) => self.selected.extend(&selectable),
            Some(false) => self.selected.retain(|i| !selectable.contains(i)),
            None => {}
        }
        match action {
            Some(RowAction::Edit(i)) => self.edit = Some(PokemonEdit::new(Some(i), Some(shared.box_file.list[i].clone()))),
            Some(RowAction::Delete(i)) => {
                let p = &shared.box_file.list[i];
                self.confirm_delete = Some((format!("{}(Lv{})を削除します。よろしいですか？", p.name, p.lv), vec![i]));
            }
            // 本家 toggleFix: 未設定 → 固定(1) → 除外(-1) → 未設定
            Some(RowAction::Fix(i, value)) => {
                let p = &mut shared.box_file.list[i];
                p.fix = value.unwrap_or(match p.fix {
                    None => Some(1.0),
                    Some(1.0) => Some(-1.0),
                    _ => None,
                });
                shared.save_box();
            }
            // 本家 toggleFavorite
            Some(RowAction::Favorite(i)) => {
                let p = &mut shared.box_file.list[i];
                p.favorite = !p.favorite;
                shared.save_box();
            }
            None => {}
        }
    }

    /// ボックス整理(詳細)のサマリー情報: 最終進化ポケモンごとの、ボックス内で進化先に持つポケモンの数
    fn summary(&mut self, ui: &mut egui::Ui) {
        struct Row {
            p: &'static Pokemon,
            seed: Option<&'static Pokemon>,
            num: usize,
            checked: bool,
        }
        const COLUMNS: [(&str, &str, f32); 10] = [
            ("seedNo", "種ポケ\n図鑑No", 60.0),
            ("seed", "種ポケ", 200.0),
            ("no", "図鑑No", 60.0),
            ("name", "名前", 230.0),
            ("berryName", "きのみ", 90.0),
            ("typeName", "タイプ", 80.0),
            ("specialty", "とくい", 70.0),
            ("num", "ボックス内\n候補数", 80.0),
            ("checklistChecked", "チェックリスト\n該当", 100.0),
            ("dummy", "", 230.0),
        ];
        let m = master();
        // ポケモンごとにボックス全体を見ず、概算結果を1回だけ集計する(本家と同じ)
        let mut count: HashMap<&str, (usize, bool)> = HashMap::new();
        for r in self.results.iter().flatten() {
            for name in &r.pokemon.after_list {
                let e = count.entry(name.as_str()).or_default();
                e.0 += 1;
                e.1 |= !r.hit_check_list.is_empty();
            }
        }
        let mut list: Vec<Row> = m
            .pokemon
            .iter()
            .filter(|p| p.is_last)
            .map(|p| {
                let (num, checked) = count.get(p.name.as_str()).copied().unwrap_or_default();
                Row { p, seed: seed_of(p), num, checked }
            })
            .collect();
        let sort_value = |r: &Row, key: &str| -> SortValue {
            let text = |v: &str| SortValue::Str(kana_sort_key(v));
            match key {
                "seedNo" => SortValue::Num(r.seed.map_or(0.0, |s| s.no as f64)),
                "seed" => text(r.p.seed.as_deref().unwrap_or("")),
                "no" => SortValue::Num(r.p.no as f64),
                "name" => text(&r.p.name),
                "berryName" => text(&m.berries[r.p.berry].name),
                "typeName" => text(&m.berries[r.p.berry].type_name),
                "specialty" => text(r.p.specialty.name()),
                "num" => SortValue::Num(r.num as f64),
                "checklistChecked" => SortValue::Num(r.checked as u8 as f64),
                _ => SortValue::Num(0.0),
            }
        };
        if !self.summary_sort.is_empty() {
            let sort = &self.summary_sort;
            list.sort_by(|a, b| {
                for (key, dir) in sort {
                    let (x, y) = (sort_value(a, key), sort_value(b, key));
                    let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                    if o != Ordering::Equal {
                        return if *dir > 0 { o } else { o.reverse() };
                    }
                }
                Ordering::Equal
            });
        }

        let mut sort_click: Option<(String, bool)> = None;
        // (最終進化ポケモン, 種ポケ)。確認なら種ポケは None
        let mut pick: Option<(Option<String>, Option<String>)> = None;
        let mut table = TableBuilder::new(ui).id_salt("box_cleaning_summary").striped(true).cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for (_, _, w) in COLUMNS {
            table = table.column(Column::initial(w).at_least(30.0).clip(true));
        }
        table
            .column(Column::remainder())
            .header(40.0, |mut header| {
                for (key, title, _) in COLUMNS {
                    header.col(|ui| {
                        let mark = match self.summary_sort.iter().position(|(k, _)| k == key) {
                            Some(n) => {
                                let arrow = if self.summary_sort[n].1 > 0 { "▲" } else { "▼" };
                                if self.summary_sort.len() > 1 { format!("{arrow}{}", n + 1) } else { arrow.to_string() }
                            }
                            None => String::new(),
                        };
                        ui.add(egui::Label::new(egui::RichText::new(format!("{title}{mark}")).strong()).selectable(false).extend());
                        let r = ui.interact(ui.max_rect(), egui::Id::new(("box_summary_sort", key)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        if r.clicked() {
                            sort_click = Some((key.to_string(), ui.input(|i| i.modifiers.shift)));
                        }
                    });
                }
                header.col(|_| {});
            })
            .body(|body| {
                body.rows(30.0, list.len(), |mut row| {
                    let r = &list[row.index()];
                    row.col(|ui| {
                        right(ui, r.seed.map(|s| s.no.to_string()).unwrap_or_default());
                    });
                    row.col(|ui| {
                        ui.label(r.p.seed.as_deref().unwrap_or(""));
                    });
                    row.col(|ui| right(ui, r.p.no.to_string()));
                    row.col(|ui| {
                        ui.label(&r.p.name);
                    });
                    row.col(|ui| {
                        ui.label(&m.berries[r.p.berry].name);
                    });
                    row.col(|ui| {
                        ui.label(&m.berries[r.p.berry].type_name);
                    });
                    row.col(|ui| {
                        ui.label(r.p.specialty.name());
                    });
                    row.col(|ui| right(ui, r.num.to_string()));
                    row.col(|ui| {
                        if r.checked {
                            ui.label("◯");
                        }
                    });
                    row.col(|ui| {
                        if style::form_button(ui, "確認", style::ButtonKind::Normal).clicked() {
                            pick = Some((Some(r.p.name.clone()), None));
                        }
                        if style::form_button(ui, "種ポケで絞り込み", style::ButtonKind::Normal).clicked() {
                            pick = Some((None, r.p.seed.clone()));
                        }
                    });
                    row.col(|_| {});
                });
            });

        if let Some((key, shift)) = sort_click {
            toggle_sort(&mut self.summary_sort, &key, shift);
        }
        if let Some((pokemon, seed)) = pick {
            self.detail_pokemon = pokemon;
            self.detail_seed = seed;
            self.detail_tab = 1;
            self.page = 0;
        }
    }

    /// 削除の確認(本家は confirm)
    fn confirm_delete_window(&mut self, ctx: &egui::Context, shared: &mut Shared) {
        let Some((message, list)) = &self.confirm_delete else { return };
        let mut close = false;
        let mut ok = false;
        let r = egui::Modal::new(egui::Id::new("box_confirm_delete")).show(ctx, |ui| {
            ui.label(message.as_str());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if style::form_button(ui, "削除", style::ButtonKind::Important).clicked() {
                    ok = true;
                }
                if style::form_button(ui, "キャンセル", style::ButtonKind::Normal).clicked() {
                    close = true;
                }
            });
        });
        if ok {
            // 後ろから消して位置がずれないようにする
            let mut list = list.clone();
            list.sort_unstable_by(|a, b| b.cmp(a));
            list.dedup();
            for i in list {
                if i < shared.box_file.list.len() {
                    shared.box_file.list.remove(i);
                }
            }
            shared.save_box();
            self.results.clear();
            self.selected.clear();
        }
        if ok || close || r.should_close() {
            self.confirm_delete = None;
        }
    }

    /// 並べ替えの値。数値が無いときは 0(本家 getSortKey の `value || 0`)
    fn sort_value(&self, shared: &Shared, i: usize, key: &str) -> SortValue {
        let m = master();
        let p = &shared.box_file.list[i];
        let result = self.results.get(i).and_then(|r| r.as_ref());
        let base = m.pokemon(&p.name);
        let num = |v: Option<f64>| SortValue::Num(v.filter(|x| !x.is_nan()).unwrap_or(0.0));
        let text = |v: &str| SortValue::Str(kana_sort_key(v));
        match key {
            "index" => num(Some(i as f64)),
            // 本家: 除外(-1)は固定(1)の後ろに並べる
            "edit" => num(p.fix.map(|f| if f == -1.0 { 2.0 } else { f })),
            // 本家は並べ替えに図鑑の並び順(order)を使う
            "pokemonSeedNo" => num(base_of(p, result).and_then(seed_of).map(|b| b.order as f64)),
            "pokemonNo" => num(base_of(p, result).map(|b| b.order as f64)),
            "hitCheckList" => num(Some(result.map_or(0, |r| r.hit_check_list.len()) as f64)),
            "name" => text(&p.name),
            "memo" => text(p.memo.as_deref().unwrap_or("")),
            "lv" => num(Some(p.lv)),
            // 本家と同じく補正後のスキルLv
            "skillLv" => num(result.map(|r| r.pokemon.fixed_skill_lv)),
            "foodNameList" => text(&p.food_list.join(",")),
            "natureName" => text(&p.nature),
            "score" => num(result.map(|r| r.pokemon.score)),
            "candy" => num(base.and_then(|b| shared.config.candy.bag.get(&b.candy_name).copied())),
            "training" => num(p.training),
            "nextExp" => num(p.next_exp),
            "no" => num(base.map(|b| b.no as f64)),
            "afterList" => text(&result.map_or_else(|| base.map(|b| b.after_list.join(",")).unwrap_or_default(), |r| r.pokemon.after_list.join(","))),
            "berryName" => text(base.map(|b| m.berries[b.berry].name.as_str()).unwrap_or("")),
            "skillName" => text(base.map(|b| m.pokemon_skill(b).name.as_str()).unwrap_or("")),
            _ => {
                if let Some(slot) = SUB_SKILL_LV.iter().position(|lv| key == format!("subSkill{lv}")) {
                    let name = p.sub_skill_list.get(slot).map(|n| m.sub_skill(n).map(|x| x.short.as_str()).unwrap_or(n)).unwrap_or("");
                    return text(name);
                }
                if let Some(n) = CANDY_COLUMNS.iter().position(|(k, _)| *k == key) {
                    let r = calc_require_info(p, result.and_then(|r| r.pokemon.nature).map(|n| &m.natures[n]), &shared.config.candy);
                    return num(require_value(&r, n));
                }
                if let Some(n) = m.foods.iter().position(|f| f.name == key) {
                    return num(result.and_then(|r| r.pokemon.food_count.get(n).copied()));
                }
                if let Some((k, kind, lv)) = parse_eval_key(key) {
                    let v = result
                        .and_then(|r| r.evaluate_result.as_ref())
                        .and_then(|e| e.best.get(&lv))
                        .and_then(|b| b.get(EVAL_KINDS[k].0))
                        .map(|x| if kind { x.value } else { x.score });
                    return num(v);
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
                        self.selected.clear();
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

/// 右寄せの数値
fn right(ui: &mut egui::Ui, text: String) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(text);
    });
}

/// "energy_percentile_60" → (EVAL_KINDS の位置, 元値か, レベル)
fn parse_eval_key(key: &str) -> Option<(usize, bool, EvalLv)> {
    let mut it = key.split('_');
    let k = EVAL_KINDS.iter().position(|(x, _)| Some(*x) == it.next())?;
    let value = match it.next()? {
        "percentile" => false,
        "value" => true,
        _ => return None,
    };
    let lv = match it.next()? {
        "max" => EvalLv::Max,
        x => EvalLv::Lv(x.parse().ok()?),
    };
    Some((k, value, lv))
}

/// 名前(本家 name-label.vue): 進化を仮定したら「進化前→」、色違いは赤字、メモ列を出さないときは名前の下にメモ
fn name_cell(ui: &mut egui::Ui, p: &BoxPokemon, result: Option<&BoxResult>, memo_column: bool) {
    let m = master();
    let name = result.map_or(p.name.as_str(), |r| m.pokemon[r.pokemon.base].name.as_str());
    let mut text = String::new();
    if p.fix == Some(1.0) {
        text.push('📌');
    }
    let before = result.map(|r| r.before_name.as_str()).filter(|b| !b.is_empty() && *b != name);
    let mut main = egui::RichText::new(format!("{text}{name}{}", if p.favorite { "⭐" } else { "" }));
    if p.shiny {
        main = main.strong().color(egui::Color32::from_rgb(0xee, 0x55, 0x22));
    }
    let memo = p.memo.as_deref().filter(|x| !x.is_empty() && !memo_column);
    let name_line = |ui: &mut egui::Ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if let Some(b) = before {
                ui.label(egui::RichText::new(format!("{b}→")).size(10.0));
            }
            ui.label(main);
        });
    };
    match memo {
        Some(memo) => {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                name_line(ui);
                ui.label(egui::RichText::new(memo).size(10.0).color(style::MUTED));
            });
        }
        None => name_line(ui),
    }
}

/// 整理備考(本家 index.vue の hitCheckList)
fn hit_check_cell(ui: &mut egui::Ui, p: &BoxPokemon, result: Option<&BoxResult>, shiny_lock: bool) {
    let m = master();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        if p.favorite {
            ui.label("⭐お気に入り");
        }
        if p.shiny && shiny_lock {
            ui.label("✨色違い");
        }
        for hit in result.map_or(&[][..], |r| &r.hit_check_list[..]) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.label("✅");
                match hit {
                    CheckHit::Food(f) => {
                        food_icon::food_icon(ui, &m.foods[*f].name, 20.0).on_hover_text(&m.foods[*f].name);
                    }
                    _ => {
                        ui.label(hit.label());
                    }
                }
            });
        }
    });
}

/// 厳選の値(本家 evaluate-result.vue)
fn eval_cell(ui: &mut egui::Ui, result: Option<&BoxResult>, k: usize, lv: EvalLv, value: bool, detail: bool, pure_mint: bool) {
    let rows = eval_rows(result, k, lv, value, detail);
    let key = EVAL_KINDS[k].0;
    ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (after, v, best, diff) in rows {
            let text = v.map_or_else(|| "-".to_string(), |v| eval_text(v, key, value));
            let style = |t: egui::RichText| if best { t.strong() } else { t };
            // 増えたら赤、減ったら青
            let diff_label = |ui: &mut egui::Ui| {
                if let Some(d) = diff {
                    let color = if d > 0.0 { egui::Color32::RED } else if d < 0.0 { egui::Color32::BLUE } else { ui.visuals().text_color() };
                    ui.label(style(egui::RichText::new(diff_text(d, key, value)).size(10.0).color(color)));
                } else if pure_mint && detail {
                    ui.label("");
                }
            };
            if detail {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        diff_label(ui);
                        ui.add_sized([if key == "energy" && value { 52.0 } else { 46.0 }, 16.0], egui::Label::new(style(egui::RichText::new(text).size(11.0))));
                        ui.label(style(egui::RichText::new(after).size(10.0)));
                    });
                });
            } else {
                ui.label(style(egui::RichText::new(after).size(10.0)));
                ui.label(style(egui::RichText::new(text).size(11.0)));
                diff_label(ui);
            }
        }
    });
}

/// 表示設定(本家 index.vue の SettingButton「表示設定」)
fn display_setting(ui: &mut egui::Ui, shared: &mut Shared) {
    let mut s = ListSetting::load(shared);
    let mut candy = (shared.config.candy.boost_multiply, shared.config.candy.boost_shard);
    let mut changed = false;
    style::setting(ui, "box_display", "表示設定", &style::plain("表示設定"), false, |ui| {
        ui.set_max_width(560.0);
        egui::Grid::new("box_display_grid").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            let th = |ui: &mut egui::Ui, t: &str| {
                ui.vertical(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    style::th(ui, t);
                });
            };
            th(ui, "サブスキル名省略");
            changed |= ui.checkbox(&mut s.sub_skill_short, "サブスキル名省略").changed();
            ui.end_row();

            th(ui, "メモ");
            changed |= ui.checkbox(&mut s.memo, "メモを列として表示").changed();
            ui.end_row();

            th(ui, "厳選");
            ui.vertical(|ui| {
                for (v, l) in [(0.0, "非表示"), (1.0, "総合スコアのみ"), (2.0, "総合スコア＆とくい"), (3.0, "総合スコア＆きのみ＆食材＆スキル")] {
                    changed |= ui.radio_value(&mut s.evaluate_list, v, l).changed();
                }
                style::small(ui, "きのみはきのみの数、食材は食材の数、スキルは発動期待値で評価しています。", 300.0);
                ui.add_space(5.0);
                changed |= ui.checkbox(&mut s.select_energy, "厳選元値表示").changed();
                style::small(ui, "厳選度の場合はエナジー、とくい厳選度の場合はその分野の数量を表示するかどうかのオプションです。", 300.0);
                ui.add_space(5.0);
                changed |= ui.checkbox(&mut s.select_detail, "最適でない進化先の厳選値も表示").changed();
                style::small(ui, "進化先が複数あるポケモンについて、最も適正が高い進化先以外の結果もすべて表示します。", 300.0);
            });
            ui.end_row();

            th(ui, "アメ情報");
            ui.vertical(|ui| {
                changed |= ui.checkbox(&mut s.candy, "アメ情報").changed();
                ui.horizontal(|ui| {
                    ui.label("ブーストEXP倍率");
                    changed |= style::number(ui, &mut candy.0, 0.1, 50.0);
                    ui.label("倍");
                });
                ui.horizontal(|ui| {
                    ui.label("ブーストかけら倍率");
                    changed |= style::number(ui, &mut candy.1, 0.1, 50.0);
                    ui.label("倍");
                });
            });
            ui.end_row();

            th(ui, "基礎情報");
            changed |= ui.checkbox(&mut s.base_info, "基礎情報").changed();
            ui.end_row();

            th(ui, "食材数");
            changed |= ui.checkbox(&mut s.food_info, "食材数").changed();
            ui.end_row();

            th(ui, "シミュ詳細");
            changed |= ui.checkbox(&mut s.simulated_info, "シミュ詳細").changed();
            ui.end_row();

            th(ui, "表示設定");
            ui.vertical(|ui| {
                changed |= ui.checkbox(&mut s.fix_score, "スコアまで列固定").changed();
                ui.horizontal(|ui| {
                    ui.label("1ページ表示件数");
                    changed |= style::opt_number(ui, "box_page_unit", &mut s.page_unit, false, 50.0, "");
                    ui.label("件");
                });
            });
            ui.end_row();
        });
    });
    if changed {
        shared.config.candy.boost_multiply = candy.0;
        shared.config.candy.boost_shard = candy.1;
        s.save(shared);
    }
}

/// 操作列のアイコン(本家 index.vue の SVG)
#[derive(Clone, Copy)]
pub(super) enum Icon {
    Edit,
    Delete,
    Fix,
    Exclude,
    /// お気に入り(本家 star-icon.vue)
    Star,
}

/// アイコンのボタン(16×16)
pub(super) fn icon_button(ui: &mut egui::Ui, icon: Icon, color: egui::Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let color = if response.hovered() { color.gamma_multiply(0.7) } else { color };
    let p = ui.painter();
    // 本家の SVG は 100×100 の座標
    let at = |x: f32, y: f32| rect.min + egui::vec2(x, y) * (rect.width() / 100.0);
    let poly = |pts: &[(f32, f32)]| egui::Shape::convex_polygon(pts.iter().map(|&(x, y)| at(x, y)).collect(), color, egui::Stroke::NONE);
    match icon {
        Icon::Edit => {
            p.add(poly(&[(0.0, 100.0), (0.0, 80.0), (60.0, 20.0), (80.0, 40.0), (20.0, 100.0)]));
            p.add(poly(&[(65.0, 15.0), (80.0, 0.0), (100.0, 20.0), (85.0, 35.0)]));
        }
        Icon::Delete => {
            p.rect_filled(egui::Rect::from_min_max(at(10.0, 15.0), at(90.0, 30.0)), 0.0, color);
            p.rect_filled(egui::Rect::from_min_max(at(40.0, 0.0), at(60.0, 15.0)), 0.0, color);
            p.add(poly(&[(30.0, 100.0), (20.0, 40.0), (80.0, 40.0), (70.0, 100.0)]));
        }
        Icon::Star => {
            // 本家 star-icon.vue の頂点。凹多角形は塗れないので、中心から三角形に分けて塗る
            const PTS: [(f32, f32); 10] =
                [(50.0, 10.0), (61.8, 38.2), (92.4, 38.2), (67.6, 56.2), (77.3, 85.0), (50.0, 67.3), (22.7, 85.0), (32.4, 56.2), (7.6, 38.2), (38.2, 38.2)];
            let center = at(50.0, 52.0);
            for k in 0..PTS.len() {
                let (a, b) = (PTS[k], PTS[(k + 1) % PTS.len()]);
                p.add(egui::Shape::convex_polygon(vec![center, at(a.0, a.1), at(b.0, b.1)], color, egui::Stroke::new(0.5, color)));
            }
        }
        // 人の形(頭と体)。固定は右に＋、除外は斜線
        Icon::Fix | Icon::Exclude => {
            let (cx, w) = if matches!(icon, Icon::Fix) { (36.0, 64.0) } else { (50.0, 70.0) };
            p.circle_filled(at(cx, 30.0), rect.width() * 0.19, color);
            let body = egui::Rect::from_min_max(at(cx - w / 2.0, 58.0), at(cx + w / 2.0, 92.0));
            p.rect_filled(body, egui::CornerRadius { nw: 7, ne: 7, sw: 1, se: 1 }, color);
            if matches!(icon, Icon::Fix) {
                let stroke = egui::Stroke::new(2.0, color);
                p.line_segment([at(86.0, 34.0), at(86.0, 66.0)], stroke);
                p.line_segment([at(70.0, 50.0), at(100.0, 50.0)], stroke);
            } else {
                let bg = ui.visuals().extreme_bg_color;
                p.line_segment([at(4.0, 4.0), at(96.0, 96.0)], egui::Stroke::new(4.0, bg));
                p.line_segment([at(4.0, 4.0), at(96.0, 96.0)], egui::Stroke::new(2.0, color));
            }
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
