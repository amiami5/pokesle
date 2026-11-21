//! 厳選チェックリスト(本家 `models/check-list.ts`)
//!
//! 本家は呼び出しのたびに厳選テーブル全体から理論値を集計し直すが、
//! 結果は設定と厳選テーブルだけで決まるため `CheckList::new` で一度だけ求めておく。

use std::collections::HashMap;

use crate::config::{ChecklistConfig, Config};
use crate::data::master;
use crate::evaluate_setting::matches_pokemon_target;
use crate::evaluate_table::{EvalLv, EvaluateEntry, EvaluateResult, EvaluateTable};
use crate::js;

/// チェックリストの判定対象(本家 SimulatedPokemon のうち参照する部分)
#[derive(Debug, Clone, Copy)]
pub struct CheckTarget<'a> {
    /// `simulatedPokemon.base.name`
    pub base_name: &'a str,
    /// 食材構成("AAB" など)
    pub food_combination: &'a str,
    pub evaluate_result: &'a EvaluateResult,
}

/// チェックリストに該当した項目(本家 hitCheckList の要素)
#[derive(Debug, Clone, PartialEq)]
pub enum CheckHit {
    /// ポケモン別の厳選度条件。`condition` は条件の番号
    Pokemon { condition: usize, pokemon: String },
    /// 食材(マスターの添字)
    Food(usize),
    /// スキル(マスターの添字)
    Skill(usize),
}

impl CheckHit {
    pub fn label(&self) -> String {
        let m = master();
        match self {
            CheckHit::Pokemon { pokemon, .. } => format!("厳選度({pokemon})"),
            CheckHit::Food(f) => m.foods[*f].name.clone(),
            CheckHit::Skill(s) => m.skills[*s].name.clone(),
        }
    }
}

/// 判定対象1匹分のスコア(`index` は判定対象リスト内の位置)
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreEntry {
    pub index: usize,
    pub score: f64,
    pub lv: u32,
}

/// 厳選テーブル上の理論値
#[derive(Debug, Clone, PartialEq)]
pub struct BestEntry {
    /// ポケモン(マスターの添字)
    pub pokemon: usize,
    /// 食材構成("012" など。スキルは None)
    pub food_combination: Option<String>,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct PokemonCheckRow {
    /// 厳選対象のポケモン(マスターの添字)
    pub pokemon: usize,
    /// 適用された条件の番号
    pub condition: usize,
    /// 食材構成("全て" または "AAA/AAB" など)
    pub food_name: String,
    pub checked: bool,
    pub mint_checked: bool,
    /// 最も厳選度の高い判定対象
    pub hit: Option<usize>,
    /// 目標(改行区切り)
    pub target: String,
    pub energy_score: Option<f64>,
    pub specialty_score: Option<f64>,
    /// (判定対象の位置, 条件に対するスコア) の降順
    pub list: Vec<(usize, f64)>,
}

#[derive(Debug, Clone)]
pub struct FoodCheckRow {
    pub food: usize,
    pub best_food_score: f64,
    /// 料理1回に必要な最大数
    pub max_num: f64,
    pub border: f64,
    pub score: f64,
    pub lv: u32,
    pub checked: bool,
    pub mint_checked: bool,
    pub checked_pokemon_list: Vec<String>,
    pub hit: usize,
    pub rate: f64,
    /// 捕獲対象(理論値の降順)
    pub targets: Vec<BestEntry>,
    /// ボックス内の個数(降順)
    pub list: Vec<ScoreEntry>,
}

impl FoodCheckRow {
    /// 本家の note 列
    pub fn note(&self) -> String {
        let m = master();
        self.targets
            .iter()
            .map(|x| {
                let abc: String = x
                    .food_combination
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .map(|c| char::from_u32(c.to_digit(10).unwrap_or(0) + 65).unwrap_or('?'))
                    .collect();
                format!("{}{abc} ({})", m.pokemon[x.pokemon].name, js::to_fixed(x.score, 1))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone)]
pub struct SkillCheckRow {
    pub skill: usize,
    pub best_skill_score: f64,
    pub border: f64,
    pub score: f64,
    pub lv: u32,
    pub checked: bool,
    pub mint_checked: bool,
    pub checked_pokemon_list: Vec<String>,
    pub hit: usize,
    pub rate: f64,
    pub targets: Vec<BestEntry>,
    pub list: Vec<ScoreEntry>,
}

impl SkillCheckRow {
    pub fn note(&self) -> String {
        let m = master();
        self.targets
            .iter()
            .map(|x| format!("{} ({})", m.pokemon[x.pokemon].name, js::to_fixed(x.score, 1)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone)]
pub struct CheckListResult<R> {
    pub rows: Vec<R>,
    /// 未達成の項目で捕獲対象になるポケモン
    pub target_pokemon_list: Vec<String>,
}

impl<R> Default for CheckListResult<R> {
    fn default() -> Self {
        CheckListResult { rows: Vec::new(), target_pokemon_list: Vec::new() }
    }
}

pub struct CheckList {
    config: ChecklistConfig,
    lv_list: Vec<u32>,
    enabled: bool,
    /// (ポケモン, 条件番号)。本家 conditionAndPokemonMap の挿入順
    condition_pokemon: Vec<(usize, usize)>,
    /// 食材 → 理論値(降順)
    best_food: HashMap<usize, Vec<BestEntry>>,
    /// スキル → 理論値(降順)
    best_skill: HashMap<usize, Vec<BestEntry>>,
}

/// JS の文字列比較(UTF-16 のコード単位順)
fn cmp_js_str(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// JS の `list.at(i)`
fn js_at(list: &[f64], i: f64) -> Option<f64> {
    let i = if i.is_nan() { 0.0 } else { i.trunc() };
    let i = if i < 0.0 { list.len() as f64 + i } else { i };
    (i >= 0.0 && i < list.len() as f64).then(|| list[i as usize])
}

/// JS の `list[i]`(範囲外・非整数は undefined → NaN)
fn js_index(list: &[f64], i: f64) -> f64 {
    if i >= 0.0 && i.fract() == 0.0 && i < list.len() as f64 { list[i as usize] } else { f64::NAN }
}

impl CheckList {
    /// `table` が None のときは本家の `evaluateTable == null` と同じく全て空になる
    pub fn new(config: &Config, table: Option<&EvaluateTable>) -> CheckList {
        let m = master();
        let cl = config.summary.checklist.clone();
        let lv_list = config.select_evaluate.enabled_levels();

        // ポケモン別の条件: 最終進化かつフィールドに出るポケモンを名前順に並べ、後の条件で上書きする
        let mut last: Vec<usize> = m.pokemon.iter().filter(|p| p.is_last && !p.field_list.is_empty()).map(|p| p.index).collect();
        last.sort_by(|&a, &b| cmp_js_str(&m.pokemon[a].name, &m.pokemon[b].name));
        last.retain(|&p| !cl.pokemon_condition.disable_pokemon_map.get(&m.pokemon[p].name).copied().unwrap_or(false));
        let mut condition_pokemon: Vec<(usize, usize)> = Vec::new();
        let mut pos: HashMap<usize, usize> = HashMap::new();
        for (ci, item) in cl.pokemon_condition.list.iter().enumerate() {
            for &p in &last {
                if matches_pokemon_target(item.target.as_ref(), &m.pokemon[p]) {
                    match pos.get(&p) {
                        Some(&i) => condition_pokemon[i].1 = ci,
                        None => {
                            pos.insert(p, condition_pokemon.len());
                            condition_pokemon.push((p, ci));
                        }
                    }
                }
            }
        }

        // 理論値。本家は IndexedDB のキー `${name}_${lv}_${food}` 順に読み込んだ表を走査する
        let mut best_food: HashMap<usize, Vec<BestEntry>> = HashMap::new();
        let mut best_skill: HashMap<usize, Vec<BestEntry>> = HashMap::new();
        if let Some(table) = table {
            let mut names: Vec<&String> = table.pokemon.keys().collect();
            names.sort_by(|a, b| cmp_js_str(&format!("{a}_"), &format!("{b}_")));
            let lv_of = |x: Option<f64>| x.filter(|v| *v >= 0.0 && v.fract() == 0.0).map(|v| v as u32);
            let food_lv = lv_of(cl.food.border_lv);
            let skill_lv = lv_of(cl.skill.border_lv);
            for name in names {
                let Some(pokemon) = m.pokemon(name) else { continue };
                let by_lv = &table.pokemon[name];
                let sorted_foods = |lv: Option<u32>| {
                    let mut list: Vec<(&String, &crate::evaluate_table::PercentileSet)> =
                        lv.and_then(|lv| by_lv.get(&lv)).map(|x| x.iter().collect()).unwrap_or_default();
                    list.sort_by(|a, b| a.0.cmp(b.0));
                    list
                };

                for (food_combination, perc) in sorted_foods(food_lv) {
                    for (index, food) in pokemon.food_list.iter().enumerate() {
                        let list = match index {
                            0 => &perc.food1,
                            1 => &perc.food2,
                            _ => &perc.food3,
                        };
                        let Some(score) = js_at(list, cl.food.border_rate.unwrap_or(100.0)) else { continue };
                        if score == 0.0 {
                            continue;
                        }
                        best_food.entry(food.food).or_default().push(BestEntry {
                            pokemon: pokemon.index,
                            food_combination: Some(food_combination.clone()),
                            score,
                        });
                    }
                }

                let mut score = 0.0;
                for (_, perc) in sorted_foods(skill_lv) {
                    score = js::max(score, js_index(&perc.skill, cl.skill.border_rate.unwrap_or(100.0)));
                }
                best_skill.entry(pokemon.skill).or_default().push(BestEntry { pokemon: pokemon.index, food_combination: None, score });
            }
            for list in best_food.values_mut().chain(best_skill.values_mut()) {
                list.sort_by(|a, b| js::cmp_num(b.score, a.score));
            }
        }

        CheckList { config: cl, lv_list, enabled: table.is_some(), condition_pokemon, best_food, best_skill }
    }

    /// 本家 `pokemonCheckList`
    pub fn pokemon_check_list<'a>(
        &self,
        targets: &[CheckTarget<'a>],
        mut hit_list: Option<&mut Vec<CheckHit>>,
        food_check_list: Option<&CheckListResult<FoodCheckRow>>,
        skill_check_list: Option<&CheckListResult<SkillCheckRow>>,
    ) -> CheckListResult<PokemonCheckRow> {
        if !self.enabled {
            return CheckListResult::default();
        }
        let m = master();
        let select_lv = self.config.pokemon_condition.select_lv();
        let mut rows: Vec<PokemonCheckRow> = Vec::new();

        for &(p, ci) in &self.condition_pokemon {
            let pokemon = &m.pokemon[p];
            let item = &self.config.pokemon_condition.list[ci];
            let flags = item.food_flags();
            let food_list: Vec<&str> = flags.iter().filter(|(_, e)| *e).map(|(n, _)| *n).collect();
            let food_name = if food_list.len() == 6 { "全て".to_string() } else { food_list.join("/") };

            let entry_of = |t: &CheckTarget<'a>| -> Option<&'a EvaluateEntry> { select_lv.and_then(|lv| t.evaluate_result.get(lv, &pokemon.name)) };
            let mut list: Vec<(usize, f64)> = Vec::new();
            for (i, t) in targets.iter().enumerate() {
                let Some(entry) = entry_of(t) else { continue };
                let (Some(energy), Some(specialty)) = (&entry.energy, &entry.specialty) else { continue };
                if !(pokemon.kaihou && food_list.len() == 6) && !food_list.contains(&t.food_combination) {
                    continue;
                }
                // 厳選度チェック
                let mut score = f64::NEG_INFINITY;
                if let Some(b) = item.energy_border {
                    score = js::max(score, energy.score - b / 100.0);
                }
                if let Some(b) = item.specialty_border {
                    score = js::max(score, specialty.score - b / 100.0);
                }
                if score >= 0.0
                    && let Some(h) = hit_list.as_deref_mut()
                {
                    h.push(CheckHit::Pokemon { condition: ci, pokemon: pokemon.name.clone() });
                }
                list.push((i, score));
            }
            list.sort_by(|a, b| js::cmp_num(b.1, a.1));

            let in_checked = |rows: &[String]| rows.contains(&pokemon.name);
            let checked = list.first().is_some_and(|x| x.1 >= 0.0)
                || (item.also_food_and_skill
                    && (food_check_list.is_some_and(|f| f.rows.iter().any(|r| in_checked(&r.checked_pokemon_list)))
                        || skill_check_list.is_some_and(|s| s.rows.iter().any(|r| in_checked(&r.checked_pokemon_list)))));
            let hit = list.first().map(|x| x.0);
            let hit_entry = hit.and_then(|i| entry_of(&targets[i]));

            let target = [
                item.energy_border.map(|b| format!("総合{b}%以上")),
                item.specialty_border.map(|b| format!("とくい{b}%以上")),
                item.also_food_and_skill.then(|| "食材/スキル基準".to_string()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");

            rows.push(PokemonCheckRow {
                pokemon: p,
                condition: ci,
                food_name,
                checked,
                mint_checked: false,
                hit,
                target,
                energy_score: hit_entry.and_then(|e| e.energy.as_ref()).map(|x| x.score),
                specialty_score: hit_entry.and_then(|e| e.specialty.as_ref()).map(|x| x.score),
                list,
            });
        }

        if rows.iter().any(|x| m.pokemon[x.pokemon].name.starts_with("パンプジン") && x.checked) {
            for r in rows.iter_mut().filter(|x| m.pokemon[x.pokemon].name.starts_with("パンプジン")) {
                r.checked = true;
            }
        }

        rows.sort_by_key(|x| m.pokemon[x.pokemon].no);
        let target_pokemon_list = rows.iter().filter(|x| !x.checked).map(|x| m.pokemon[x.pokemon].name.clone()).collect();
        CheckListResult { rows, target_pokemon_list }
    }

    fn target_lv_list(&self, border_lv: Option<f64>) -> Vec<u32> {
        // null との比較(x <= null)は x <= 0 になる
        let border = border_lv.unwrap_or(0.0);
        self.lv_list.iter().copied().filter(|&x| x as f64 <= border).collect()
    }

    /// 本家 `foodCheckList`
    pub fn food_check_list(&self, targets: &[CheckTarget], mut hit_list: Option<&mut Vec<CheckHit>>) -> CheckListResult<FoodCheckRow> {
        if !self.enabled {
            return CheckListResult::default();
        }
        let m = master();
        let cfg = &self.config.food;

        // 事前整理
        let mut score_map: HashMap<usize, Vec<ScoreEntry>> = HashMap::new();
        for (i, t) in targets.iter().enumerate() {
            for lv in self.target_lv_list(cfg.border_lv) {
                for (name, entry) in t.evaluate_result.by_lv.get(&EvalLv::Lv(lv)).map(|x| x.as_slice()).unwrap_or(&[]) {
                    let Some(pokemon) = m.pokemon(name) else { continue };
                    for (index, food) in pokemon.food_list.iter().enumerate() {
                        let Some(&score) = entry.food_num_list.get(index) else { continue };
                        if score == 0.0 {
                            continue;
                        }
                        score_map.entry(food.food).or_default().push(ScoreEntry { index: i, score, lv });
                    }
                }
            }
        }

        let mut rows = Vec::new();
        for (f, food) in m.foods.iter().enumerate() {
            let (Some(best_list), Some(list)) = (self.best_food.get(&f), score_map.get_mut(&f)) else { continue };
            if !cfg.enable_map.get(&food.name).copied().unwrap_or(false) {
                continue;
            }
            list.sort_by(|a, b| js::cmp_num(b.score, a.score));

            let max_num = js::max_of(
                m.cookings.iter().map(|c| c.food_list.iter().find(|x| x.food == f).map(|x| x.num).unwrap_or(0.0)).chain([0.0]),
            );
            let best_food_score = if cfg.border_type == 0.0 { best_list.first().map(|x| x.score).unwrap_or(0.0) } else { max_num };
            let border = best_food_score * (cfg.border_value / 100.0);
            let target_border = best_food_score * (cfg.target_value / 100.0);

            let top = &list[0];
            let checked = top.score >= border;
            if checked && let Some(h) = hit_list.as_deref_mut() {
                h.push(CheckHit::Food(f));
            }
            rows.push(FoodCheckRow {
                food: f,
                best_food_score,
                max_num,
                border,
                score: top.score,
                lv: top.lv,
                checked,
                mint_checked: false,
                checked_pokemon_list: list.iter().filter(|x| x.score >= border).map(|x| targets[x.index].base_name.to_string()).collect(),
                hit: top.index,
                rate: top.score / best_food_score,
                targets: best_list.iter().filter(|x| x.score >= target_border).cloned().collect(),
                list: list.clone(),
            });
        }

        let target_pokemon_list =
            rows.iter().filter(|x| !x.checked).flat_map(|x| x.targets.iter().map(|t| m.pokemon[t.pokemon].name.clone())).collect();
        CheckListResult { rows, target_pokemon_list }
    }

    /// 本家 `skillCheckList`
    pub fn skill_check_list(&self, targets: &[CheckTarget], mut hit_list: Option<&mut Vec<CheckHit>>) -> CheckListResult<SkillCheckRow> {
        if !self.enabled {
            return CheckListResult::default();
        }
        let m = master();
        let cfg = &self.config.skill;

        let mut score_map: HashMap<usize, Vec<ScoreEntry>> = HashMap::new();
        for (i, t) in targets.iter().enumerate() {
            for lv in self.target_lv_list(cfg.border_lv) {
                for (name, entry) in t.evaluate_result.by_lv.get(&EvalLv::Lv(lv)).map(|x| x.as_slice()).unwrap_or(&[]) {
                    let Some(pokemon) = m.pokemon(name) else { continue };
                    let Some(skill) = &entry.skill else { continue };
                    score_map.entry(pokemon.skill).or_default().push(ScoreEntry { index: i, score: skill.value, lv });
                }
            }
        }

        let mut rows = Vec::new();
        for (s, skill) in m.skills.iter().enumerate() {
            if cfg.skill_specialty_only && !skill.skill_specialty_only {
                continue;
            }
            let (Some(best_list), Some(list)) = (self.best_skill.get(&s), score_map.get_mut(&s)) else { continue };
            if !cfg.enable_map.get(&skill.name).copied().unwrap_or(false) {
                continue;
            }
            list.sort_by(|a, b| js::cmp_num(b.score, a.score));

            let best_skill_score = best_list.first().map(|x| x.score).unwrap_or(0.0);
            let border = best_skill_score * (cfg.border_value / 100.0);
            let target_border = best_skill_score * (cfg.target_value / 100.0);

            let top = &list[0];
            let checked = top.score >= border;
            if checked && let Some(h) = hit_list.as_deref_mut() {
                h.push(CheckHit::Skill(s));
            }
            rows.push(SkillCheckRow {
                skill: s,
                best_skill_score,
                border,
                score: top.score,
                lv: top.lv,
                checked,
                mint_checked: false,
                checked_pokemon_list: list.iter().filter(|x| x.score >= border).map(|x| targets[x.index].base_name.to_string()).collect(),
                hit: top.index,
                rate: top.score / best_skill_score,
                targets: best_list.iter().filter(|x| x.score >= target_border).cloned().collect(),
                list: list.clone(),
            });
        }

        let target_pokemon_list =
            rows.iter().filter(|x| !x.checked).flat_map(|x| x.targets.iter().map(|t| m.pokemon[t.pokemon].name.clone())).collect();
        CheckListResult { rows, target_pokemon_list }
    }

    /// 本家 `getChecked`。1匹分の該当項目(食材 → スキル → ポケモンの順)
    pub fn get_checked(&self, target: &CheckTarget) -> Vec<CheckHit> {
        let mut result = Vec::new();
        let targets = std::slice::from_ref(target);
        let food = self.food_check_list(targets, Some(&mut result));
        let skill = self.skill_check_list(targets, Some(&mut result));
        self.pokemon_check_list(targets, Some(&mut result), Some(&food), Some(&skill));
        result
    }
}

pub trait CheckRow {
    fn key(&self) -> usize;
    fn checked(&self) -> bool;
    fn set_checked(&mut self, v: bool);
    fn set_mint_checked(&mut self, v: bool);
}

impl CheckRow for PokemonCheckRow {
    fn key(&self) -> usize {
        self.pokemon
    }
    fn checked(&self) -> bool {
        self.checked
    }
    fn set_checked(&mut self, v: bool) {
        self.checked = v;
    }
    fn set_mint_checked(&mut self, v: bool) {
        self.mint_checked = v;
    }
}

impl CheckRow for FoodCheckRow {
    fn key(&self) -> usize {
        self.food
    }
    fn checked(&self) -> bool {
        self.checked
    }
    fn set_checked(&mut self, v: bool) {
        self.checked = v;
    }
    fn set_mint_checked(&mut self, v: bool) {
        self.mint_checked = v;
    }
}

impl CheckRow for SkillCheckRow {
    fn key(&self) -> usize {
        self.skill
    }
    fn checked(&self) -> bool {
        self.checked
    }
    fn set_checked(&mut self, v: bool) {
        self.checked = v;
    }
    fn set_mint_checked(&mut self, v: bool) {
        self.mint_checked = v;
    }
}

/// 本家 `withMintChecked`: 最良値とボックスは両方を含め、厳選済の表記はミント前の該当を優先する。
pub fn with_mint_checked<R: CheckRow>(
    normal: &CheckListResult<R>,
    mint: &CheckListResult<R>,
    mut combined: CheckListResult<R>,
    exclude_mint_from_field: bool,
) -> CheckListResult<R> {
    let normal_checked: HashMap<usize, bool> = normal.rows.iter().map(|r| (r.key(), r.checked())).collect();
    let mint_checked: std::collections::HashSet<usize> = mint.rows.iter().filter(|r| r.checked()).map(|r| r.key()).collect();
    for row in &mut combined.rows {
        let k = row.key();
        row.set_checked(normal_checked.get(&k).copied().unwrap_or(false));
        row.set_mint_checked(mint_checked.contains(&k));
    }
    if !exclude_mint_from_field {
        combined.target_pokemon_list = normal.target_pokemon_list.clone();
    }
    combined
}

/// 本家 check-list.vue の食材構成("AAB" など)。見つからない食材は "@" になる
pub fn food_combination(base_food_name_list: &[String], food_list: &[String]) -> String {
    food_list
        .iter()
        .map(|f| {
            let i = base_food_name_list.iter().position(|x| x == f).map(|i| i as i64).unwrap_or(-1);
            char::from_u32((i + 65) as u32).unwrap_or('?')
        })
        .collect()
}

/// フィールド別画面のポケモン1匹
#[derive(Debug, Clone)]
pub struct FieldPokemon {
    pub name: String,
    /// 厳選テーブルの理論値(総合)の平均
    pub energy: f64,
}

/// フィールド別画面の1行(睡眠タイプは うとうと・すやすや・ぐっすり の順)
#[derive(Debug, Clone)]
pub struct FieldRow {
    pub name: String,
    pub pokemon: [Vec<FieldPokemon>; 3],
    /// そのフィールド・睡眠タイプに出る全ポケモンの数
    pub all_num: [usize; 3],
    pub rate: [f64; 3],
    pub energy: [f64; 3],
}

pub const SLEEP_TYPES: [&str; 3] = ["うとうと", "すやすや", "ぐっすり"];

/// 本家 check-list/field.vue。`target_pokemon_list` は各チェックリストの捕獲対象を合わせたもの
pub fn field_check_list(config: &Config, table: Option<&EvaluateTable>, target_pokemon_list: &[String]) -> Vec<FieldRow> {
    let m = master();
    let cl = &config.summary.checklist;
    let lv_list = config.select_evaluate.enabled_levels();
    let evaluate_lv = match cl.pokemon_condition.select_lv() {
        Some(EvalLv::Max) => lv_list.last().copied(),
        Some(EvalLv::Lv(lv)) => Some(lv),
        None => None,
    };

    let mut list: Vec<(String, &crate::data::Pokemon, f64)> = m
        .pokemon
        .iter()
        .filter(|p| (cl.field.shinkago || p.evolve.before.is_none()) && p.after_list.iter().any(|a| target_pokemon_list.contains(a)))
        .map(|p| {
            let energy_list: Vec<f64> = p
                .after_list
                .iter()
                .filter_map(|a| table.and_then(|t| t.pokemon.get(a)).and_then(|x| evaluate_lv.and_then(|lv| x.get(&lv))))
                .flat_map(|by_food| {
                    let mut foods: Vec<_> = by_food.iter().collect();
                    foods.sort_by(|a, b| a.0.cmp(b.0));
                    foods.into_iter().map(|(_, perc)| perc.energy.get(100).copied().unwrap_or(f64::NAN)).collect::<Vec<_>>()
                })
                .collect();
            let energy = energy_list.iter().fold(0.0, |a, x| a + x) / (energy_list.len().max(1) as f64);
            (p.name.clone(), p, energy)
        })
        .collect();

    if cl.field.bakeccha_matome && list.iter().any(|x| x.0.starts_with("バケッチャ")) {
        let first = list.iter().find(|x| x.0.starts_with("バケッチャ")).cloned();
        list.retain(|x| !x.0.starts_with("バケッチャ"));
        if let Some((_, p, energy)) = first {
            list.push(("バケッチャ".to_string(), p, energy));
        }
    }

    let in_field = |p: &crate::data::Pokemon, field: &str, ty: &str| p.field_list.iter().any(|f| f.name == field && f.sleep_type == ty);
    m.fields
        .iter()
        .map(|field| {
            let pokemon = SLEEP_TYPES.map(|ty| {
                list.iter().filter(|x| in_field(x.1, &field.name, ty)).map(|x| FieldPokemon { name: x.0.clone(), energy: x.2 }).collect::<Vec<_>>()
            });
            let all_num = SLEEP_TYPES.map(|ty| m.pokemon.iter().filter(|p| in_field(p, &field.name, ty)).count());
            let rate: [f64; 3] = std::array::from_fn(|i| pokemon[i].len() as f64 / all_num[i].max(1) as f64);
            let energy: [f64; 3] = std::array::from_fn(|i| {
                let l = &pokemon[i];
                if l.is_empty() { 0.0 } else { l.iter().fold(0.0, |a, x| a + x.energy) / l.len() as f64 * rate[i] }
            });
            FieldRow { name: field.name.clone(), pokemon, all_num, rate, energy }
        })
        .collect()
}
