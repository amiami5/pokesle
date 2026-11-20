//! ボックス集計(本家 `pages/box-summary/*.vue` の計算部分)
//!
//! きのみ・食材・スキル・ポケモンごとに、ボックスの1位ポケモンを仮定Lv別に並べる。
//! ボックス概算は `simulate_box` をそのまま使う(本家の `PokemonBox.simulation`)。

use crate::box_sim::BoxResult;
use crate::config::Config;
use crate::data::master;
use crate::evaluate_table::{EvalLv, EvaluateTable};
use crate::sim_pokemon::{BoxPokemon, SimPokemon};
use crate::team_sim::simulate_box;

/// 列ごとの育成仮定(固定Lvなし, Lv30, Lv50, Lv60, Lv70)。表の見出しは本家どおり 30/50/60/75
pub const SETTINGS: [(bool, Option<f64>); 5] = [(false, None), (true, Some(30.0)), (true, Some(50.0)), (true, Some(60.0)), (true, Some(70.0))];
pub const COLUMN_NAMES: [&str; 5] = ["", "Lv30仮定", "Lv50仮定", "Lv60仮定", "Lv75仮定"];

/// 集計の1セル
#[derive(Debug, Clone, Default)]
pub struct Ranked {
    pub value: Option<f64>,
    pub pokemon: Option<SimPokemon>,
}

/// 集計用の設定にする(本家の `field: 'ワカクサ本島'` などの上書き)
fn base_config(config: &Config, fix: bool, fix_lv: Option<f64>) -> Config {
    let mut c = config.clone();
    let s = &mut c.simulation;
    s.field = "ワカクサ本島".into();
    s.field_bonus = 0.0;
    s.berry_list = vec![String::new(), String::new(), String::new()];
    s.mode = 0.0;
    s.fix = fix;
    s.fix_lv = fix_lv;
    c
}

/// 進行度を 0〜1 で通知する
pub type OnProgress<'a> = &'a (dyn Fn(f64) + Sync);

fn simulate_settings(
    config: &Config,
    box_list: &[BoxPokemon],
    table: Option<&EvaluateTable>,
    tweak: impl Fn(&mut Config),
    progress: (f64, f64),
    on_progress: OnProgress,
) -> Result<Vec<Vec<BoxResult>>, String> {
    let mut out = Vec::new();
    for (i, (fix, lv)) in SETTINGS.iter().enumerate() {
        let mut c = base_config(config, *fix, *lv);
        tweak(&mut c);
        out.push(simulate_box(&c, box_list, table, true)?);
        on_progress(progress.0 + (progress.1 - progress.0) * (i + 1) as f64 / SETTINGS.len() as f64);
    }
    Ok(out)
}

/// `key` が最大の要素(同値なら先頭。本家の安定ソート後の先頭と同じ)
fn top<'a>(list: impl Iterator<Item = &'a BoxResult>, key: impl Fn(&SimPokemon) -> f64) -> Option<&'a BoxResult> {
    let mut best: Option<(&BoxResult, f64)> = None;
    for r in list {
        let v = key(&r.pokemon);
        if best.is_none_or(|(_, b)| v > b) {
            best = Some((r, v));
        }
    }
    best.map(|(r, _)| r)
}

// ---- きのみ ----

#[derive(Debug, Clone)]
pub struct BerryRow {
    pub name: String,
    pub type_name: String,
    pub cols: Vec<Ranked>,
}

pub fn berry_summary(config: &Config, box_list: &[BoxPokemon], table: Option<&EvaluateTable>, on_progress: OnProgress) -> Result<Vec<BerryRow>, String> {
    let m = master();
    let sims = simulate_settings(
        config,
        box_list,
        table,
        |c| {
            c.simulation.fix_evolve_all_stages = true;
        },
        (0.0, 1.0),
        on_progress,
    )?;
    Ok(m.berries
        .iter()
        .enumerate()
        .map(|(bi, berry)| BerryRow {
            name: berry.name.clone(),
            type_name: berry.type_name.clone(),
            cols: sims
                .iter()
                .map(|list| {
                    let t = top(list.iter().filter(|r| m.pokemon[r.pokemon.base].berry == bi), |p| p.b_ep_d);
                    Ranked { value: t.map(|r| r.pokemon.b_ep_d), pokemon: t.map(|r| r.pokemon.clone()) }
                })
                .collect(),
        })
        .collect())
}

// ---- 食材 ----

#[derive(Debug, Clone)]
pub struct FoodRow {
    pub name: String,
    /// 最大要求数(その食材を最も使うレシピの数量)
    pub require: f64,
    /// 取得数目安(サブスキル・せいかく補正抜き)
    pub max: f64,
    pub max_pokemon: Option<usize>,
    pub max_food_list: Vec<String>,
    /// 1日あたりの個数と1位ポケモン
    pub cols: Vec<Ranked>,
}

impl FoodRow {
    /// 表示用の割合。`mode` 0: 最大要求数に対する割合、1: 取得数目安に対する割合、2: 個数そのまま
    pub fn score(&self, col: usize, mode: u8) -> Option<f64> {
        let n = self.cols.get(col)?.value?;
        Some(match mode {
            0 => n / self.require,
            1 => n / self.max,
            _ => n,
        })
    }
}

pub fn food_summary(config: &Config, box_list: &[BoxPokemon], table: Option<&EvaluateTable>, on_progress: OnProgress) -> Result<Vec<FoodRow>, String> {
    let m = master();
    let sims = simulate_settings(
        config,
        box_list,
        table,
        |c| {
            c.simulation.genki_full = true;
            c.simulation.help_bonus = Some(3.0);
        },
        (0.0, 1.0),
        on_progress,
    )?;

    // 取得数目安(本家 foodMaxMap)
    struct Max {
        num: f64,
        pokemon: Option<usize>,
        food_list: Vec<String>,
    }
    let mut max: Vec<Max> = m.foods.iter().map(|_| Max { num: 0.0, pokemon: None, food_list: Vec::new() }).collect();
    for (pi, pokemon) in m.pokemon.iter().enumerate() {
        let food_help_num = pokemon.food_rate * 86400.0 / pokemon.help / 0.45;
        let usable = |slot: usize| -> Vec<&crate::data::PokemonFood> {
            pokemon.food_list.iter().filter(|f| !f.name.is_empty() && f.num_list.get(slot).copied().flatten().is_some_and(|n| n != 0.0)).collect()
        };
        for f1 in usable(0) {
            for f2 in usable(1) {
                for f3 in usable(2) {
                    let mut map: Vec<(usize, f64)> = Vec::new();
                    for (f, slot) in [(f1, 0), (f2, 1), (f3, 2)] {
                        let n = f.num_list[slot].unwrap_or(0.0) / 3.0;
                        match map.iter_mut().find(|(i, _)| *i == f.food) {
                            Some(e) => e.1 += n,
                            None => map.push((f.food, n)),
                        }
                    }
                    for (food, num) in map {
                        let num = num * food_help_num;
                        if max[food].num < num {
                            max[food] = Max { num, pokemon: Some(pi), food_list: vec![f1.name.clone(), f2.name.clone(), f3.name.clone()] };
                        }
                    }
                }
            }
        }
    }

    Ok(m.foods
        .iter()
        .enumerate()
        .map(|(fi, food)| {
            let require = m
                .cookings
                .iter()
                .map(|c| c.food_list.iter().find(|f| f.name == food.name).map_or(0.0, |f| f.num))
                .fold(0.0, f64::max);
            FoodRow {
                name: food.name.clone(),
                require,
                max: max[fi].num,
                max_pokemon: max[fi].pokemon,
                max_food_list: max[fi].food_list.clone(),
                cols: sims
                    .iter()
                    .map(|list| {
                        let t = top(list.iter(), |p| p.food_count.get(fi).copied().unwrap_or(f64::NAN));
                        Ranked { value: t.and_then(|r| r.pokemon.food_count.get(fi).copied()), pokemon: t.map(|r| r.pokemon.clone()) }
                    })
                    .collect(),
            }
        })
        .collect())
}

// ---- スキル ----

#[derive(Debug, Clone)]
pub struct SkillRow {
    pub name: String,
    /// 最大(スキル確率S+M・なまいき・おてつだいスピード35%短縮・Lv100)の発動回数
    pub max: f64,
    pub max_pokemon: Option<String>,
    /// 1日あたりの発動回数と1位ポケモン
    pub cols: Vec<Ranked>,
}

impl SkillRow {
    pub fn score(&self, col: usize) -> Option<f64> {
        Some(self.cols.get(col)?.value? / self.max)
    }
}

/// 最終進化の各ポケモンの食材構成(000,001,002,010,011,012)ごとに呼ぶ
fn food_combination_pokemon(mut make: impl FnMut(&str, Vec<String>)) {
    let m = master();
    for base in m.pokemon.iter().filter(|p| p.after_list.len() == 1 && p.after_list[0] == p.name) {
        for combo in ["000", "001", "002", "010", "011", "012"] {
            let names: Option<Vec<String>> = combo
                .chars()
                .map(|c| base.food_list.get(c as usize - '0' as usize).filter(|f| m.food(&f.name).is_some()).map(|f| f.name.clone()))
                .collect();
            if let Some(names) = names {
                make(&base.name, names);
            }
        }
    }
}

pub fn skill_summary(config: &Config, box_list: &[BoxPokemon], table: Option<&EvaluateTable>, on_progress: OnProgress) -> Result<Vec<SkillRow>, String> {
    let m = master();
    let sims = simulate_settings(config, box_list, table, |_| {}, (0.0, 0.8), on_progress)?;

    let sub_skill_list: Vec<String> = ["おてつだいボーナス", "おてつだいスピードM", "スキル確率アップM", "スキル確率アップS", "所持数アップL"].iter().map(|s| s.to_string()).collect();
    let mut pokemon_list = Vec::new();
    food_combination_pokemon(|name, foods| {
        pokemon_list.push(BoxPokemon {
            name: name.to_string(),
            lv: 100.0,
            food_list: foods,
            sub_skill_list: sub_skill_list.clone(),
            nature: "なまいき".into(),
            ..Default::default()
        });
    });
    let mut c = config.clone();
    c.simulation.expect_type = [("food".to_string(), 0.0), ("skill".to_string(), 0.0)].into_iter().collect();
    c.simulation.help_bonus = Some(5.0);
    let skill_sims = simulate_box(&c, &pokemon_list, None, true)?;
    on_progress(1.0);

    // スキルごとの最大(同値なら先頭)
    let mut max: Vec<Option<(f64, String)>> = vec![None; m.skills.len()];
    for r in &skill_sims {
        let p = &r.pokemon;
        let skill = m.pokemon[p.base].skill;
        if max[skill].as_ref().map_or(0.0, |x| x.0) < p.skill_per_day {
            max[skill] = Some((p.skill_per_day, m.pokemon[p.base].name.clone()));
        }
    }

    Ok(m.skills
        .iter()
        .enumerate()
        .map(|(si, skill)| SkillRow {
            name: skill.name.clone(),
            max: max[si].as_ref().map_or(0.0, |x| x.0),
            max_pokemon: max[si].as_ref().map(|x| x.1.clone()),
            cols: sims
                .iter()
                .map(|list| {
                    let t = top(list.iter().filter(|r| m.pokemon[r.pokemon.base].skill == si), |p| p.skill_per_day);
                    Ranked { value: t.map(|r| r.pokemon.skill_per_day), pokemon: t.map(|r| r.pokemon.clone()) }
                })
                .collect(),
        })
        .collect())
}

// ---- ポケモン ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluateType {
    Energy,
    Specialty,
}

#[derive(Debug, Clone)]
pub struct PokemonRow {
    /// ポケモンのインデックス
    pub base: usize,
    /// 食材構成ごとの表示のときの食材
    pub food_list: Vec<String>,
    /// 食材構成ごとの表示のときの食材数(食材インデックス順。Lv60・サブスキルなし)
    pub food_count: Vec<f64>,
    /// 厳選(最大)
    pub max: Ranked,
    /// 選択中の厳選Lvごと
    pub by_lv: Vec<(u32, Ranked)>,
}

pub struct PokemonSummary {
    pub lv_list: Vec<u32>,
    pub rows: Vec<PokemonRow>,
}

/// 食材構成ごとの表示で使う、Lv60・サブスキルなし・まじめの各ポケモン(本家 `loadFoodNumInfo`)
fn food_num_list(config: &Config, box_list: &[BoxPokemon]) -> Result<Vec<SimPokemon>, String> {
    use crate::simulator::{HelpOption, Mode, PokemonSimulator};
    let m = master();
    let mut c = config.clone();
    c.simulation.expect_type = [("food".to_string(), 0.0), ("skill".to_string(), 0.0)].into_iter().collect();
    let mut simulator = PokemonSimulator::new(c, Mode::About);
    let mut boxes = Vec::new();
    food_combination_pokemon(|name, foods| {
        if config.kaihou_evaluate && m.pokemon(name).is_some_and(|p| p.kaihou) {
            return;
        }
        boxes.push(BoxPokemon { name: name.to_string(), lv: 60.0, food_list: foods, nature: "まじめ".into(), ..Default::default() });
    });
    if config.kaihou_evaluate {
        // かいほうポケモン(ミュウ・ダークライ)は512通り出さず、ボックスに所持している食材構成だけを追加する
        let mut owned: Vec<(usize, String, Vec<String>)> = Vec::new();
        for bx in box_list {
            let Some(base) = m.pokemon(&bx.name) else { continue };
            if !base.kaihou || bx.food_list.first().is_none_or(|f| m.food(f).is_none()) {
                continue;
            }
            let foods: Vec<String> = (0..3)
                .map(|k| bx.food_list.get(k).filter(|f| base.food_name_list.contains(f)).cloned().unwrap_or_default())
                .collect();
            if !owned.iter().any(|(_, n, fl)| n == &base.name && fl == &foods) {
                owned.push((base.index, base.name.clone(), foods));
            }
        }
        owned.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
        for (_, name, foods) in owned {
            boxes.push(BoxPokemon { name, lv: 60.0, food_list: foods, nature: "まじめ".into(), ..Default::default() });
        }
    }
    let mut out = Vec::new();
    for bx in &boxes {
        let mut p = simulator.from_box(bx, false, 0.0, 0.0)?;
        simulator.calc_status(&mut p, 0.0, 0.0, None);
        simulator.calc_team_heal(std::slice::from_mut(&mut p), &[0]);
        simulator.calc_help(&mut p, &HelpOption::default())?;
        out.push(p);
    }
    Ok(out)
}

pub fn pokemon_summary(
    config: &Config,
    box_list: &[BoxPokemon],
    table: Option<&EvaluateTable>,
    each_food: bool,
    evaluate_type: EvaluateType,
    on_progress: OnProgress,
) -> Result<PokemonSummary, String> {
    let m = master();
    let lv_list = config.select_evaluate.enabled_levels();
    let c = base_config(config, false, None);
    let sims = simulate_box(&c, box_list, table, true)?;
    on_progress(0.8);

    // 対象(本家 pokemonList)
    struct Target {
        base: usize,
        food_list: Vec<String>,
        food_count: Vec<f64>,
    }
    let targets: Vec<Target> = if each_food {
        food_num_list(config, box_list)?
            .into_iter()
            .map(|p| Target {
                base: p.base,
                food_list: p.box_pokemon.as_ref().map(|b| b.food_list.clone()).unwrap_or_default(),
                food_count: p.food_count.clone(),
            })
            .collect()
    } else {
        m.pokemon.iter().filter(|p| p.is_last).map(|p| Target { base: p.index, food_list: Vec::new(), food_count: Vec::new() }).collect()
    };

    let score_of = |r: &BoxResult, lv: u32, name: &str| -> Option<f64> {
        let entry = r.evaluate_result.as_ref()?.get(EvalLv::Lv(lv), name)?;
        let info = match evaluate_type {
            EvaluateType::Energy => entry.energy.as_ref(),
            EvaluateType::Specialty => entry.specialty.as_ref(),
        };
        Some(info.map_or(f64::NAN, |i| i.score))
    };

    let rows = targets
        .into_iter()
        .map(|t| {
            let name = m.pokemon[t.base].name.as_str();
            let mut max: Option<f64> = None;
            let mut max_ranked = Ranked::default();
            let mut by_lv = Vec::new();
            for &lv in &lv_list {
                let mut best: Option<(&BoxResult, f64)> = None;
                for r in &sims {
                    let Some(score) = score_of(r, lv, name) else { continue };
                    if each_food {
                        let bf = r.pokemon.box_pokemon.as_ref().map(|b| b.food_list.as_slice()).unwrap_or(&[]);
                        if !t.food_list.iter().enumerate().all(|(i, f)| bf.get(i).map(String::as_str).unwrap_or("") == f.as_str()) {
                            continue;
                        }
                    }
                    if best.is_none_or(|(_, b)| score > b) {
                        best = Some((r, score));
                    }
                }
                let ranked = Ranked { value: best.map(|b| b.1), pokemon: best.map(|b| b.0.pokemon.clone()) };
                // 本家: max == null || max < score
                if max.is_none() || ranked.value.is_some_and(|v| max.is_some_and(|m| m < v)) {
                    max = ranked.value;
                    max_ranked = ranked.clone();
                }
                by_lv.push((lv, ranked));
            }
            PokemonRow { base: t.base, food_list: t.food_list, food_count: t.food_count, max: max_ranked, by_lv }
        })
        .collect();
    on_progress(1.0);
    Ok(PokemonSummary { lv_list, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AllSpecialtyMode;

    #[test]
    fn pokemon_summary_each_food_lists_only_owned_kaihou_foods() {
        let mut table = EvaluateTable::default();
        for lv in [30, 50, 60, 70] {
            table.healer.insert(lv, 10000.0);
            table.support.insert(lv, 150.0);
        }
        let mut cfg = Config::default_config();
        cfg.kaihou_evaluate = true;
        cfg.all_specialty = Some(AllSpecialtyMode::Max);
        cfg.select_evaluate.level_list = [(30, true), (50, false), (60, false), (70, false)].into_iter().collect();

        let bx = BoxPokemon {
            name: "ダークライ".into(),
            lv: 25.0,
            food_list: vec!["とくせんリンゴ".into(), "めざましコーヒー".into(), String::new()],
            sub_skill_list: vec!["きのみの数S".into(), String::new(), String::new(), String::new(), String::new()],
            nature: "いじっぱり".into(),
            ..Default::default()
        };
        let summary = pokemon_summary(&cfg, &[bx], Some(&table), true, EvaluateType::Specialty, &|_| {}).unwrap();
        let darkrai_rows: Vec<&PokemonRow> = summary.rows.iter().filter(|r| master().pokemon[r.base].name == "ダークライ").collect();
        assert_eq!(darkrai_rows.len(), 1);
        assert_eq!(darkrai_rows[0].food_list, vec!["とくせんリンゴ", "めざましコーヒー", ""]);
        assert!(darkrai_rows[0].max.value.is_some());
        // 未所持のミュウ(kaihou)は「食材構成ごと」には出ない
        assert!(summary.rows.iter().all(|r| !master().pokemon[r.base].name.starts_with("ミュウ(")));
    }

    #[test]
    fn berry_summary_prefers_higher_berry_energy_evolution_stage() {
        use std::collections::{BTreeMap, HashMap};

        use crate::evaluate_table::PercentileSet;

        let mut table = EvaluateTable::default();
        for lv in [30, 50, 60, 70] {
            table.healer.insert(lv, 10000.0);
            table.support.insert(lv, 150.0);
        }
        // 育成仮定のボーダー判定(this_fix = true)を通すためダミーエントリを用意
        for name in ["ケッキング", "ライチュウ"] {
            let mut lv_map = BTreeMap::new();
            let mut food_map = HashMap::new();
            food_map.insert("00".to_string(), PercentileSet { energy: vec![0.0, 1.0], ..Default::default() });
            lv_map.insert(50, food_map);
            table.pokemon.insert(name.to_string(), lv_map);
        }

        let mut cfg = Config::default_config();
        cfg.simulation.fix_evolve = true;
        cfg.simulation.fix_border = Some(0.0);
        cfg.select_evaluate.level_list = [(30, false), (50, true), (60, false), (70, false)].into_iter().collect();

        let namakero = BoxPokemon {
            index: 0,
            name: "ナマケロ".into(),
            lv: 10.0,
            food_list: vec!["あんみんトマト".into(), "あんみんトマト".into(), "あんみんトマト".into()],
            sub_skill_list: vec!["きのみの数S".into(), String::new(), String::new(), String::new(), String::new()],
            nature: "いじっぱり".into(),
            ..Default::default()
        };
        let pichu = BoxPokemon {
            index: 1,
            name: "ピチュー".into(),
            lv: 10.0,
            food_list: vec!["とくせんリンゴ".into(), "とくせんリンゴ".into(), "とくせんリンゴ".into()],
            sub_skill_list: vec!["きのみの数S".into(), String::new(), String::new(), String::new(), String::new()],
            nature: "いじっぱり".into(),
            ..Default::default()
        };

        let rows = berry_summary(&cfg, &[namakero, pichu], Some(&table), &|_| {}).unwrap();
        let ki = rows.iter().find(|r| r.name == "キー").unwrap();
        let ubu = rows.iter().find(|r| r.name == "ウブ").unwrap();

        // cols[2] = Lv50仮定: ナマケロはケッキングよりきのみエナジーが高い中間進化のヤルキモノが選ばれる
        assert_eq!(ki.cols[2].pokemon.as_ref().unwrap().base().name, "ヤルキモノ");
        // ピチューは最終進化のライチュウが選ばれる
        assert_eq!(ubu.cols[2].pokemon.as_ref().unwrap().base().name, "ライチュウ");
    }
}
