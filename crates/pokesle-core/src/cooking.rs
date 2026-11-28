//! 料理の計算(本家 `data/food_and_cooking.ts` の `Cooking` の関数部分)

use crate::config::SimulationConfig;
use crate::data::{master, Cooking, CookingType};
use crate::js;

/// レシピレベルを評価した料理(本家 `Cooking.evaluateLvList` の各要素)
#[derive(Debug, Clone)]
pub struct EvaluatedCooking {
    /// マスターの料理インデックス
    pub index: usize,
    pub lv: f64,
    pub lv1: Option<f64>,
    pub lv2: Option<f64>,
    pub lv3: Option<f64>,
    pub lv4: Option<f64>,
    pub recipe_lv_bonus: f64,
    pub event_food_rate: f64,
    pub fix_energy: f64,
    pub fix_add_energy: f64,
    pub enable: bool,
}

impl EvaluatedCooking {
    pub fn cooking(&self) -> &'static Cooking {
        &master().cookings[self.index]
    }
}

/// レシピレベルのボーナス(本家 `Cooking.recipeLvs[lv]?.bonus ?? 0`)
pub fn recipe_bonus(lv: f64) -> f64 {
    let stats = &master().cooking_stats;
    if lv.fract() != 0.0 || lv < 1.0 {
        return 0.0;
    }
    stats.recipe_bonus(lv as usize)
}

fn recipe_lv_row(lv: f64) -> Option<&'static crate::data::RecipeLv> {
    let stats = &master().cooking_stats;
    if lv.fract() != 0.0 || lv < 1.0 {
        return None;
    }
    stats.recipe_lvs.get(lv as usize - 1)
}

/// 指定回数作った時のレシピレベル
fn repeat_lv(energy: f64, times: f64) -> f64 {
    let max_lv = master().cooking_stats.max_recipe_lv as f64;
    let mut exp = 0.0;
    let mut lv = 1.0;
    let mut i = 0.0;
    while i < times {
        exp += recipe_lv_row(lv).map(|r| r.bonus).unwrap_or(f64::NAN) * energy;
        while recipe_lv_row(lv).is_some_and(|r| r.total_exp <= exp) && lv < max_lv {
            lv += 1.0;
        }
        i += 1.0;
    }
    lv
}

/// 本家 `Cooking.evaluateLvList(config)`。
/// `sim` が None の場合は厳選評価用(全料理を最大レシピレベルで評価)。
pub fn evaluate_lv_list(sim: Option<&SimulationConfig>) -> Vec<EvaluatedCooking> {
    let m = master();
    let max_lv = m.cooking_stats.max_recipe_lv as f64;
    m.cookings
        .iter()
        .enumerate()
        .map(|(index, cooking)| {
            let mut c = EvaluatedCooking {
                index,
                lv: 1.0,
                lv1: None,
                lv2: None,
                lv3: None,
                lv4: None,
                recipe_lv_bonus: 0.0,
                event_food_rate: 1.0,
                fix_energy: 0.0,
                fix_add_energy: 0.0,
                enable: true,
            };
            if cooking.rate > 1.0 {
                match sim {
                    None => {
                        c.lv2 = Some(max_lv);
                        c.lv = js::max(c.lv, max_lv);
                    }
                    Some(sim) => {
                        if sim.cooking_recipe_lv_type1 {
                            let lv1 = sim.cooking_settings.get(&cooking.name).map(|x| x.lv).unwrap_or(f64::NAN);
                            c.lv1 = Some(lv1);
                            c.lv = js::max(c.lv, lv1);
                        }
                        if sim.cooking_recipe_lv_type2 {
                            c.lv2 = Some(sim.cooking_recipe_fix_lv);
                            c.lv = js::max(c.lv, sim.cooking_recipe_fix_lv);
                        }
                        if sim.cooking_recipe_lv_type3 {
                            let lv3 = repeat_lv(cooking.energy, sim.cooking_recipe_repeat_lv);
                            c.lv3 = Some(lv3);
                            c.lv = js::max(c.lv, lv3);
                        }
                        if sim.cooking_recipe_lv_type4 {
                            let num = js::round(cooking.energy * sim.cooking_recipe_repeat_lv2 / 100.0);
                            let lv4 = repeat_lv(cooking.energy, num);
                            c.lv4 = Some(lv4);
                            c.lv = js::max(c.lv, lv4);
                        }
                    }
                }
            }

            c.recipe_lv_bonus = recipe_bonus(c.lv);
            if let Some(event_food) = sim.and_then(|s| s.event_bonus.food.as_deref()).and_then(|n| m.food_index(n))
                && cooking.food_list.iter().any(|cf| cf.food == event_food)
            {
                c.event_food_rate = 1.5;
            }
            c.fix_energy = cooking.energy * c.recipe_lv_bonus * c.event_food_rate;
            c.fix_add_energy = c.fix_energy - cooking.raw_energy;

            if let Some(sim) = sim {
                if sim.enable_cooking.get(&cooking.name) == Some(&false) {
                    c.enable = false;
                }
                if (sim.mode == 1.0 || sim.mode == 2.0)
                    && sim.cooking_settings.get(&cooking.name).map(|x| x.lv).unwrap_or(f64::NAN) >= max_lv
                {
                    c.enable = false;
                }
            }
            c
        })
        .collect()
}

/// 料理チャンスの週内効果(本家 `Cooking.getChanceWeekEffect`)
#[derive(Debug, Clone, PartialEq)]
pub struct ChanceWeekEffect {
    pub total: f64,
    pub success_probability_list: Vec<f64>,
}

pub fn get_chance_week_effect(effect: f64, day: Option<f64>, initial: f64) -> ChanceWeekEffect {
    let initial = if initial.is_nan() { 0.0 } else { initial.max(0.0) };
    if effect == 0.0 && initial == 0.0 {
        return match day {
            None => {
                let mut list = vec![0.1; 18];
                list.extend([0.3; 3]);
                ChanceWeekEffect { total: 24.6, success_probability_list: list }
            }
            Some(6.0) => ChanceWeekEffect { total: 3.9, success_probability_list: vec![0.3; 3] },
            Some(_) => ChanceWeekEffect { total: 3.3, success_probability_list: vec![0.1; 3] },
        };
    }

    let (n, p) = match day {
        None => (21usize, js::min(effect / 21.0, 0.7)),
        Some(_) => (3usize, js::min(effect / 3.0, 0.7)),
    };
    let sunday = day == Some(6.0);
    let mut result = 0.0;
    let mut p_list = vec![1.0];
    for i in 1..=n {
        let mut this_p = 0.0;
        for j in 1..=i {
            let base = match day {
                None => if i <= 18 { 0.1 } else { 0.3 },
                Some(_) => if sunday { 0.3 } else { 0.1 },
            };
            let jf = j as f64;
            let boost = if i == j { p * jf + initial } else { p * jf };
            let mut rate = (base + js::min(boost, 0.7)) * p_list[i - j];
            for k in (i - j + 1)..i {
                rate *= 1.0 - p_list[k];
            }
            this_p += rate;
        }
        let mult = match day {
            None => if i <= 18 { 1.0 } else { 2.0 },
            Some(_) => if sunday { 2.0 } else { 1.0 },
        };
        result += this_p * mult + 1.0;
        p_list.push(this_p);
    }
    ChanceWeekEffect { total: result, success_probability_list: p_list[1..].to_vec() }
}

/// 料理の種類で絞り込む
pub fn of_type(list: &[EvaluatedCooking], cooking_type: CookingType) -> impl Iterator<Item = &EvaluatedCooking> {
    list.iter().filter(move |c| c.cooking().cooking_type == cooking_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_chance_works_without_skill_effect() {
        let none = get_chance_week_effect(0.0, None, 0.0);
        assert!((none.total - 24.6).abs() < 1e-9);
        assert!((none.success_probability_list[0] - 0.1).abs() < 1e-9);

        // チームに料理チャンス持ちがいなくても初期料理チャンス(+70%)が効くこと
        let with_initial = get_chance_week_effect(0.0, None, 0.7);
        assert!((with_initial.success_probability_list[0] - 0.8).abs() < 1e-9);
        // 2食目: 1食目成功(0.8)*0.1 + 1食目失敗(0.2)*0.8 = 0.24
        assert!((with_initial.success_probability_list[1] - 0.24).abs() < 1e-9);
    }
}
