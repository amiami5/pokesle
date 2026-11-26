//! 厳選テーブル(本家 `models/simulation/evaluate-table.ts`, `evaluate-simulator.ts`,
//! `models/sub-skill-combination-worker.ts`)

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::{Config, EvaluateConfig};
use crate::data::{master, Pokemon};
use crate::js;
use crate::sim_pokemon::BoxPokemon;
use crate::simulator::{Mode, PokemonSimulator};

// ---------------------------------------------------------------------------
// サブスキルの組み合わせ

/// 同じ効果になるサブスキルの組み合わせ。`weights[code]` は (件数, サブスキルID列)。
/// code は ゆめボ=1、リサボ=2 のビット和。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SubSkillCombination {
    pub s: Vec<u32>,
    pub weights: [Option<(f64, Vec<u32>)>; 4],
}

/// サブスキル数(1～5)ごとの組み合わせ一覧。添字0は未使用。
pub type SubSkillCombinationList = Vec<Vec<SubSkillCombination>>;

/// 本家 `sub-skill-combination-worker.ts`
pub fn sub_skill_combination_list(silver_seed: &HashMap<String, bool>) -> SubSkillCombinationList {
    let m = master();
    let n = m.sub_skills.len();
    let idx = |name: &str| m.sub_skills.iter().position(|s| s.name == name);

    // 銀種の置換手順(本家 useSilverSeed と同じ順)
    let steps: Vec<(usize, usize)> = [
        ("最大所持数アップM", "最大所持数アップL"),
        ("最大所持数アップS", "最大所持数アップM"),
        ("最大所持数アップM", "最大所持数アップL"),
        ("スキルレベルアップS", "スキルレベルアップM"),
        ("スキル確率アップS", "スキル確率アップM"),
        ("食材確率アップS", "食材確率アップM"),
        ("おてつだいスピードS", "おてつだいスピードM"),
    ]
    .iter()
    .filter(|(from, _)| silver_seed.get(*from).copied().unwrap_or(false))
    .filter_map(|(from, to)| Some((idx(from)?, idx(to)?)))
    .collect();

    let effect = |list: &[usize]| -> [i32; 8] {
        use crate::data::SubSkillKind::*;
        let mut e = [0i32; 8];
        for &i in list {
            match m.sub_skills[i].kind {
                BerryNumS => e[0] = 1,
                GenkiBonus => e[1] = 1,
                HelpBonus => e[2] = 1,
                SkillLvS => e[3] += 1,
                SkillLvM => e[3] += 2,
                SkillRateS => e[4] += 18,
                SkillRateM => e[4] += 36,
                FoodRateS => e[5] += 18,
                FoodRateM => e[5] += 36,
                SpeedS => e[6] += 7,
                SpeedM => e[6] += 14,
                BagS => e[7] += 6,
                BagM => e[7] += 12,
                BagL => e[7] += 18,
                _ => {}
            }
        }
        e
    };
    let yumebo = idx("ゆめのかけらボーナス");
    let risabo = idx("リサーチEXPボーナス");

    let mut result: Vec<(HashMap<[i32; 8], usize>, Vec<SubSkillCombination>)> = (0..=5).map(|_| (HashMap::new(), Vec::new())).collect();
    let mut list = [0usize; 5];
    for s1 in 0..n {
        for s2 in 0..n {
            if s1 == s2 {
                continue;
            }
            for s3 in 0..n {
                if s3 == s1 || s3 == s2 {
                    continue;
                }
                for s4 in 0..n {
                    if s4 == s1 || s4 == s2 || s4 == s3 {
                        continue;
                    }
                    for s5 in 0..n {
                        if s5 == s1 || s5 == s2 || s5 == s3 || s5 == s4 {
                            continue;
                        }
                        list = [s1, s2, s3, s4, s5];
                        for &(from, to) in &steps {
                            if list.contains(&from) && !list.contains(&to) {
                                let p = list.iter().position(|&x| x == from).expect("from");
                                list[p] = to;
                            }
                        }
                        for i in 1..=5 {
                            let sliced = &list[..i];
                            let key = effect(sliced);
                            let mut code = 0;
                            if yumebo.is_some_and(|y| sliced.contains(&y)) {
                                code += 1;
                            }
                            if risabo.is_some_and(|r| sliced.contains(&r)) {
                                code += 2;
                            }
                            let ids: Vec<u32> = sliced.iter().map(|&x| m.sub_skills[x].id).collect();
                            let (map, combos) = &mut result[i];
                            let pos = *map.entry(key).or_insert_with(|| {
                                combos.push(SubSkillCombination { s: ids.clone(), weights: Default::default() });
                                combos.len() - 1
                            });
                            let w = combos[pos].weights[code].get_or_insert((0.0, ids));
                            w.0 += 1.0;
                        }
                    }
                }
            }
        }
    }
    let _ = list;
    result.into_iter().map(|(_, combos)| combos).collect()
}

// ---------------------------------------------------------------------------
// 厳選テーブル

/// パーセンタイル表(0～100%、最大101要素)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PercentileSet {
    pub energy: Vec<f64>,
    pub berry: Vec<f64>,
    pub food: Vec<f64>,
    pub skill: Vec<f64>,
    #[serde(default)]
    pub food1: Vec<f64>,
    #[serde(default)]
    pub food2: Vec<f64>,
    #[serde(default)]
    pub food3: Vec<f64>,
}

impl PercentileSet {
    pub fn get(&self, key: &str) -> &[f64] {
        match key {
            "energy" => &self.energy,
            "berry" => &self.berry,
            "food" => &self.food,
            "skill" => &self.skill,
            "food1" => &self.food1,
            "food2" => &self.food2,
            "food3" => &self.food3,
            _ => &[],
        }
    }
}

/// 厳選テーブル全体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvaluateTable {
    /// 本家の evaluateTable バージョン(マスターの version.evaluate)
    pub version: u64,
    pub sleep_time: f64,
    pub check_freq: f64,
    /// レベルごとの回復役基準値
    pub healer: BTreeMap<u32, f64>,
    /// レベルごとのサポート役基準値
    pub support: BTreeMap<u32, f64>,
    /// ポケモン名 → レベル → 食材の組み合わせ("012" など) → パーセンタイル
    pub pokemon: HashMap<String, BTreeMap<u32, HashMap<String, PercentileSet>>>,
}

impl EvaluateTable {
    /// 本家 `isEnableEvaluateTable`
    pub fn is_enable(&self, config: &Config) -> bool {
        self.version == master().version.evaluate && self.sleep_time == config.sleep_time && self.check_freq == config.check_freq
    }
}

/// 1ポケモン・1食材構成分の評価結果
#[derive(Debug, Clone, Default)]
pub struct EvaluateOutput {
    pub percentile: PercentileSet,
    pub base_score: Option<f64>,
    pub pickup_energy_per_help: Option<f64>,
}

/// evaluate-simulator 1回分の出力
#[derive(Debug, Clone, Default)]
pub struct EvaluateWorkerResult {
    pub score_for_healer_evaluate_list: Vec<f64>,
    pub score_for_support_evaluate_list: Vec<f64>,
    /// ポケモン名 → 食材の組み合わせ → 結果(ポケモンは入力順)
    pub result: Vec<(String, Vec<(String, EvaluateOutput)>)>,
}

#[derive(Clone)]
struct Weight {
    yumebo: bool,
    risabo: bool,
    weight: f64,
}

struct EvalCombo {
    sub_skill_list: Vec<Option<usize>>,
    weights: Vec<Weight>,
}

fn prepare_eval_combos(combinations: &[SubSkillCombination]) -> (Vec<EvalCombo>, Vec<Option<usize>>, f64) {
    let m = master();
    let id_to_pos = |id: u32| m.sub_skills.iter().position(|s| s.id == id);
    let combos: Vec<EvalCombo> = combinations
        .iter()
        .map(|c| EvalCombo {
            sub_skill_list: c.s.iter().map(|&id| id_to_pos(id)).collect(),
            weights: c
                .weights
                .iter()
                .enumerate()
                .filter_map(|(code, w)| {
                    w.as_ref().map(|(count, _)| Weight { yumebo: code & 1 != 0, risabo: code & 2 != 0, weight: *count })
                })
                .collect(),
        })
        .collect();
    let mut nature_list: Vec<Option<usize>> =
        m.natures.iter().enumerate().filter(|(_, n)| n.good.is_some()).map(|(i, _)| Some(i)).collect();
    nature_list.push(None);
    let total_weight = combos.iter().fold(0.0, |a, c| a + c.weights.iter().fold(0.0, |a, x| a + x.weight)) * m.natures.len() as f64;
    (combos, nature_list, total_weight)
}

struct EvalContext<'a> {
    combos: &'a [EvalCombo],
    nature_list: &'a [Option<usize>],
    total_weight: f64,
    healer: f64,
    support: f64,
    support_border: f64,
}

#[derive(Default)]
struct EvalScratch {
    energy_list: Vec<[f64; 4]>,
    common_list: Vec<[f64; 7]>,
}

fn evaluate_single_food(
    lv: u32,
    pokemon: &Pokemon,
    food_name_list: &[String],
    ctx: &EvalContext<'_>,
    simulator: &mut PokemonSimulator,
    scratch: &mut EvalScratch,
    mut progress: impl FnMut(),
) -> Result<(EvaluateOutput, Vec<f64>, Vec<f64>), String> {
    let m = master();
    scratch.energy_list.clear();
    scratch.common_list.clear();
    let mut healer_scores = Vec::new();
    let mut support_scores = Vec::new();

    let mut simulated = simulator.from_evaluate(pokemon, lv as f64, food_name_list)?;
    let mut head_foods = [None; 3];
    for (slot, name) in pokemon.food_name_list.iter().take(3).enumerate() {
        head_foods[slot] = m.food_index(name);
    }

    for c in ctx.combos {
        for &nature in ctx.nature_list {
            let nature_weight = if nature.is_none() { 5.0 } else { 1.0 };
            simulator.select_evaluate(&mut simulated, &c.sub_skill_list, nature, ctx.healer, ctx.support)?;
            let r = &simulated;
            let food_n = |k: usize| head_foods[k].map(|f| r.food_count[f]).unwrap_or(0.0);
            if r.energy_per_day.is_nan() {
                return Err(format!("計算エラーが発生しました。({})", pokemon.name));
            }
            let mut common_weight = 0.0;
            for w in &c.weights {
                let raw = simulator.select_evaluate_to_score(r, w.yumebo, w.risabo, false);
                let score = simulator.select_evaluate_to_score(r, w.yumebo, w.risabo, true);
                scratch.energy_list.push([score, raw / r.average_help_rate, r.pickup_energy_per_help, w.weight * nature_weight]);
                common_weight += w.weight * nature_weight;
            }
            scratch.common_list.push([r.berry_num_per_day, r.food_num_per_day, r.skill_per_day, food_n(0), food_n(1), food_n(2), common_weight]);
            progress();
        }
    }

    let mut result = EvaluateOutput::default();
    scratch.energy_list.sort_by(|a, b| js::cmp_num(a[0], b[0]));
    let mut weight_sum = 0.0;
    let mut next_index = 0.0;
    for item in &scratch.energy_list {
        let next_weight_sum = weight_sum + item[3];
        while weight_sum <= next_index && next_index < next_weight_sum && result.percentile.energy.len() <= 100 {
            if result.percentile.energy.len() as f64 == ctx.support_border {
                healer_scores.push(item[1]);
                support_scores.push(item[2]);
                result.base_score = Some(item[1]);
                result.pickup_energy_per_help = Some(item[2]);
            }
            result.percentile.energy.push(item[0]);
            next_index = js::round((ctx.total_weight - 1.0) * result.percentile.energy.len() as f64 / 100.0);
        }
        weight_sum = next_weight_sum;
    }

    for index in 0..6 {
        scratch.common_list.sort_by(|a, b| js::cmp_num(a[index], b[index]));
        let target = match index {
            0 => &mut result.percentile.berry,
            1 => &mut result.percentile.food,
            2 => &mut result.percentile.skill,
            3 => &mut result.percentile.food1,
            4 => &mut result.percentile.food2,
            _ => &mut result.percentile.food3,
        };
        let mut weight_sum = 0.0;
        let mut next_index = 0.0;
        for item in &scratch.common_list {
            let next_weight_sum = weight_sum + item[6];
            while weight_sum <= next_index && next_index < next_weight_sum && target.len() <= 100 {
                target.push(item[index]);
                next_index = js::round((ctx.total_weight - 1.0) * target.len() as f64 / 100.0);
            }
            weight_sum = next_weight_sum;
        }
    }

    Ok((result, healer_scores, support_scores))
}

/// 本家 `evaluate-simulator.ts` の処理本体。`pokemon_list` を同じシミュレーターで順番に評価する。
pub fn evaluate_pokemon(
    config: &Config,
    lv: u32,
    pokemon_list: &[&Pokemon],
    food_combination_list: &[String],
    combinations: &[SubSkillCombination],
    score_for_healer_evaluate: f64,
    score_for_support_evaluate: f64,
    mut progress: impl FnMut(),
) -> Result<EvaluateWorkerResult, String> {
    let (combos, nature_list, total_weight) = prepare_eval_combos(combinations);
    let food_index_list_list: Vec<Vec<usize>> =
        food_combination_list.iter().map(|x| x.chars().map(|c| c.to_digit(10).unwrap_or(99) as usize).collect()).collect();

    let mut simulator = PokemonSimulator::new(config.clone(), Mode::Select);
    let mut scratch = EvalScratch::default();
    let ctx = EvalContext {
        combos: &combos,
        nature_list: &nature_list,
        total_weight,
        healer: score_for_healer_evaluate,
        support: score_for_support_evaluate,
        support_border: config.select_evaluate.support_border,
    };
    let mut out = EvaluateWorkerResult::default();

    for pokemon in pokemon_list {
        let mut per_food = Vec::new();
        for (fi, food_index_list) in food_index_list_list.iter().enumerate() {
            let Some(food_name_list) = food_index_list.iter().map(|&f| pokemon.food_name_list.get(f).cloned()).collect::<Option<Vec<String>>>()
            else {
                continue;
            };
            let (result, healers, supports) = evaluate_single_food(lv, pokemon, &food_name_list, &ctx, &mut simulator, &mut scratch, &mut progress)?;
            out.score_for_healer_evaluate_list.extend(healers);
            out.score_for_support_evaluate_list.extend(supports);
            per_food.push((food_combination_list[fi].clone(), result));
        }
        out.result.push((pokemon.name.clone(), per_food));
    }
    Ok(out)
}

/// 厳選テーブルの指定厳選度に当たる個体(本家 evaluate-table.vue の showDetail が evaluate-simulator から取り出す subSkillList / nature)
#[derive(Debug, Clone, PartialEq)]
pub struct EnergyDetail {
    pub sub_skill_list: Vec<String>,
    /// 性格(マスターの添字)。無補正は None
    pub nature: Option<usize>,
    /// テーブルに載っているエナジーの値
    pub score: f64,
}

/// 1ポケモン・1食材構成について、エナジーの `percentile` 番目(0～100)に当たるサブスキル・性格を求める。
/// `evaluate_pokemon` のエナジー側と同じ並べ方・同じ位置の取り方で、重みの内訳も覚えておく。
pub fn evaluate_energy_detail(
    config: &Config,
    table: &EvaluateTable,
    combination_list: &SubSkillCombinationList,
    pokemon_name: &str,
    lv: u32,
    food_key: &str,
    percentile: usize,
) -> Result<EnergyDetail, String> {
    let m = master();
    let pokemon = m.pokemon(pokemon_name).ok_or_else(|| format!("知らないポケモン: {pokemon_name}"))?;
    let healer = *table.healer.get(&lv).ok_or("このレベルの厳選テーブルがありません")?;
    let support = *table.support.get(&lv).ok_or("このレベルの厳選テーブルがありません")?;

    let sub_skill_num = crate::simulator::enabled_sub_skill_len(lv as f64);
    let combos: Vec<SubSkillCombination> = combination_list
        .get(sub_skill_num)
        .filter(|_| sub_skill_num > 0)
        .cloned()
        .unwrap_or_else(|| vec![SubSkillCombination { s: vec![], weights: [Some((1.0, vec![])), None, None, None] }]);

    let id_to_pos = |id: u32| m.sub_skills.iter().position(|s| s.id == id);
    let name_of = |id: u32| m.sub_skill_by_id(id).map(|s| s.name.clone());
    let mut nature_list: Vec<Option<usize>> = m.natures.iter().enumerate().filter(|(_, n)| n.good.is_some()).map(|(i, _)| Some(i)).collect();
    nature_list.push(None);

    let food_name_list = food_key
        .chars()
        .map(|c| pokemon.food_name_list.get(c.to_digit(10).unwrap_or(99) as usize).cloned())
        .collect::<Option<Vec<String>>>()
        .ok_or("食材の組み合わせが正しくありません")?;

    let total_weight = combos.iter().fold(0.0, |a, c| a + c.weights.iter().flatten().fold(0.0, |a, (w, _)| a + w)) * m.natures.len() as f64;
    let mut simulator = PokemonSimulator::new(config.clone(), Mode::Select);
    let mut simulated = simulator.from_evaluate(pokemon, lv as f64, &food_name_list)?;

    // (score, weight, サブスキルの組み合わせの添字, 重みの内訳の添字, 性格)
    let mut energy_list: Vec<(f64, f64, usize, usize, Option<usize>)> = Vec::new();
    for (ci, c) in combos.iter().enumerate() {
        let sub_skill_list: Vec<Option<usize>> = c.s.iter().map(|&id| id_to_pos(id)).collect();
        for &nature in &nature_list {
            let nature_weight = if nature.is_none() { 5.0 } else { 1.0 };
            simulator.select_evaluate(&mut simulated, &sub_skill_list, nature, healer, support)?;
            if simulated.energy_per_day.is_nan() {
                return Err(format!("計算エラーが発生しました。({})", pokemon.name));
            }
            for (code, w) in c.weights.iter().enumerate() {
                let Some((count, _)) = w else { continue };
                let score = simulator.select_evaluate_to_score(&simulated, code & 1 != 0, code & 2 != 0, true);
                energy_list.push((score, count * nature_weight, ci, code, nature));
            }
        }
    }

    energy_list.sort_by(|a, b| js::cmp_num(a.0, b.0));
    let mut weight_sum = 0.0;
    let mut next_index = 0.0;
    let mut len = 0usize;
    for &(score, weight, ci, code, nature) in &energy_list {
        let next_weight_sum = weight_sum + weight;
        while weight_sum <= next_index && next_index < next_weight_sum && len <= 100 {
            if len == percentile {
                let ids = combos[ci].weights[code].as_ref().map(|(_, ids)| ids.clone()).unwrap_or_default();
                return Ok(EnergyDetail { sub_skill_list: ids.into_iter().filter_map(name_of).collect(), nature, score });
            }
            len += 1;
            next_index = js::round((total_weight - 1.0) * len as f64 / 100.0);
        }
        weight_sum = next_weight_sum;
    }
    Err("指定の厳選度に当たる個体が見つかりません".into())
}

/// 進捗の通知先(0～1)
pub trait Progress: Sync {
    fn set(&self, value: f64, message: &str);
}

impl Progress for () {
    fn set(&self, _: f64, _: &str) {}
}

fn food_combination_list(lv: u32) -> Vec<String> {
    let list: &[&str] = if lv < 30 { &["0"] } else if lv < 60 { &["00", "01"] } else { &["000", "001", "002", "010", "011", "012"] };
    list.iter().map(|x| x.to_string()).collect()
}

/// `pokemon_list` を並列評価する(動的ワークスティーリングでスレッドごとにシミュレーターを使い回す)
fn evaluate_parallel(
    config: &Config,
    lv: u32,
    pokemon_list: &[&Pokemon],
    combinations: &[SubSkillCombination],
    healer: f64,
    support: f64,
    _thread_num: usize,
    progress: &(dyn Fn(usize) + Sync),
) -> Result<Vec<EvaluateWorkerResult>, String> {
    let foods = food_combination_list(lv);
    let (combos, nature_list, total_weight) = prepare_eval_combos(combinations);
    let food_index_list_list: Vec<Vec<usize>> =
        foods.iter().map(|x| x.chars().map(|c| c.to_digit(10).unwrap_or(99) as usize).collect()).collect();
    let ctx = EvalContext {
        combos: &combos,
        nature_list: &nature_list,
        total_weight,
        healer,
        support,
        support_border: config.select_evaluate.support_border,
    };

    pokemon_list
        .par_iter()
        .map_init(
            || (PokemonSimulator::new(config.clone(), Mode::Select), EvalScratch::default()),
            |(simulator, scratch), &pokemon| {
                let mut out = EvaluateWorkerResult::default();
                let mut per_food = Vec::new();
                for (fi, food_index_list) in food_index_list_list.iter().enumerate() {
                    let Some(food_name_list) = food_index_list.iter().map(|&f| pokemon.food_name_list.get(f).cloned()).collect::<Option<Vec<String>>>()
                    else {
                        continue;
                    };
                    let (result, healers, supports) = evaluate_single_food(lv, pokemon, &food_name_list, &ctx, simulator, scratch, || progress(1))?;
                    out.score_for_healer_evaluate_list.extend(healers);
                    out.score_for_support_evaluate_list.extend(supports);
                    per_food.push((foods[fi].clone(), result));
                }
                out.result.push((pokemon.name.clone(), per_food));
                Ok(out)
            },
        )
        .collect()
}

/// 本家 `EvaluateTable.simulation`。厳選テーブルを作成する。
pub fn simulate_evaluate_table(config: &Config, progress: &dyn Progress) -> Result<EvaluateTable, String> {
    let m = master();
    let se = &config.select_evaluate;
    let lv_list = se.enabled_levels();
    let thread_num = config.thread_num();

    progress.set(0.0, "サブスキルの組み合わせを準備しています…");
    let combination_list = cached_sub_skill_combinations(&se.silver_seed);

    let pokemon_list: Vec<&Pokemon> =
        m.pokemon.iter().filter(|p| p.after_list.len() == 1 && p.after_list[0] == p.name && !p.kaihou).collect();
    let is_support = |p: &&Pokemon| {
        let s = m.pokemon_skill(p);
        s.team || s.shard
    };
    let normal: Vec<&Pokemon> = pokemon_list.iter().copied().filter(|p| !is_support(p)).collect();
    let support_list: Vec<&Pokemon> = pokemon_list.iter().copied().filter(is_support).collect();

    let total_steps: usize = lv_list
        .iter()
        .map(|&lv| {
            let combos = combination_list.get(crate::simulator::enabled_sub_skill_len(lv as f64)).map(|x| x.len()).unwrap_or(1);
            pokemon_list.len() * food_combination_list(lv).len() * combos * 21
        })
        .sum::<usize>()
        .max(1);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let tick = |n: usize| {
        let d = done.fetch_add(n, std::sync::atomic::Ordering::Relaxed) + n;
        if d.is_multiple_of(2000) {
            progress.set(d as f64 / total_steps as f64, "厳選情報を作成しています…");
        }
    };

    let mut table = EvaluateTable {
        version: m.version.evaluate,
        sleep_time: config.sleep_time,
        check_freq: config.check_freq,
        ..Default::default()
    };

    for &lv in &lv_list {
        let sub_skill_num = crate::simulator::enabled_sub_skill_len(lv as f64);
        let combos: Vec<SubSkillCombination> = combination_list
            .get(sub_skill_num)
            .filter(|_| sub_skill_num > 0)
            .cloned()
            .unwrap_or_else(|| vec![SubSkillCombination { s: vec![], weights: [Some((1.0, vec![])), None, None, None] }]);

        progress.set(done.load(std::sync::atomic::Ordering::Relaxed) as f64 / total_steps as f64, &format!("Lv{lv}の通常ポケモンの厳選情報を作成しています…"));
        let normal_result = evaluate_parallel(config, lv, &normal, &combos, f64::NAN, f64::NAN, thread_num, &tick)?;

        let mut healer_list: Vec<f64> = normal_result.iter().flat_map(|x| x.score_for_healer_evaluate_list.iter().copied()).collect();
        let mut support_scores: Vec<f64> = normal_result.iter().flat_map(|x| x.score_for_support_evaluate_list.iter().copied()).collect();
        let take = |list: &mut Vec<f64>| {
            list.sort_by(|a, b| js::cmp_num(*b, *a));
            let n = (list.len() as f64 * se.support_rank_num).floor().max(0.0) as usize;
            list.truncate(n);
            list.iter().fold(0.0, |a, x| a + x) / list.len() as f64
        };
        let healer = take(&mut healer_list);
        let support = take(&mut support_scores);
        table.healer.insert(lv, healer);
        table.support.insert(lv, support);

        progress.set(done.load(std::sync::atomic::Ordering::Relaxed) as f64 / total_steps as f64, &format!("Lv{lv}のサポート系スキルポケモンの厳選情報を作成しています…"));
        let support_result = evaluate_parallel(config, lv, &support_list, &combos, healer, support, thread_num, &tick)?;

        for chunk in normal_result.into_iter().chain(support_result) {
            for (name, per_food) in chunk.result {
                let by_lv = table.pokemon.entry(name).or_default();
                let by_food = by_lv.entry(lv).or_default();
                for (food, r) in per_food {
                    by_food.insert(food, r.percentile);
                }
            }
        }
    }
    progress.set(1.0, "完了");
    Ok(table)
}

/// 本家 `EvaluateTable.simulateTemporary`。指定ポケモンの進化先だけの簡易診断表を作る。
pub fn simulate_temporary(config: &Config, pokemon_name: &str, progress: &dyn Progress) -> Result<EvaluateTable, String> {
    let m = master();
    let mut fixed = config.clone();
    let mut evaluate_config: EvaluateConfig = config.tmp_evaluate.clone();
    evaluate_config.level_list = [(10, false), (25, false), (30, true), (50, true), (60, true), (70, true), (80, true)].into_iter().collect();
    fixed.select_evaluate = evaluate_config.clone();

    let base = m.pokemon(pokemon_name).ok_or_else(|| format!("知らないポケモン: {pokemon_name}"))?;
    let pokemon_list: Vec<&Pokemon> = base.after_list.iter().filter_map(|n| m.pokemon(n)).collect();
    let lv_list = [30u32, 50, 60, 70, 80];
    let mut table = EvaluateTable {
        version: m.version.evaluate,
        sleep_time: config.sleep_time,
        check_freq: config.check_freq,
        ..Default::default()
    };
    if base.kaihou && config.kaihou_evaluate {
        for &lv in &lv_list {
            let level_rate = 1.025_f64.powf(lv as f64 - 1.0) / (1.0 - 0.002 * (lv as f64 - 1.0));
            let healer = evaluate_config.score_for_healer_evaluate.unwrap_or(f64::NAN) * level_rate;
            let support = evaluate_config.score_for_support_evaluate.unwrap_or(f64::NAN) * level_rate;
            table.healer.insert(lv, healer);
            table.support.insert(lv, support);
        }
        progress.set(1.0, "完了");
        return Ok(table);
    }

    progress.set(0.0, "サブスキルの組み合わせを準備しています…");
    let combination_list = cached_sub_skill_combinations(&evaluate_config.silver_seed);

    // 5レベル × 進化先 × 食材構成を並列評価する
    struct TmpTask<'a> {
        lv: u32,
        pokemon: &'a Pokemon,
        food_key: String,
        food_names: Vec<String>,
        sub_skill_num: usize,
        healer: f64,
        support: f64,
    }
    let mut prepared: Vec<Option<(Vec<EvalCombo>, Vec<Option<usize>>, f64)>> = (0..combination_list.len()).map(|_| None).collect();
    let mut tasks: Vec<TmpTask<'_>> = Vec::new();
    for &lv in &lv_list {
        let foods: Vec<String> = if lv < 60 { vec!["00".into(), "01".into()] } else { food_combination_list(lv) };
        let sub_skill_num = crate::simulator::enabled_sub_skill_len(lv as f64);
        if prepared[sub_skill_num].is_none() {
            prepared[sub_skill_num] = Some(prepare_eval_combos(&combination_list[sub_skill_num]));
        }
        let level_rate = 1.025_f64.powf(lv as f64 - 1.0) / (1.0 - 0.002 * (lv as f64 - 1.0));
        let healer = evaluate_config.score_for_healer_evaluate.unwrap_or(f64::NAN) * level_rate;
        let support = evaluate_config.score_for_support_evaluate.unwrap_or(f64::NAN) * level_rate;
        table.healer.insert(lv, healer);
        table.support.insert(lv, support);
        for &pokemon in &pokemon_list {
            for food_key in &foods {
                let Some(food_names) = food_key
                    .chars()
                    .map(|c| pokemon.food_name_list.get(c.to_digit(10).unwrap_or(99) as usize).cloned())
                    .collect::<Option<Vec<String>>>()
                else {
                    continue;
                };
                tasks.push(TmpTask { lv, pokemon, food_key: food_key.clone(), food_names, sub_skill_num, healer, support });
            }
        }
    }

    let total_tasks = tasks.len().max(1);
    let done_tasks = std::sync::atomic::AtomicUsize::new(0);
    let support_border = fixed.select_evaluate.support_border;
    let results: Result<Vec<(String, u32, String, PercentileSet)>, String> = tasks
        .par_iter()
        .map_init(
            || (PokemonSimulator::new(fixed.clone(), Mode::Select), EvalScratch::default()),
            |(simulator, scratch), task| {
                let (combos, nature_list, total_weight) = prepared[task.sub_skill_num].as_ref().expect("prepared");
                let ctx = EvalContext {
                    combos,
                    nature_list,
                    total_weight: *total_weight,
                    healer: task.healer,
                    support: task.support,
                    support_border,
                };
                let (out, ..) = evaluate_single_food(task.lv, task.pokemon, &task.food_names, &ctx, simulator, scratch, || {})?;
                let mut p = out.percentile;
                p.food1.clear();
                p.food2.clear();
                p.food3.clear();
                let d = done_tasks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                progress.set(d as f64 / total_tasks as f64 * 0.85, "簡易診断を計算しています…");
                Ok((task.pokemon.name.clone(), task.lv, task.food_key.clone(), p))
            },
        )
        .collect();
    for (name, lv, food, p) in results? {
        table.pokemon.entry(name).or_default().entry(lv).or_default().insert(food, p);
    }
    progress.set(1.0, "完了");
    Ok(table)
}

/// かいほうポケモン(ミュウ・ダークライ)のオンデマンド厳選テーブル:
/// ポケモン名 → レベル → 食材キー("012" や未解放枠を含む "01_") → パーセンタイル
pub type KaihouOverlay = HashMap<String, BTreeMap<u32, HashMap<String, PercentileSet>>>;

type ComboCacheMap = HashMap<Vec<(String, bool)>, Arc<SubSkillCombinationList>>;

fn cached_sub_skill_combinations(silver_seed: &HashMap<String, bool>) -> Arc<SubSkillCombinationList> {
    static CACHE: OnceLock<Mutex<ComboCacheMap>> = OnceLock::new();
    let mut key: Vec<(String, bool)> = silver_seed.iter().map(|(k, &v)| (k.clone(), v)).collect();
    key.sort();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().expect("combo cache").get(&key) {
        return Arc::clone(hit);
    }
    let built = Arc::new(sub_skill_combination_list(silver_seed));
    let mut guard = cache.lock().expect("combo cache");
    Arc::clone(guard.entry(key).or_insert(built))
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct KaihouCacheKey {
    sleep_time: u64,
    check_freq: u64,
    select_evaluate: String,
    healer: u64,
    support: u64,
    pokemon: String,
    lv: u32,
    foods: Vec<String>,
}

const KAIHOU_CACHE_MAX: usize = 4096;

/// ボックス内の `kaihou` ポケモンについて、所持している食材構成だけをオンデマンドで厳選評価する
pub fn build_kaihou_overlay(config: &Config, box_list: &[BoxPokemon], table: &EvaluateTable) -> Result<KaihouOverlay, String> {
    if !config.kaihou_evaluate {
        return Ok(HashMap::new());
    }
    let m = master();
    let lv_list = config.select_evaluate.enabled_levels();

    // (after, lv, food_key) -> food_names
    let mut needed: BTreeSet<(String, u32, String, Vec<String>)> = BTreeSet::new();
    for bx in box_list {
        let Some(original_base) = m.pokemon(&bx.name) else { continue };
        if !original_base.kaihou {
            continue;
        }
        if bx.food_list.first().is_none_or(|f| !original_base.food_name_list.contains(f) || m.food(f).is_none()) {
            continue;
        }
        for after in &original_base.after_list {
            if table.pokemon.contains_key(after) {
                continue;
            }
            for &lv in &lv_list {
                if !table.healer.contains_key(&lv) || !table.support.contains_key(&lv) {
                    continue;
                }
                let food_num = if lv < 30 { 1 } else if lv < 60 { 2 } else { 3 };
                let food_names: Vec<String> = (0..food_num)
                    .map(|k| bx.food_list.get(k).filter(|f| original_base.food_name_list.contains(f)).cloned().unwrap_or_default())
                    .collect();
                let food_key: String = (0..food_num)
                    .map(|k| original_base.food_name_list.iter().position(|x| x == &food_names[k]).map_or_else(|| "_".to_string(), |i| i.to_string()))
                    .collect();
                needed.insert((after.clone(), lv, food_key, food_names));
            }
        }
    }
    if needed.is_empty() {
        return Ok(HashMap::new());
    }

    static CACHE: OnceLock<Mutex<HashMap<KaihouCacheKey, PercentileSet>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let select_evaluate_json = serde_json::to_string(&config.select_evaluate).unwrap_or_default();

    let mut overlay: KaihouOverlay = HashMap::new();
    let mut misses: Vec<(String, u32, String, Vec<String>, KaihouCacheKey)> = Vec::new();
    {
        let guard = cache.lock().expect("kaihou cache");
        for (after, lv, food_key, food_names) in needed {
            let healer = table.healer[&lv];
            let support = table.support[&lv];
            let key = KaihouCacheKey {
                sleep_time: config.sleep_time.to_bits(),
                check_freq: config.check_freq.to_bits(),
                select_evaluate: select_evaluate_json.clone(),
                healer: healer.to_bits(),
                support: support.to_bits(),
                pokemon: after.clone(),
                lv,
                foods: food_names.clone(),
            };
            if let Some(perc) = guard.get(&key) {
                overlay.entry(after).or_default().entry(lv).or_default().insert(food_key, perc.clone());
            } else {
                misses.push((after, lv, food_key, food_names, key));
            }
        }
    }
    if misses.is_empty() {
        return Ok(overlay);
    }

    let combination_list = cached_sub_skill_combinations(&config.select_evaluate.silver_seed);
    let computed: Vec<(String, u32, String, KaihouCacheKey, PercentileSet)> = misses
        .into_par_iter()
        .map(|(after, lv, food_key, food_names, key)| {
            let pokemon = m.pokemon(&after).ok_or_else(|| format!("知らないポケモン: {after}"))?;
            let sub_skill_num = crate::simulator::enabled_sub_skill_len(lv as f64);
            let fallback;
            let raw_combos: &[SubSkillCombination] = match combination_list.get(sub_skill_num).filter(|_| sub_skill_num > 0) {
                Some(c) => c,
                None => {
                    fallback = [SubSkillCombination { s: vec![], weights: [Some((1.0, vec![])), None, None, None] }];
                    &fallback
                }
            };
            let (combos, nature_list, total_weight) = prepare_eval_combos(raw_combos);
            let ctx = EvalContext {
                combos: &combos,
                nature_list: &nature_list,
                total_weight,
                healer: table.healer[&lv],
                support: table.support[&lv],
                support_border: config.select_evaluate.support_border,
            };
            let mut simulator = PokemonSimulator::new(config.clone(), Mode::Select);
            let mut scratch = EvalScratch::default();
            let (out, ..) = evaluate_single_food(lv, pokemon, &food_names, &ctx, &mut simulator, &mut scratch, || {})?;
            Ok((after, lv, food_key, key, out.percentile))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut guard = cache.lock().expect("kaihou cache");
    if guard.len() + computed.len() > KAIHOU_CACHE_MAX {
        guard.clear();
    }
    for (after, lv, food_key, key, perc) in computed {
        guard.insert(key, perc.clone());
        overlay.entry(after).or_default().entry(lv).or_default().insert(food_key, perc);
    }
    Ok(overlay)
}

/// 1件のかいほうポケモン用パーセンタイルを取得(キャッシュにあれば即返し、なければ計算)
pub fn evaluate_kaihou_percentile(config: &Config, table: &EvaluateTable, pokemon_name: &str, lv: u32, food_names: &[String]) -> Option<PercentileSet> {
    if !config.kaihou_evaluate {
        return None;
    }
    let m = master();
    let pokemon = m.pokemon(pokemon_name)?;
    if !pokemon.kaihou {
        return None;
    }
    let food_num = if lv < 30 { 1 } else if lv < 60 { 2 } else { 3 };
    let foods: Vec<String> = (0..food_num)
        .map(|k| food_names.get(k).filter(|f| pokemon.food_name_list.contains(f)).cloned().unwrap_or_default())
        .collect();
    if foods.first().is_none_or(|f| m.food(f).is_none()) {
        return None;
    }
    let bx = BoxPokemon { name: pokemon.name.clone(), food_list: foods.clone(), ..Default::default() };
    let overlay = build_kaihou_overlay(config, &[bx], table).ok()?;
    let food_key: String = (0..food_num)
        .map(|k| pokemon.food_name_list.iter().position(|x| x == &foods[k]).map_or_else(|| "_".to_string(), |i| i.to_string()))
        .collect();
    overlay.get(pokemon_name)?.get(&lv)?.get(&food_key).cloned()
}

// ---------------------------------------------------------------------------
// ボックスのポケモンの厳選結果

/// 厳選結果のレベル(`max` は全レベルの最大)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvalLv {
    Lv(u32),
    Max,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalInfo {
    pub name: Option<String>,
    pub rate: f64,
    pub ratio: f64,
    pub score: f64,
    pub value: f64,
    pub pure_mint: Option<Box<EvalInfo>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub berry_energy: f64,
    pub food_num_map: Vec<(String, f64)>,
    pub skill_activation_num: f64,
    pub pure_mint: Option<Box<Diagnostic>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluateEntry {
    pub energy: Option<EvalInfo>,
    pub berry: Option<EvalInfo>,
    pub food: Option<EvalInfo>,
    pub skill: Option<EvalInfo>,
    pub specialty: Option<EvalInfo>,
    pub food_num_list: Vec<f64>,
    pub diagnostic: Option<Diagnostic>,
}

impl EvaluateEntry {
    pub fn get(&self, key: &str) -> Option<&EvalInfo> {
        match key {
            "energy" => self.energy.as_ref(),
            "berry" => self.berry.as_ref(),
            "food" => self.food.as_ref(),
            "skill" => self.skill.as_ref(),
            "specialty" => self.specialty.as_ref(),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> &mut Option<EvalInfo> {
        match key {
            "energy" => &mut self.energy,
            "berry" => &mut self.berry,
            "food" => &mut self.food,
            "skill" => &mut self.skill,
            _ => &mut self.specialty,
        }
    }
}

/// レベル → 進化先 → 評価(進化先は挿入順)、および各レベルで最も良い進化先
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluateResult {
    pub by_lv: BTreeMap<EvalLv, Vec<(String, EvaluateEntry)>>,
    pub best: BTreeMap<EvalLv, EvaluateEntry>,
}

impl EvaluateResult {
    pub fn has_lv(&self, lv: EvalLv) -> bool {
        self.by_lv.contains_key(&lv)
    }

    pub fn names(&self, lv: EvalLv) -> Vec<String> {
        self.by_lv.get(&lv).map(|l| l.iter().map(|(n, _)| n.clone()).collect()).unwrap_or_default()
    }

    pub fn clear_lv(&mut self, lv: EvalLv) {
        self.by_lv.remove(&lv);
    }

    pub fn get(&self, lv: EvalLv, name: &str) -> Option<&EvaluateEntry> {
        self.by_lv.get(&lv).and_then(|l| l.iter().find(|(n, _)| n == name)).map(|(_, e)| e)
    }

    pub fn entry_mut(&mut self, lv: EvalLv, name: &str) -> &mut EvaluateEntry {
        let list = self.by_lv.entry(lv).or_default();
        let pos = match list.iter().position(|(n, _)| n == name) {
            Some(p) => p,
            None => {
                list.push((name.to_string(), EvaluateEntry::default()));
                list.len() - 1
            }
        };
        &mut list[pos].1
    }

    /// 指定した進化先だけを残したもの
    pub fn only(&self, name: &str, lv_list: &[u32]) -> EvaluateResult {
        let mut out = EvaluateResult::default();
        for lv in lv_list.iter().map(|&l| EvalLv::Lv(l)).chain([EvalLv::Max]) {
            if let Some(e) = self.get(lv, name) {
                out.by_lv.insert(lv, vec![(name.to_string(), e.clone())]);
            }
        }
        out
    }

    /// 各レベルごとに最適な進化先のスコアを求める
    pub fn compute_best(&mut self, lv_list: &[u32], pure_mint: bool) {
        for lv in lv_list.iter().map(|&l| EvalLv::Lv(l)).chain([EvalLv::Max]) {
            let Some(list) = self.by_lv.get(&lv) else { continue };
            let mut best = EvaluateEntry::default();
            for key in ["energy", "berry", "food", "skill", "specialty"] {
                let score_of = |e: &EvaluateEntry| -> f64 {
                    let info = e.get(key);
                    if pure_mint { info.and_then(|x| x.pure_mint.as_ref()).map(|x| x.score) } else { info.map(|x| x.score) }.unwrap_or(f64::NAN)
                };
                let mut sorted: Vec<&(String, EvaluateEntry)> = list.iter().collect();
                sorted.sort_by(|a, b| js::cmp_num(score_of(&b.1), score_of(&a.1)));
                if let Some((name, entry)) = sorted.first() {
                    let info = entry.get(key).and_then(|x| if pure_mint { x.pure_mint.as_deref().cloned() } else { Some(x.clone()) });
                    *best.get_mut(key) = info.map(|mut x| {
                        x.name = Some(name.clone());
                        x
                    });
                }
            }
            self.best.insert(lv, best);
        }
    }
}
