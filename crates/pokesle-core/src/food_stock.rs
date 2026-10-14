//! 食材ストックの準備計算(本家 `models/simulation/food-stock-worker.ts` と `pages/food_stock.vue` の計算部分)
//!
//! 焼きなまし法でカレー・サラダ・デザートそれぞれの料理計画を探す。乱数は本家と同じ線形合同法で、
//! 同じシードなら本家と同じ結果になる(ゴールデンテストで確認)。

use std::collections::BTreeMap;

use rayon::prelude::*;

use crate::config::{Config, FoodStockConfig};
use crate::cooking::evaluate_lv_list;
use crate::data::{master, CookingType};

const COOKING_TYPES: [CookingType; 3] = CookingType::ALL;
const RESTART_NUM: usize = 12;
const ITERATION_NUM: usize = 12000;

/// ワーカーに渡す料理
#[derive(Debug, Clone)]
pub struct Recipe {
    pub name: String,
    pub cooking_type: CookingType,
    pub food_num: f64,
    /// (食材のインデックス, 個数)
    pub food_list: Vec<(usize, f64)>,
    pub energy: f64,
}

/// ワーカーへの入力(本家 `Input`)
#[derive(Debug, Clone)]
pub struct Input {
    pub bag_size: f64,
    pub cooking_num: f64,
    pub candidate_num: f64,
    pub min_food_num: f64,
    pub max_food_num: f64,
    pub weights: [f64; 3],
    pub cooking_list: Vec<Recipe>,
    pub seed: u32,
}

#[derive(Debug, Clone)]
pub struct PlanCooking {
    pub name: String,
    pub num: f64,
    /// 食材インデックスごとの個数(0 の食材は含まない)
    pub food_num_map: BTreeMap<usize, f64>,
    pub energy: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub cooking_list: Vec<PlanCooking>,
    pub energy: f64,
}

#[derive(Debug, Clone)]
pub struct FoodStockResult {
    pub score: f64,
    /// 食材インデックスごとの準備数
    pub food_num_list: Vec<f64>,
    pub total_food_num: f64,
    /// カレー・サラダ・デザートの順
    pub plans: [Plan; 3],
}

#[derive(Clone)]
struct Evaluation {
    score: f64,
    total_food_num: f64,
    food_num_list: Vec<f64>,
    energy_list: [f64; 3],
}

struct Random(u32);

impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0 as f64 / 4294967296.0
    }

    fn index(&mut self, len: usize) -> usize {
        (self.next() * len as f64).floor() as usize
    }
}

type PlanList = [Vec<usize>; 3];

struct Worker<'a> {
    input: &'a Input,
    cooking_num: usize,
    recipe_list: [Vec<&'a Recipe>; 3],
    food_len: usize,
}

impl Worker<'_> {
    fn evaluate(&self, plan_list: &PlanList) -> Evaluation {
        let mut by_type = vec![vec![0.0_f64; self.food_len]; 3];
        let mut energy_list = [0.0_f64; 3];
        for t in 0..3 {
            for &i in &plan_list[t] {
                let recipe = self.recipe_list[t][i];
                energy_list[t] += recipe.energy;
                for &(food, num) in &recipe.food_list {
                    by_type[t][food] += num;
                }
            }
        }
        let food_num_list: Vec<f64> = (0..self.food_len).map(|f| by_type[0][f].max(by_type[1][f]).max(by_type[2][f])).collect();
        let total_food_num = food_num_list.iter().sum();
        let mut score = 0.0;
        for t in 0..3 {
            if self.input.weights[t] > 0.0 {
                score += self.input.weights[t] * energy_list[t].ln();
            }
        }
        Evaluation { score, total_food_num, food_num_list, energy_list }
    }

    fn objective(&self, e: &Evaluation) -> f64 {
        if e.total_food_num <= self.input.bag_size {
            return e.score;
        }
        // 一時的な容量超過を許容し、より良い構成へ移りやすくする。
        e.score - (e.total_food_num - self.input.bag_size) * 12.0
    }

    fn mutate(&self, plan_list: &mut PlanList, random: &mut Random) {
        let t = random.index(3);
        if self.input.weights[t] <= 0.0 {
            return;
        }
        let len = self.recipe_list[t].len();
        let plan = &mut plan_list[t];
        let action = random.next();
        if action < 0.34 && plan.len() < self.cooking_num {
            plan.push(random.index(len));
        } else if action < 0.48 && plan.len() > 1 {
            let i = random.index(plan.len());
            plan.remove(i);
        } else {
            let i = random.index(plan.len());
            plan[i] = random.index(len);
        }
    }

    fn initial_plan(&self, random: &mut Random) -> PlanList {
        let mut plan_list: PlanList = std::array::from_fn(|t| {
            if self.input.weights[t] <= 0.0 {
                return Vec::new();
            }
            let list = &self.recipe_list[t];
            let mut best = 0;
            for (i, r) in list.iter().enumerate() {
                if r.food_num < list[best].food_num {
                    best = i;
                }
            }
            vec![best]
        });
        let mut current = self.evaluate(&plan_list);
        for _ in 0..300 {
            let mut next_plan = plan_list.clone();
            self.mutate(&mut next_plan, random);
            let next = self.evaluate(&next_plan);
            if self.objective(&next) >= self.objective(&current) || random.next() < 0.08 {
                current = next;
                plan_list = next_plan;
            }
        }
        plan_list
    }

    fn create_result(&self, plan_list: &PlanList, e: &Evaluation) -> FoodStockResult {
        let plans = std::array::from_fn(|t| {
            let mut num_map: BTreeMap<usize, f64> = BTreeMap::new();
            for &i in &plan_list[t] {
                *num_map.entry(i).or_insert(0.0) += 1.0;
            }
            let mut cooking_list: Vec<PlanCooking> = num_map
                .into_iter()
                .map(|(i, num)| {
                    let recipe = self.recipe_list[t][i];
                    PlanCooking {
                        name: recipe.name.clone(),
                        num,
                        food_num_map: recipe.food_list.iter().map(|&(f, n)| (f, n * num)).collect(),
                        energy: recipe.energy * num,
                    }
                })
                .collect();
            cooking_list.sort_by(|a, b| b.energy.partial_cmp(&a.energy).unwrap_or(std::cmp::Ordering::Equal));
            Plan { cooking_list, energy: e.energy_list[t] }
        });
        FoodStockResult { score: e.score, food_num_list: e.food_num_list.clone(), total_food_num: e.total_food_num, plans }
    }
}

/// JS の `Math.floor(x) || fallback`
fn floor_or(x: f64, fallback: f64) -> f64 {
    let f = x.floor();
    if f == 0.0 || f.is_nan() { fallback } else { f }
}

/// 本家ワーカー1つ分の探索。見つからなければ None
pub fn run_worker(input: &Input, on_progress: &(dyn Fn(f64) + Sync)) -> Result<Option<FoodStockResult>, String> {
    let cooking_num = floor_or(input.cooking_num, 1.0).clamp(1.0, 21.0) as usize;
    let min_food_num = floor_or(input.min_food_num, 0.0).max(0.0);
    let max_food_num = floor_or(input.max_food_num, 0.0).max(0.0);
    let candidate = floor_or(input.candidate_num, 1.0).max(1.0);

    let recipe_list: [Vec<&Recipe>; 3] = std::array::from_fn(|t| {
        let mut all: Vec<&Recipe> = input.cooking_list.iter().filter(|c| c.cooking_type == COOKING_TYPES[t] && c.food_num > 0.0).collect();
        all.sort_by(|a, b| b.energy.partial_cmp(&a.energy).unwrap_or(std::cmp::Ordering::Equal));
        let take = candidate.min(all.len() as f64) as usize;
        all.into_iter().filter(|c| c.food_num >= min_food_num && c.food_num <= max_food_num).take(take).collect()
    });
    for t in 0..3 {
        if input.weights[t] > 0.0 && recipe_list[t].is_empty() {
            return Err("必要食材数の条件に該当する料理がない種類があります。".into());
        }
    }

    let worker = Worker { input, cooking_num, recipe_list, food_len: master().foods.len() };
    let mut random = Random(input.seed);
    let mut best: Option<(PlanList, Evaluation)> = None;
    let better = |best: &Option<(PlanList, Evaluation)>, e: &Evaluation| {
        e.total_food_num <= input.bag_size && best.as_ref().is_none_or(|(_, b)| b.score < e.score)
    };

    for restart in 0..RESTART_NUM {
        let mut current_plan = worker.initial_plan(&mut random);
        let mut current = worker.evaluate(&current_plan);
        let mut current_objective = worker.objective(&current);
        if better(&best, &current) {
            best = Some((current_plan.clone(), current.clone()));
        }
        for iteration in 0..ITERATION_NUM {
            let mut next_plan = current_plan.clone();
            let mutation_num = if random.next() < 0.08 { 2 + (random.next() * 3.0).floor() as usize } else { 1 };
            for _ in 0..mutation_num {
                worker.mutate(&mut next_plan, &mut random);
            }
            let next = worker.evaluate(&next_plan);
            let next_objective = worker.objective(&next);
            let temperature = 35.0 * (1.0 - iteration as f64 / ITERATION_NUM as f64) + 0.05;
            let accept = next_objective >= current_objective || random.next() < ((next_objective - current_objective) / temperature).exp();
            if better(&best, &next) {
                best = Some((next_plan.clone(), next.clone()));
            }
            if accept {
                current_plan = next_plan;
                current = next;
                current_objective = next_objective;
            }
        }
        if better(&best, &current) {
            best = Some((current_plan, current));
        }
        on_progress((restart + 1) as f64 / RESTART_NUM as f64);
    }

    Ok(best.map(|(plan, e)| worker.create_result(&plan, &e)))
}

/// 設定から有効な料理を求める(本家 food_stock.vue の `cookingList`)。モードは常に通常(0)として扱う
pub fn enable_cooking_list(config: &Config) -> Vec<crate::cooking::EvaluatedCooking> {
    let mut sim = config.simulation.clone();
    sim.mode = 0.0;
    let max_lv = master().cooking_stats.max_recipe_lv as f64;
    let exclude_max = config.food_stock.exclude_max_recipe_lv;
    evaluate_lv_list(Some(&sim))
        .into_iter()
        .filter(|c| c.enable)
        .filter(|c| !exclude_max || sim.cooking_settings.get(&c.cooking().name).map(|x| x.lv).unwrap_or(f64::NAN) < max_lv)
        .collect()
}

/// 候補数の上限(本家 `maxCandidateNum`)。重み0の種類しか無ければ 0
pub fn max_candidate_num(config: &Config, list: &[crate::cooking::EvaluatedCooking]) -> usize {
    let fs = &config.food_stock;
    let min = fs.min_food_num.max(0.0);
    let max = fs.max_food_num.max(0.0);
    COOKING_TYPES
        .iter()
        .filter(|t| fs.weight_of(**t) > 0.0)
        .map(|t| {
            list.iter()
                .filter(|c| {
                    let cooking = c.cooking();
                    cooking.cooking_type == *t && cooking.food_num >= min && cooking.food_num <= max
                })
                .count()
        })
        .min()
        .unwrap_or(0)
}

/// 本家 `calculate()`。`workers` 個のワーカー(シードは seed_base + i*100003)を並列に走らせ、最良の結果を返す
pub fn calculate(config: &Config, seed_base: u32, on_progress: &(dyn Fn(f64) + Sync)) -> Result<FoodStockResult, String> {
    let fs: &FoodStockConfig = &config.food_stock;
    let weights = COOKING_TYPES.map(|t| fs.weight_of(t));
    if weights.iter().all(|w| *w <= 0.0) {
        return Err("いずれかの料理種類の重みを1%以上にしてください。".into());
    }
    let list = enable_cooking_list(config);
    let max_candidate = max_candidate_num(config, &list);
    if max_candidate == 0 {
        return Err("必要食材数の条件に該当する料理がない種類があります。".into());
    }
    let cooking_list: Vec<Recipe> = list
        .iter()
        .map(|c| {
            let cooking = c.cooking();
            Recipe {
                name: cooking.name.clone(),
                cooking_type: cooking.cooking_type,
                food_num: cooking.food_num,
                food_list: cooking.food_list.iter().map(|f| (f.food, f.num)).collect(),
                energy: c.fix_energy,
            }
        })
        .collect();
    let candidates = floor_or(fs.candidate_num, 1.0).clamp(1.0, max_candidate as f64);
    let worker_num = config.worker_num.max(1.0) as usize;

    let done = std::sync::Mutex::new(vec![0.0_f64; worker_num]);
    let results: Vec<Result<Option<FoodStockResult>, String>> = (0..worker_num)
        .into_par_iter()
        .map(|i| {
            let input = Input {
                bag_size: floor_or(fs.bag_size, 0.0).max(0.0),
                cooking_num: floor_or(fs.cooking_num, 1.0).clamp(1.0, 21.0),
                candidate_num: candidates,
                min_food_num: floor_or(fs.min_food_num, 0.0).max(0.0),
                max_food_num: floor_or(fs.max_food_num, 0.0).max(0.0),
                weights,
                cooking_list: cooking_list.clone(),
                seed: seed_base.wrapping_add((i as u32).wrapping_mul(100003)),
            };
            run_worker(&input, &|v| {
                if let Ok(mut d) = done.lock() {
                    d[i] = v;
                    on_progress(d.iter().sum::<f64>() / worker_num as f64);
                }
            })
        })
        .collect();

    let mut best: Option<FoodStockResult> = None;
    for r in results {
        if let Some(r) = r? && best.as_ref().is_none_or(|b| r.score > b.score) {
            best = Some(r);
        }
    }
    best.ok_or_else(|| "食材バッグ容量内に、各種類の料理回数分を収める組み合わせがありません。".into())
}
