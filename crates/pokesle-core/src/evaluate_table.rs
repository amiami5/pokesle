//! 厳選テーブル(本家 `models/simulation/evaluate-table.ts`, `evaluate-simulator.ts`,
//! `models/sub-skill-combination-worker.ts`)

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::{Config, EvaluateConfig};
use crate::data::{master, NatureStat, Pokemon};
use crate::js;
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
                            let w = combos[pos].weights[code].get_or_insert_with(|| (0.0, ids));
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

struct Weight {
    yumebo: bool,
    risabo: bool,
    weight: f64,
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
    let m = master();
    let id_to_pos = |id: u32| m.sub_skills.iter().position(|s| s.id == id);
    let combos: Vec<(Vec<Option<usize>>, Vec<Weight>, Vec<String>)> = combinations
        .iter()
        .map(|c| {
            let sub_skill_list: Vec<Option<usize>> = c.s.iter().map(|&id| id_to_pos(id)).collect();
            let names: Vec<String> = c.s.iter().filter_map(|&id| m.sub_skill_by_id(id).map(|s| s.name.clone())).collect();
            let weights = c
                .weights
                .iter()
                .enumerate()
                .filter_map(|(code, w)| {
                    w.as_ref().map(|(count, _)| Weight { yumebo: code & 1 != 0, risabo: code & 2 != 0, weight: *count })
                })
                .collect();
            (sub_skill_list, weights, names)
        })
        .collect();

    let mut nature_list: Vec<Option<usize>> =
        m.natures.iter().enumerate().filter(|(_, n)| n.good.is_some()).map(|(i, _)| Some(i)).collect();
    nature_list.push(None);

    let total_weight = combos.iter().fold(0.0, |a, (_, w, _)| a + w.iter().fold(0.0, |a, x| a + x.weight)) * m.natures.len() as f64;
    let food_index_list_list: Vec<Vec<usize>> =
        food_combination_list.iter().map(|x| x.chars().map(|c| c.to_digit(10).unwrap_or(99) as usize).collect()).collect();

    let mut simulator = PokemonSimulator::new(config.clone(), Mode::Select);
    let se = &config.select_evaluate;
    let mut out = EvaluateWorkerResult::default();

    for pokemon in pokemon_list {
        let mut per_food = Vec::new();
        for (fi, food_index_list) in food_index_list_list.iter().enumerate() {
            let Some(food_name_list) = food_index_list.iter().map(|&f| pokemon.food_name_list.get(f).cloned()).collect::<Option<Vec<String>>>()
            else {
                continue;
            };

            // (score, baseScore, pickupEnergyPerHelp, weight)
            let mut energy_list: Vec<[f64; 4]> = Vec::new();
            // berry, food, skill, food1, food2, food3, weight
            let mut common_list: Vec<[f64; 7]> = Vec::new();

            let mut simulated = simulator.from_evaluate(pokemon, lv as f64, &food_name_list)?;
            let head_foods: Vec<Option<usize>> = pokemon.food_name_list.iter().take(3).map(|n| m.food_index(n)).collect();

            for (sub_skill_list, weights, names) in &combos {
                let suimin = names.iter().any(|x| x == "睡眠EXPボーナス");
                for &nature in &nature_list {
                    let nature_weight = if nature.is_none() { 5.0 } else { 1.0 };
                    simulator.select_evaluate(&mut simulated, sub_skill_list, nature, score_for_healer_evaluate, score_for_support_evaluate)?;
                    let r = &simulated;
                    let food_n = |k: usize| head_foods.get(k).copied().flatten().map(|f| r.food_count[f]).unwrap_or(0.0);
                    if r.energy_per_day.is_nan() {
                        return Err(format!("計算エラーが発生しました。({})", pokemon.name));
                    }
                    let nature_ref = nature.map(|i| &m.natures[i]);
                    let mut common_weight = 0.0;
                    for w in weights {
                        let mut score = simulator.select_evaluate_to_score(r, w.yumebo, w.risabo);
                        let raw = score;
                        if suimin {
                            score += se.sub_skill.suimin_exp_bonus.add;
                            score += raw * (se.sub_skill.suimin_exp_bonus.rate - 1.0);
                        }
                        if nature_ref.is_some_and(|n| n.good_stat == Some(NatureStat::Exp)) {
                            score += se.nature.exp_up.add;
                            score += raw * (se.nature.exp_up.rate - 1.0);
                        }
                        if nature_ref.is_some_and(|n| n.weak_stat == Some(NatureStat::Exp)) {
                            score += se.nature.exp_down.add;
                            score += raw * (se.nature.exp_down.rate - 1.0);
                        }
                        energy_list.push([score, raw / r.average_help_rate, r.pickup_energy_per_help, w.weight * nature_weight]);
                        common_weight += w.weight * nature_weight;
                    }
                    common_list.push([r.berry_num_per_day, r.food_num_per_day, r.skill_per_day, food_n(0), food_n(1), food_n(2), common_weight]);
                    progress();
                }
            }

            let mut result = EvaluateOutput::default();
            energy_list.sort_by(|a, b| js::cmp_num(a[0], b[0]));
            let mut weight_sum = 0.0;
            let mut next_index = 0.0;
            for item in &energy_list {
                let next_weight_sum = weight_sum + item[3];
                while weight_sum <= next_index && next_index < next_weight_sum && result.percentile.energy.len() <= 100 {
                    if result.percentile.energy.len() as f64 == se.support_border {
                        out.score_for_healer_evaluate_list.push(item[1]);
                        out.score_for_support_evaluate_list.push(item[2]);
                        result.base_score = Some(item[1]);
                        result.pickup_energy_per_help = Some(item[2]);
                    }
                    result.percentile.energy.push(item[0]);
                    next_index = js::round((total_weight - 1.0) * result.percentile.energy.len() as f64 / 100.0);
                }
                weight_sum = next_weight_sum;
            }

            for index in 0..6 {
                common_list.sort_by(|a, b| js::cmp_num(a[index], b[index]));
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
                for item in &common_list {
                    let next_weight_sum = weight_sum + item[6];
                    while weight_sum <= next_index && next_index < next_weight_sum && target.len() <= 100 {
                        target.push(item[index]);
                        next_index = js::round((total_weight - 1.0) * target.len() as f64 / 100.0);
                    }
                    weight_sum = next_weight_sum;
                }
            }

            per_food.push((food_combination_list[fi].clone(), result));
        }
        out.result.push((pokemon.name.clone(), per_food));
    }
    Ok(out)
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

/// `pokemon_list` をスレッド数に分けて評価する(本家 MultiWorker 相当)
fn evaluate_parallel(
    config: &Config,
    lv: u32,
    pokemon_list: &[&Pokemon],
    combinations: &[SubSkillCombination],
    healer: f64,
    support: f64,
    thread_num: usize,
    progress: &(dyn Fn(usize) + Sync),
) -> Result<Vec<EvaluateWorkerResult>, String> {
    let foods = food_combination_list(lv);
    let n = thread_num.max(1);
    (0..n)
        .into_par_iter()
        .map(|i| {
            let i1 = pokemon_list.len() * i / n;
            let i2 = pokemon_list.len() * (i + 1) / n;
            evaluate_pokemon(config, lv, &pokemon_list[i1..i2], &foods, combinations, healer, support, || progress(1))
        })
        .collect()
}

/// 本家 `EvaluateTable.simulation`。厳選テーブルを作成する。
pub fn simulate_evaluate_table(config: &Config, progress: &dyn Progress) -> Result<EvaluateTable, String> {
    let m = master();
    let se = &config.select_evaluate;
    let lv_list = se.enabled_levels();
    let thread_num = config.worker_num.max(1.0) as usize;

    progress.set(0.0, "サブスキルの組み合わせを準備しています…");
    let combination_list = sub_skill_combination_list(&se.silver_seed);

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
        if d % 2000 == 0 {
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
    progress.set(0.0, "サブスキルの組み合わせを準備しています…");
    let combination_list = sub_skill_combination_list(&evaluate_config.silver_seed);

    let mut table = EvaluateTable {
        version: m.version.evaluate,
        sleep_time: config.sleep_time,
        check_freq: config.check_freq,
        ..Default::default()
    };
    let lv_list = [30u32, 50, 60, 70, 80];
    for (index, &lv) in lv_list.iter().enumerate() {
        let foods: Vec<String> = if lv < 60 { vec!["00".into(), "01".into()] } else { food_combination_list(lv) };
        let sub_skill_num = crate::simulator::enabled_sub_skill_len(lv as f64);
        let level_rate = 1.025_f64.powf(lv as f64 - 1.0) / (1.0 - 0.002 * (lv as f64 - 1.0));
        let healer = evaluate_config.score_for_healer_evaluate.unwrap_or(f64::NAN) * level_rate;
        let support = evaluate_config.score_for_support_evaluate.unwrap_or(f64::NAN) * level_rate;
        progress.set(index as f64 / lv_list.len() as f64, &format!("Lv{lv}の簡易診断を計算しています…"));
        let combos = &combination_list[sub_skill_num];
        let r = evaluate_pokemon(&fixed, lv, &pokemon_list, &foods, combos, healer, support, || {})?;
        table.healer.insert(lv, healer);
        table.support.insert(lv, support);
        for (name, per_food) in r.result {
            let by_food = table.pokemon.entry(name).or_default().entry(lv).or_default();
            for (food, out) in per_food {
                let mut p = out.percentile;
                p.food1.clear();
                p.food2.clear();
                p.food3.clear();
                by_food.insert(food, p);
            }
        }
    }
    progress.set(1.0, "完了");
    Ok(table)
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
