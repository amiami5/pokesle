//! チームシミュレーション(本家 `models/simulation/team-simulator.ts` と `pages/simulation.vue` の計算部分)

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::*;

use crate::box_sim::{BoxResult, BoxSimulator};
use crate::config::Config;
use crate::cooking::{self, ChanceWeekEffect, EvaluatedCooking};
use crate::data::{berry_energy_at, master, SkillKind, Specialty, SubSkillKind};
use crate::evaluate_table::EvaluateTable;
use crate::filter::filter_pokemon;
use crate::help_rate::HelpRate;
use crate::js;
use crate::sim_pokemon::{BoxPokemon, SimPokemon};
use crate::simulator::{HelpOption, Mode, PokemonSimulator, TeamCtx};

/// ナイトキャップピカチュウ(本家 `data/nightcap_pikachu.js`)
#[derive(Debug, Clone)]
pub struct NightCapPikachu {
    pub lv: f64,
    pub time: f64,
    pub help_num: f64,
    pub b_ep_d: f64,
    /// (食材インデックス, 個数/回)
    pub food_list: Vec<(usize, f64)>,
    pub food_count: Vec<f64>,
}

impl NightCapPikachu {
    pub fn get(lv: f64) -> NightCapPikachu {
        let m = master();
        let list = &m.night_cap_pikachu;
        let i = (js::min(js::max(1.0, lv), list.len() as f64) - 1.0) as usize;
        let row = &list[i];
        let foods = ["とくせんリンゴ", "リラックスカカオ", "あまいミツ"];
        let food_list: Vec<(usize, f64)> = foods
            .iter()
            .zip(&row.food_num_list)
            .map(|(name, &num)| (m.food_index(name).expect("食材"), num))
            .collect();
        let help_num = 86400.0 / row.time;
        let ubu = m.berry("ウブ").map(|b| b.energy).unwrap_or(f64::NAN);
        let b_ep_d = help_num * berry_energy_at(ubu, lv) * (1.0 - 0.16 * row.food_num_list.len() as f64);
        let mut food_count = vec![0.0; m.foods.len()];
        for (k, name) in foods.iter().enumerate() {
            let idx = m.food_index(name).expect("食材");
            food_count[idx] = if row.food_num_list.len() > k { help_num * 0.16 * row.food_num_list[k] } else { 0.0 };
        }
        NightCapPikachu { lv, time: row.time, help_num, b_ep_d, food_list, food_count }
    }
}

#[derive(Debug, Clone)]
pub enum TeamMember {
    Pokemon { pokemon: Box<SimPokemon>, shard_bonus: f64 },
    NightCapPikachu(NightCapPikachu),
}

impl TeamMember {
    pub fn food_count(&self) -> &[f64] {
        match self {
            TeamMember::Pokemon { pokemon, .. } => &pokemon.food_count,
            TeamMember::NightCapPikachu(p) => &p.food_count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CookingResult {
    /// マスターの料理インデックス
    pub cooking: usize,
    pub fix_energy: f64,
    pub energy: f64,
    pub success_power: f64,
    pub add_energy: f64,
    pub pot_size: f64,
}

#[derive(Debug, Clone)]
pub struct TeamResult {
    pub before_energy: Option<f64>,
    pub energy: f64,
    pub raw_energy: f64,
    pub energy_shard: f64,
    pub bonus_shard: f64,
    pub skill_shard: f64,
    pub today_shard: f64,
    pub today_research_exp: f64,
    pub score: f64,
    pub shard_bonus_count: f64,
    pub research_exp_bonus_count: f64,
    pub pokemon_list: Vec<TeamMember>,
    pub use_food_num: Vec<f64>,
    pub add_food_num: Vec<f64>,
    pub default_food_num: Vec<f64>,
    pub food_num: Vec<f64>,
    pub cooking_power_up_effect_list: Vec<f64>,
    pub remain_food_energy: f64,
    pub cooking_list: Vec<CookingResult>,
}

/// チームシミュレーションの入力(Worker 1つ分)
pub struct TeamInput<'a> {
    pub config: &'a Config,
    pub target_num: usize,
    pub pickup: usize,
    pub top_list: Vec<usize>,
    pub fixed_pokemon_list: &'a [SimPokemon],
    pub target_pokemon_list: &'a [SimPokemon],
    pub box_pokemon_list: &'a [BoxPokemon],
}

/// 組み合わせの列挙(本家 getCombinations)
fn for_each_combination(pickup: usize, target_num: usize, top_list: &[usize], scores: &[f64], mut f: impl FnMut(&[usize], f64)) {
    if pickup == 0 {
        f(&[], 0.0);
        return;
    }
    for &top in top_list {
        let mut combination: Vec<usize> = (0..pickup).map(|i| if i == 0 { top } else { i + top }).collect();
        let limit: Vec<i64> = (0..pickup).map(|i| if i > 0 { target_num as i64 - pickup as i64 + i as i64 } else { top as i64 }).collect();
        'outer: loop {
            let mut about = 0.0;
            for &i in &combination {
                about += scores.get(i).copied().unwrap_or(f64::NAN);
            }
            f(&combination, about);

            let mut i = pickup;
            loop {
                if i == 0 {
                    break 'outer;
                }
                i -= 1;
                combination[i] += 1;
                if combination[i] as i64 > limit[i] {
                    if i == 0 {
                        break 'outer;
                    }
                } else {
                    for j in i + 1..pickup {
                        combination[j] = combination[j - 1] + 1;
                    }
                    break;
                }
            }
        }
    }
}

/// 本家 team-simulator の 'simulate'(Worker 1つ分)
pub fn simulate_team(input: &TeamInput, progress: &(dyn Fn(usize) + Sync)) -> Result<Vec<TeamResult>, String> {
    let m = master();
    let config = input.config;
    let sim = &config.simulation;
    let ts = &config.team_simulation;
    let food_len = m.foods.len();
    let mut best_result: Vec<TeamResult> = Vec::new();
    let mut border_score = -1.0_f64;

    // プール: 固定 + 対象
    let mut pool: Vec<SimPokemon> = input.fixed_pokemon_list.iter().chain(input.target_pokemon_list).cloned().collect();
    let target_offset = input.fixed_pokemon_list.len();

    // 固定ポケモンをボックス番号ごとにまとめる(同じ個体の別候補)
    let mut box_no_map: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, p) in input.fixed_pokemon_list.iter().enumerate() {
        box_no_map.entry(p.box_index().unwrap_or(usize::MAX)).or_default().push(i);
    }
    let mut fixed_combination_list: Vec<Vec<usize>> = vec![vec![]];
    for combination in box_no_map.values() {
        let mut next = Vec::new();
        for fc in &fixed_combination_list {
            for &p in combination {
                let mut c = fc.clone();
                c.push(p);
                next.push(c);
            }
        }
        fixed_combination_list = next;
    }

    let free_candy = config.candy.free_candy();

    let night_cap = if ts.night_cap_pikachu > 0.0 {
        let mut p = NightCapPikachu::get(ts.night_cap_pikachu);
        let berry_list: Option<Vec<String>> = if sim.field == "ワカクサ本島" {
            Some(sim.berry_list.clone())
        } else {
            m.field(&sim.field).and_then(|f| f.berry_list.clone())
        };
        if berry_list.is_some_and(|l| l.iter().any(|x| x == "ウブ")) {
            p.b_ep_d *= 2.0;
        }
        Some(p)
    } else {
        None
    };

    let mut cooking_chance_cache: HashMap<u64, ChanceWeekEffect> = HashMap::new();

    // 料理を良い順にソート
    let cooking_list = cooking::evaluate_lv_list(Some(sim));
    let sort_desc = |mut l: Vec<EvaluatedCooking>| {
        l.sort_by(|a, b| js::cmp_num(b.fix_add_energy, a.fix_add_energy));
        l
    };
    let target_cooking_list: Vec<EvaluatedCooking> = match sim.cooking_type() {
        Some(t) => sort_desc(
            cooking_list.iter().filter(|c| c.cooking().cooking_type == t && (c.enable || c.cooking().food_num == 0.0)).cloned().collect(),
        ),
        None => {
            if sim.cooking_type.as_deref().is_some_and(|x| !x.is_empty()) {
                // 本家では未知のカテゴリ名は undefined になり、料理なしとして扱われる
                Vec::new()
            } else {
                sort_desc(
                    cooking_list
                        .iter()
                        .filter(|c| sim.enable_cooking.get(&c.cooking().name).copied().unwrap_or(false) || c.cooking().food_num == 0.0)
                        .cloned()
                        .collect(),
                )
            }
        }
    };

    let mut cooking_timing_list: Vec<(f64, usize)> = Vec::new();
    match ts.day {
        Some(day) => {
            let mut i = 0.0;
            while i < ts.cooking_num {
                cooking_timing_list.push(((day + (i / 3.0).floor()) % 7.0, (i % 3.0) as usize));
                i += 1.0;
            }
        }
        None => {
            for i in 0..21 {
                cooking_timing_list.push(((i / 3) as f64, i % 3));
            }
        }
    }

    let mut food_common_rate = match ts.day {
        None => ((2.0 * 0.1 + 0.9) * 6.0 + (3.0 * 0.3 + 0.7)) / 7.0,
        Some(6.0) => 1.3,
        Some(_) => 1.1,
    };
    let initial_change = ts.initial_cooking_change.unwrap_or(0.0);
    if initial_change != 0.0 && !initial_change.is_nan() {
        food_common_rate = 0.0;
        let length = if ts.day.is_some() { 3 } else { 21 };
        let mut no_success = 1.0;
        for i in 0..length {
            let success = (if i >= 18 || ts.day == Some(6.0) { 0.3 } else { 0.1 }) + initial_change * no_success;
            no_success *= 1.0 - success;
            food_common_rate += success;
        }
        food_common_rate /= length as f64;
    }
    food_common_rate *= sim.cooking_weight;
    let mut food_energy_map: Vec<Option<f64>> = vec![None; food_len];
    for c in &cooking_list {
        let cooking = c.cooking();
        for cf in &cooking.food_list {
            let v = m.foods[cf.food].energy * cooking.rate * c.recipe_lv_bonus * food_common_rate;
            let cur = food_energy_map[cf.food].unwrap_or(0.0);
            food_energy_map[cf.food] = Some(js::max(if cur == 0.0 || cur.is_nan() { 0.0 } else { cur }, v));
        }
    }

    let mut simulator = PokemonSimulator::new(config.clone(), Mode::Team);
    let mut help_rate = HelpRate::new(config, None);

    let day_length = if ts.day.is_some() { 1.0 } else { 7.0 };
    let mut default_food_num = vec![0.0; food_len];
    for (name, &v) in &config.food_default_num {
        if let Some(i) = m.food_index(name) {
            default_food_num[i] = v;
        }
    }
    let required_type_num: Vec<(String, f64)> = ts.require.type_num.iter().filter(|(_, n)| **n > 0.0).map(|(t, n)| (t.clone(), *n)).collect();
    let target_scores: Vec<f64> = input.target_pokemon_list.iter().map(|p| p.score).collect();
    let metronome_len = m.metronome_target.len() as f64;
    let mut count = 0usize;

    for fixed in &fixed_combination_list {
        let fixed_about: f64 = fixed.iter().fold(0.0, |a, &i| a + pool[i].score);
        let mut members: Vec<usize> = Vec::with_capacity(5);

        let mut body = |combination: &[usize], about_score: f64, pool: &mut Vec<SimPokemon>| -> Result<(), String> {
            if (about_score + fixed_about) * day_length < border_score {
                return Ok(());
            }
            members.clear();
            members.extend_from_slice(fixed);
            members.extend(combination.iter().map(|&i| i + target_offset));

            let mut help_bonus_count = 0.0;
            let mut genki_bonus_count = 0.0;
            let mut shard_bonus_count = 0.0;
            let mut suimin_count = 0.0;
            let mut research_count = 0.0;
            let mut legend_num = 0;
            let mut use_total_shard = 0.0;
            let mut use_candies: HashMap<&str, f64> = HashMap::new();
            let mut type_set: HashMap<&str, HashSet<&str>> = HashMap::new();
            let mut type_count: HashMap<&str, f64> = HashMap::new();
            let mut no_dup: HashSet<Option<usize>> = HashSet::new();
            let (mut sp_berry, mut sp_food, mut sp_skill) = (0.0, 0.0, 0.0);

            for &i in members.iter() {
                let p = &pool[i];
                let base = p.base();
                help_bonus_count += p.has_sub_skill(SubSkillKind::HelpBonus) as i32 as f64;
                genki_bonus_count += p.has_sub_skill(SubSkillKind::GenkiBonus) as i32 as f64;
                shard_bonus_count += p.has_sub_skill(SubSkillKind::ShardBonus) as i32 as f64;
                suimin_count += p.has_sub_skill(SubSkillKind::SleepExpBonus) as i32 as f64;
                legend_num += base.legend as i32;
                use_total_shard += p.use_shard;
                let used = use_candies.entry(base.candy_name.as_str()).or_insert(0.0);
                *used += p.use_candy;
                if *used > config.candy.bag.get(&base.candy_name).copied().unwrap_or(0.0) + free_candy * 5.0 {
                    return Ok(());
                }
                match base.specialty {
                    Specialty::Berry => sp_berry += 1.0,
                    Specialty::Food => sp_food += 1.0,
                    Specialty::Skill => sp_skill += 1.0,
                    Specialty::All => {}
                }
                if sim.research_rank_max && p.has_sub_skill(SubSkillKind::ResearchExpBonus) {
                    research_count += 1.0;
                }
                type_set.entry(base.type_name.as_str()).or_default().insert(base.name.as_str());
                *type_count.entry(base.type_name.as_str()).or_insert(0.0) += 1.0;
                // 通常といつ育モードは同じPTに入らない
                if !no_dup.insert(p.box_index()) {
                    return Ok(());
                }
            }

            let req = &ts.require;
            let sp = |k: &str| req.specialty_num.get(k).copied().unwrap_or(0.0);
            if suimin_count < req.suimin_exp || sp_berry < sp("きのみ") || sp_food < sp("食材") || sp_skill < sp("スキル") {
                return Ok(());
            }
            for (t, n) in &required_type_num {
                if type_count.get(t.as_str()).copied().unwrap_or(0.0) < *n {
                    return Ok(());
                }
            }
            // 伝説は2匹以上入れられない(ラティアスとラティオスの組み合わせは可)
            if legend_num >= 2 {
                let has = |n: &str| members.iter().any(|&i| pool[i].base().name == n);
                if !(legend_num == 2 && has("ラティアス") && has("ラティオス")) {
                    return Ok(());
                }
            }
            if sim.fix
                && sim.fix_resource_mode != 0.0
                && let Some(shard) = config.candy.shard.filter(|x| *x != 0.0)
                && use_total_shard > shard
            {
                return Ok(());
            }
            let help_boost_counts: Vec<f64> = members.iter().map(|&i| type_set[pool[i].base().type_name.as_str()].len() as f64).collect();
            drop(type_set);

            // 個々の評価
            for &i in members.iter() {
                let mut p = std::mem::take(&mut pool[i]);
                let ctx = TeamCtx { pool: pool.as_slice(), members: &members, self_idx: i };
                simulator.calc_status(&mut p, help_bonus_count, genki_bonus_count, Some(&ctx));
                pool[i] = p;
            }
            help_rate.calc_team_heal(pool, &members, None);

            // ほっぺすりすりによる追加発動
            let base_spd: Vec<f64> = members.iter().map(|&i| simulator.calc_skill_per_day(&pool[i])).collect();
            let mut total_spd = base_spd.clone();
            let togedemaru: Vec<usize> =
                (0..members.len()).filter(|&k| pool[members[k]].skill().kind == SkillKind::Hoppesurisuri).collect();
            if !togedemaru.is_empty() {
                for _ in 0..10 {
                    let mut next = base_spd.clone();
                    for &e in &togedemaru {
                        let executor_spd = total_spd[e];
                        for t in 0..members.len() {
                            if e == t {
                                continue;
                            }
                            let select_rate = 1.0 / (members.len() as f64 - 1.0);
                            let reactivate = 1.0 - (1.0 - pool[members[t]].skill_rate).powf(pool[members[e]].fixed_skill_lv + 1.0);
                            next[t] += executor_spd * select_rate * reactivate;
                        }
                    }
                    let diff = js::max_of((0..members.len()).map(|k| (next[k] - total_spd[k]).abs()));
                    total_spd = next;
                    if diff < 1e-6 {
                        break;
                    }
                }
            }

            let mut energy = 0.0;
            let mut skill_shard = 0.0;
            let mut add_food_num = vec![0.0; food_len];
            let mut cooking_power_up_effect_list = vec![0.0; 21];
            let mut total_cooking_chance_effect = 0.0;
            for (k, &i) in members.iter().enumerate() {
                let mut p = std::mem::take(&mut pool[i]);
                let ctx = TeamCtx { pool: pool.as_slice(), members: &members, self_idx: i };
                let opt = HelpOption {
                    team: Some(ctx),
                    box_pokemon_list: input.box_pokemon_list,
                    help_boost_count: Some(help_boost_counts[k]),
                    additional_skill_per_day: total_spd[k] - base_spd[k],
                    ..Default::default()
                };
                let r = simulator.calc_help(&mut p, &opt);
                pool[i] = p;
                r?;
                let p = &mut pool[i];

                if p.day_help_rate * 100.0 < ts.require.day_help_rate || p.night_help_rate * 100.0 < ts.require.night_help_rate {
                    return Ok(());
                }
                energy += p.b_ep_d;
                energy += p.skill_energy_per_day;
                skill_shard += p.shard;
                for f in 0..food_len {
                    add_food_num[f] += p.food_count[f];
                }
                p.shard *= day_length;
                p.skill_per_day *= day_length;

                // 料理パワーアップを発動回数に応じて振り分ける
                let skill_num = (p.skill_per_day / if p.skill().kind == SkillKind::Metronome { metronome_len } else { 1.0 }).floor();
                let mut j = 0.0;
                while j < skill_num {
                    let slot = (j / skill_num * 3.0 * day_length).floor() as usize;
                    if let Some(s) = cooking_power_up_effect_list.get_mut(slot) {
                        *s += p.cooking_power_up_effect;
                    }
                    j += 1.0;
                }
                total_cooking_chance_effect += p.cooking_chance_effect;
            }
            if let Some(v) = ts.initial_cooking_power_up
                && v != 0.0
                && !v.is_nan()
            {
                cooking_power_up_effect_list[0] += v;
            }
            if let Some(nc) = &night_cap {
                for f in 0..food_len {
                    add_food_num[f] += nc.food_count[f];
                }
            }

            if ts.day.is_none() {
                energy *= 7.0;
                skill_shard *= 7.0;
                total_cooking_chance_effect *= 7.0;
                for x in &mut add_food_num {
                    *x *= 7.0;
                }
            }
            let mut food_num: Vec<f64> = default_food_num.iter().zip(&add_food_num).map(|(a, b)| a + b).collect();
            let mut use_food_num = vec![0.0; food_len];

            // 料理
            let mut cooking_results: Vec<CookingResult> = Vec::new();
            let mut remain_food_energy = 0.0;
            if sim.cooking_weight > 0.0 {
                let mut selected: Vec<Option<(&EvaluatedCooking, f64, f64, bool, usize)>> = Vec::new();
                for (count, &(week, i)) in cooking_timing_list.iter().enumerate() {
                    let pot_size = js::round(
                        ((if week == 6.0 { sim.pot_size * 2.0 } else { sim.pot_size }) + cooking_power_up_effect_list.get(count).copied().unwrap_or(f64::NAN))
                            * if sim.camp_ticket { 1.5 } else { 1.0 },
                    );
                    let mut best = None;
                    for c in &target_cooking_list {
                        let cooking = c.cooking();
                        if cooking.food_num <= pot_size && cooking.food_list.iter().all(|cf| food_num[cf.food] >= cf.num) {
                            for cf in &cooking.food_list {
                                food_num[cf.food] -= cf.num;
                                use_food_num[cf.food] += cf.num;
                            }
                            best = Some((c, week, pot_size, week == 6.0, (week * 3.0) as usize + i));
                            break;
                        }
                    }
                    selected.push(best);
                }

                let key = js::num_key(total_cooking_chance_effect);
                let chance = cooking_chance_cache
                    .entry(key)
                    .or_insert_with(|| cooking::get_chance_week_effect(total_cooking_chance_effect, ts.day, initial_change))
                    .clone();

                // 余った食材(エナジーの高い順)
                let mut remain: Vec<(usize, f64, f64)> = (0..food_len).map(|f| (f, food_num[f], m.foods[f].energy)).collect();
                remain.sort_by(|a, b| js::cmp_num(b.2, a.2));
                let mut remain_pos = 0usize;
                for s in &selected {
                    let Some((c, _week, pot_size, sunday, index)) = *s else {
                        return Err("料理が見つかりません".into());
                    };
                    let cooking = c.cooking();
                    let mut pot_remain = pot_size - cooking.food_num;
                    if ts.day.is_none() {
                        let mut add_food_power = 0.0;
                        if sim.remain_food_mode == 0.0 {
                            while pot_remain > 0.0 && remain_pos < remain.len() {
                                let r = &mut remain[remain_pos];
                                let num = js::min(pot_remain, r.1);
                                pot_remain -= num;
                                r.1 -= num;
                                add_food_power += r.2 * num;
                                if r.1 <= 0.0 {
                                    remain_pos += 1;
                                }
                            }
                        }
                        let sp = chance.success_probability_list.get(index).copied().unwrap_or(f64::NAN) * if sunday { 2.0 } else { 1.0 } + 1.0;
                        let e = (c.fix_energy + add_food_power) * sp * sim.cooking_weight;
                        cooking_results.push(CookingResult { cooking: c.index, fix_energy: c.fix_energy, energy: e, success_power: sp, add_energy: add_food_power, pot_size });
                        energy += e;
                    } else {
                        let sp = chance.success_probability_list.get(index % 3).copied().unwrap_or(f64::NAN) * if sunday { 2.0 } else { 1.0 } + 1.0;
                        let e = c.fix_energy * sp * sim.cooking_weight;
                        cooking_results.push(CookingResult { cooking: c.index, fix_energy: c.fix_energy, energy: e, success_power: sp, add_energy: 0.0, pot_size });
                        energy += e;
                    }
                }
                if sim.remain_food_mode == 1.0 {
                    remain_food_energy = remain[remain_pos..]
                        .iter()
                        .fold(0.0, |s, &(f, _, _)| s + food_energy_map[f].unwrap_or(0.0) * add_food_num[f] * sim.remain_food_rate);
                    energy += remain_food_energy;
                }
            }

            // フィールドボーナス
            let raw_energy = energy;
            energy *= (100.0 + sim.field_bonus) / 100.0;
            if energy.is_nan() {
                return Err("計算ロジックに誤りが見つかりました".into());
            }

            let se = &config.select_evaluate;
            let rrm = sim.research_rank_max;
            let (energy_shard, research_exp, today_shard, today_research_exp);
            if let Some(day) = ts.day {
                let shard_energy = energy * (7.0 - day);
                energy_shard = shard_energy / se.shard_energy_rate;
                research_exp = if rrm { shard_energy / se.shard_energy_rate * 0.5 } else { 0.0 };
                let before = ts.before_energy.unwrap_or(0.0);
                today_shard = (before + energy) / se.shard_energy_rate;
                today_research_exp = if rrm { (before + energy) / se.shard_energy_rate * 0.5 } else { 0.0 };
            } else {
                energy_shard = energy * 4.0 / se.shard_energy_rate;
                today_shard = energy_shard;
                research_exp = if rrm { energy * 4.0 / se.shard_energy_rate * 0.5 } else { 0.0 };
                today_research_exp = research_exp;
            }
            let bonus_shard = today_shard * shard_bonus_count * 0.06 + today_research_exp * research_count * 0.09 * 0.5;
            let shard_rate = (bonus_shard + skill_shard) / (energy_shard + research_exp);
            let score = energy * (shard_rate * sim.shard_weight / 100.0 + 1.0);

            if border_score < score {
                let mut list: Vec<TeamMember> = members
                    .iter()
                    .map(|&i| {
                        let mut p = pool[i].clone();
                        p.self_heal_list.clear();
                        p.other_heal_list.clear();
                        p.sanitize_non_finite();
                        if ts.day.is_none() {
                            for x in &mut p.food_count {
                                *x *= 7.0;
                            }
                        }
                        let shard_bonus = p.shard
                            + if p.has_sub_skill(SubSkillKind::ShardBonus) { today_shard * 0.06 } else { 0.0 }
                            + if p.has_sub_skill(SubSkillKind::ResearchExpBonus) { today_research_exp * 0.09 } else { 0.0 };
                        TeamMember::Pokemon { pokemon: Box::new(p), shard_bonus }
                    })
                    .collect();
                if let Some(nc) = &night_cap {
                    let mut nc = nc.clone();
                    if ts.day.is_none() {
                        for x in &mut nc.food_count {
                            *x *= 7.0;
                        }
                    }
                    list.push(TeamMember::NightCapPikachu(nc));
                }
                best_result.push(TeamResult {
                    before_energy: ts.before_energy,
                    energy,
                    raw_energy,
                    energy_shard,
                    bonus_shard,
                    skill_shard,
                    today_shard,
                    today_research_exp,
                    score,
                    shard_bonus_count,
                    research_exp_bonus_count: research_count,
                    pokemon_list: list,
                    use_food_num,
                    add_food_num,
                    default_food_num: default_food_num.clone(),
                    food_num,
                    cooking_power_up_effect_list,
                    remain_food_energy,
                    cooking_list: cooking_results,
                });
                best_result.sort_by(|a, b| js::cmp_num(b.score, a.score));
                best_result.truncate(10);
                if best_result.len() >= 10 {
                    border_score = best_result[best_result.len() - 1].score;
                }
            }
            Ok(())
        };

        let mut err: Option<String> = None;
        for_each_combination(input.pickup, input.target_num, &input.top_list, &target_scores, |combination, about| {
            if err.is_some() {
                return;
            }
            if let Err(e) = body(combination, about, &mut pool) {
                err = Some(e);
            }
            count += 1;
            if count.is_multiple_of(1000) {
                progress(1000);
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
    }
    progress(count % 1000);
    Ok(best_result)
}

/// 画面から指定するチームシミュレーションの条件(本家 simulation.vue)
#[derive(Debug, Clone)]
pub struct TeamSimulationRequest {
    /// 曜日(0:月曜～6:日曜)、None は1週間
    pub target_day: Option<f64>,
    /// 何時間分を計算するか(24 は1日すべて)
    pub target_hour: f64,
    /// 初期の料理チャンス(%)
    pub initial_cooking_change: f64,
    pub initial_cooking_power_up: f64,
    pub before_energy: f64,
}

/// チームシミュレーション全体の結果
#[derive(Debug, Clone, Default)]
pub struct TeamSimulationResult {
    pub target_day: Option<f64>,
    pub team_list: Vec<TeamResult>,
}

/// 簡易診断(本家 `pages/tmp-evaluate.vue` の `evaluate()`)。簡易診断表を作り、入力した個体だけを概算する
pub fn simulate_tmp_evaluate(config: &Config, pokemon: &BoxPokemon, progress: &dyn crate::evaluate_table::Progress) -> Result<BoxResult, String> {
    let mut c = config.clone();
    // 簡易診断は設定画面の選択状態にかかわらず、常にグラフの5レベルを計算する
    c.tmp_evaluate.level_list = [(10, false), (25, false), (30, true), (50, true), (60, true), (70, true), (80, true)].into_iter().collect();
    let table = crate::evaluate_table::simulate_temporary(&c, &pokemon.name, progress)?;
    c.select_evaluate = c.tmp_evaluate.clone();
    c.sleep_time = c.tmp_evaluate.sleep_time.unwrap_or(c.sleep_time);
    c.check_freq = c.tmp_evaluate.check_freq.unwrap_or(c.check_freq);
    c.worker_num = c.tmp_evaluate.worker_num.unwrap_or(c.worker_num);
    c.pure_mint = false;
    progress.set(0.9, "選択したポケモンを評価しています…");
    let result = simulate_box(&c, std::slice::from_ref(pokemon), Some(&table), false)?;
    result.into_iter().next().ok_or_else(|| "評価できませんでした".to_string())
}

/// ボックス概算(本家 `PokemonBox.simulation` の basic → assist)
pub fn simulate_box(config: &Config, box_list: &[BoxPokemon], evaluate_table: Option<&EvaluateTable>, with_assist: bool) -> Result<Vec<BoxResult>, String> {
    let n = config.worker_num.max(1.0) as usize;
    let chunks: Vec<Vec<BoxResult>> = (0..n)
        .into_par_iter()
        .map(|i| {
            let s = box_list.len() * i / n;
            let e = box_list.len() * (i + 1) / n;
            BoxSimulator::new(config.clone()).basic(&box_list[s..e], evaluate_table)
        })
        .collect::<Result<_, _>>()?;
    let basic: Vec<BoxResult> = chunks.into_iter().flatten().collect();
    if !with_assist {
        return Ok(basic);
    }
    let len = basic.len();
    let out: Vec<Vec<BoxResult>> = (0..n)
        .into_par_iter()
        .map(|i| {
            let s = len * i / n;
            let e = len * (i + 1) / n;
            BoxSimulator::new(config.clone()).assist(&basic, s..e)
        })
        .collect();
    Ok(out.into_iter().flatten().collect())
}

/// 本家 simulation.vue の `simulation()`
pub fn run_team_simulation(
    base_config: &Config,
    box_list: &[BoxPokemon],
    evaluate_table: Option<&EvaluateTable>,
    request: &TeamSimulationRequest,
    progress: &(dyn Fn(f64) + Sync),
) -> Result<TeamSimulationResult, String> {
    let m = master();
    let mut config = base_config.clone();
    config.team_simulation.before_energy = Some(request.before_energy);
    if config.food_unlimited {
        config.food_default_num = m.foods.iter().map(|f| (f.name.clone(), 9999.0)).collect();
    }
    if config.simulation.mode == 2.0 {
        config.simulation.berry_energy_weight = Some(0.0);
        config.simulation.skill_energy_ignore = Some(0.0);
        config.simulation.shard_weight = 0.0;
    }
    if let Some(day) = request.target_day {
        config.simulation.shard_to_energy = Some(config.select_evaluate.shard_energy_rate / (7.0 - day));
        config.team_simulation.day = Some(day);
        config.team_simulation.time_length = if request.target_hour != 24.0 { Some(request.target_hour / 24.0) } else { None };
    }
    config.team_simulation.initial_cooking_change = Some(request.initial_cooking_change / 100.0);
    config.team_simulation.initial_cooking_power_up = Some(request.initial_cooking_power_up);

    progress(0.0);
    let filtered = filter_pokemon(box_list, &config.simulation.filter).pokemon_list;
    let mut pokemon_list = simulate_box(&config, &filtered, evaluate_table, true)?;
    for r in &mut pokemon_list {
        r.pokemon.sanitize_non_finite();
    }
    progress(1.0 / 9.0);

    let fixed: Vec<SimPokemon> =
        pokemon_list.iter().filter(|x| x.pokemon.box_pokemon.as_ref().and_then(|b| b.fix) == Some(1.0)).map(|x| x.pokemon.clone()).collect();
    let mut rest: Vec<SimPokemon> =
        pokemon_list.iter().filter(|x| x.pokemon.box_pokemon.as_ref().and_then(|b| b.fix).is_none()).map(|x| x.pokemon.clone()).collect();
    let pickup = 5usize.saturating_sub(fixed.iter().map(|x| x.box_index()).collect::<HashSet<_>>().len());

    let ts = &config.team_simulation;
    let mut targets: Vec<SimPokemon> = Vec::new();
    let mut take = |rest: &mut Vec<SimPokemon>, key: fn(&SimPokemon) -> f64, n: f64| {
        rest.sort_by(|a, b| js::cmp_num(key(b), key(a)));
        let n = (n.max(0.0) as usize).min(rest.len());
        targets.extend(rest.drain(..n));
    };
    if config.simulation.mode != 2.0 {
        take(&mut rest, |p| p.b_ep_d, ts.max_rank_berry);
        take(&mut rest, |p| p.f_ep_d, ts.max_rank_food);
    } else {
        take(&mut rest, |p| p.f_ep_d, ts.max_rank_food + ts.max_rank_berry);
    }
    take(&mut rest, |p| p.score, ts.max_rank_all);
    take(&mut rest, |p| p.energy_per_day, ts.max_rank_not_support);
    let target_num = targets.len();

    // げんき計算のキャッシュを効かせるため、ヒーラー、自身回復、その他の順に並べる
    let truthy = |x: f64| x != 0.0 && !x.is_nan();
    let mut ordered: Vec<SimPokemon> = targets.iter().filter(|p| truthy(p.other_heal)).cloned().collect();
    ordered.extend(targets.iter().filter(|p| !truthy(p.other_heal) && truthy(p.self_heal)).cloned());
    ordered.extend(targets.iter().filter(|p| !truthy(p.other_heal) && !truthy(p.self_heal)).cloned());
    let mut targets = ordered;

    // タイプ指定があるのに対象ポケモンがいなければ追加
    for berry in &m.berries {
        if ts.require.type_num.get(&berry.type_name).is_some_and(|n| *n != 0.0 && !n.is_nan())
            && !targets.iter().any(|x| x.base().type_name == berry.type_name)
        {
            match rest.iter().find(|x| x.base().type_name == berry.type_name) {
                Some(top) => targets.push(top.clone()),
                None => return Ok(TeamSimulationResult { target_day: ts.day, team_list: Vec::new() }),
            }
        }
    }

    // 組み合わせを Worker(スレッド)ごとに分割
    let worker_num = config.worker_num.max(1.0) as usize;
    let mut params: Vec<(f64, Vec<usize>)> = vec![(0.0, Vec::new()); worker_num];
    if target_num >= pickup {
        for i in 0..=(target_num - pickup) {
            let mut size = 1.0;
            for j in 1..pickup {
                size *= (target_num - i - j) as f64;
                size /= j as f64;
            }
            let min = params.iter_mut().fold(None::<&mut (f64, Vec<usize>)>, |acc, x| match acc {
                Some(a) if a.0 <= x.0 => Some(a),
                _ => Some(x),
            });
            if let Some(min) = min {
                min.0 += size;
                min.1.push(i);
            }
        }
    }
    targets.sort_by(|a, b| js::cmp_num(b.score, a.score));

    let total: f64 = params.iter().map(|x| x.0).sum::<f64>().max(1.0) * fixed.iter().map(|x| x.box_index()).collect::<HashSet<_>>().len().max(1) as f64;
    let done = std::sync::atomic::AtomicUsize::new(0);
    let tick = |n: usize| {
        let d = done.fetch_add(n, std::sync::atomic::Ordering::Relaxed) + n;
        progress(1.0 / 9.0 + d as f64 / total * 8.0 / 9.0);
    };

    let results: Vec<Vec<TeamResult>> = params
        .into_par_iter()
        .map(|(_, top_list)| {
            let input = TeamInput {
                config: &config,
                target_num,
                pickup,
                top_list,
                fixed_pokemon_list: &fixed,
                target_pokemon_list: &targets,
                box_pokemon_list: box_list,
            };
            simulate_team(&input, &tick)
        })
        .collect::<Result<_, _>>()?;
    let mut team_list: Vec<TeamResult> = results.into_iter().flatten().collect();
    team_list.sort_by(|a, b| js::cmp_num(b.score, a.score));
    team_list.truncate(ts.result_num.max(0.0) as usize);
    progress(1.0);
    Ok(TeamSimulationResult { target_day: ts.day, team_list })
}
