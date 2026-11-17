//! ボックスのポケモンの概算(本家 `models/pokemon-box/pokemon-box-worker.ts`)

use std::collections::HashSet;

use crate::check_list::{CheckHit, CheckList, CheckTarget};
use crate::config::Config;
use crate::data::{master, Specialty, SubSkillKind};
use crate::evaluate_table::{EvalInfo, EvalLv, EvaluateResult, EvaluateTable};
use crate::filter::filter_pokemon;
use crate::js;
use crate::sim_pokemon::{BoxPokemon, SimPokemon};
use crate::simulator::{nature_index, HelpOption, Mode, PokemonSimulator};

/// ボックス概算の結果1件
#[derive(Debug, Clone)]
pub struct BoxResult {
    pub pokemon: SimPokemon,
    pub evaluate_result: Option<EvaluateResult>,
    /// 進化前の名前
    pub before_name: String,
    /// ヒットしたチェックリスト
    pub hit_check_list: Vec<CheckHit>,
}

pub struct BoxSimulator {
    pub config: Config,
    pub simulator: PokemonSimulator,
    pub evaluate_simulator: PokemonSimulator,
}

const EVALUATE_KEYS: [&str; 5] = ["energy", "berry", "food", "skill", "specialty"];

/// パーセンタイル表上の位置(0～1)
pub fn get_rate(list: &[f64], num: f64) -> f64 {
    if list.is_empty() {
        return f64::NAN;
    }
    let max = (list.len() - 1) as f64;
    let last_le = list.iter().rposition(|&x| x <= num).map(|i| i as f64).unwrap_or(-1.0);
    let lower = js::min(js::max(last_le, 0.0), max);
    let lower_v = list[lower as usize];
    let upper = if lower_v >= num { lower } else { js::min(lower + 1.0, max) };
    if upper == lower {
        upper / max
    } else {
        ((num - lower_v) / (list[upper as usize] - lower_v) + lower) / max
    }
}

impl BoxSimulator {
    pub fn new(config: Config) -> BoxSimulator {
        BoxSimulator {
            simulator: PokemonSimulator::new(config.clone(), Mode::About),
            evaluate_simulator: PokemonSimulator::new(config.clone(), Mode::Select),
            config,
        }
    }

    /// 本家の 'basic'
    pub fn basic(&mut self, pokemon_list: &[BoxPokemon], evaluate_table: Option<&EvaluateTable>) -> Result<Vec<BoxResult>, String> {
        let m = master();
        let config = self.config.clone();
        let lv_list = config.select_evaluate.enabled_levels();
        let check_list = (config.simulation.fix || config.cleaning).then(|| CheckList::new(&config, evaluate_table));

        let fixable_set: Option<HashSet<usize>> = if config.simulation.fix {
            Some(filter_pokemon(pokemon_list, &config.simulation.fix_filter).pokemon_list.iter().map(|x| x.index).collect())
        } else {
            None
        };
        let mut evaluate_types: Vec<bool> = vec![false];
        if config.pure_mint {
            evaluate_types.push(true);
        }

        let mut result: Vec<BoxResult> = Vec::new();
        for bx in pokemon_list {
            let original_base = m.pokemon(&bx.name).ok_or_else(|| format!("知らないポケモン: {}", bx.name))?;
            let mut add_pokemon_list: Vec<String> = Vec::new();
            let mut evaluate_result: Option<EvaluateResult> = None;
            let mut fix = false;
            let mut check_list_hit: Vec<CheckHit> = Vec::new();

            let mut after_list = original_base.after_list.clone();
            if bx.name == "エレズン" {
                const HIGH: [&str; 13] = [
                    "いじっぱり", "やんちゃ", "ゆうかん", "わんぱく", "のうてんき", "うっかりや", "なまいき",
                    "せっかち", "ようき", "むじゃき", "がんばりや", "すなお", "きまぐれ",
                ];
                after_list = vec![if HIGH.contains(&bx.nature.as_str()) {
                    "ストリンダー(ハイなすがた)".to_string()
                } else {
                    "ストリンダー(ローなすがた)".to_string()
                }];
            }

            // 厳選
            if let Some(table) = evaluate_table {
                let food_index_list: Vec<i64> = bx
                    .food_list
                    .iter()
                    .map(|f| original_base.food_name_list.iter().position(|x| x == f).map(|i| i as i64).unwrap_or(-1))
                    .collect();
                if !food_index_list.contains(&-1) {
                    for after in &after_list {
                        let Some(after_table) = table.pokemon.get(after) else { continue };
                        let er = evaluate_result.get_or_insert_with(EvaluateResult::default);
                        let after_pokemon = m.pokemon(after).ok_or_else(|| format!("知らないポケモン: {after}"))?;
                        let mut evaluate_max_score = 0.0_f64;
                        let mut evaluate_specialty_max_score = 0.0_f64;

                        for &lv in &lv_list {
                            let (Some(&healer), Some(&support)) = (table.healer.get(&lv), table.support.get(&lv)) else {
                                continue;
                            };
                            let food_num = if lv < 30 { 1 } else if lv < 60 { 2 } else { 3 };
                            let mut simulated = self.evaluate_simulator.from_evaluate(after_pokemon, lv as f64, &bx.food_list)?;
                            let sub_skill_num = crate::simulator::enabled_sub_skill_len(lv as f64);
                            let sub_skill_list: Vec<String> = m
                                .use_silver_seed(&bx.sub_skill_list, Some(&config.select_evaluate.silver_seed))
                                .into_iter()
                                .take(sub_skill_num)
                                .collect();
                            let sub_skill_idx: Vec<Option<usize>> =
                                sub_skill_list.iter().map(|n| m.sub_skill(n).map(|s| sub_skill_pos(s.id))).collect();
                            let food_key: String = food_index_list.iter().take(food_num).map(|x| x.to_string()).collect();

                            for &pure_mint in &evaluate_types {
                                let nature = if pure_mint {
                                    let mint_name = config.pure_mint_nature.as_deref().unwrap_or("まじめ");
                                    m.nature(mint_name).map(|n| nature_index(&n.name))
                                } else {
                                    m.nature(&bx.nature).map(|n| nature_index(&n.name))
                                };
                                self.evaluate_simulator.select_evaluate(&mut simulated, &sub_skill_idx, nature, healer, support)?;
                                let score = self.evaluate_simulator.select_evaluate_to_score(
                                    &simulated,
                                    sub_skill_list.iter().any(|x| x == "ゆめのかけらボーナス"),
                                    sub_skill_list.iter().any(|x| x == "リサーチEXPボーナス"),
                                    true,
                                );

                                let entry = er.entry_mut(EvalLv::Lv(lv), after);
                                for (value, key) in [
                                    (score, "energy"),
                                    (simulated.berry_num_per_day, "berry"),
                                    (simulated.food_num_per_day, "food"),
                                    (simulated.skill_per_day, "skill"),
                                ] {
                                    let list = after_table.get(&lv).and_then(|x| x.get(&food_key)).map(|p| p.get(key)).unwrap_or(&[]);
                                    let rate = get_rate(list, value);
                                    let ratio = value / list.get(config.simulation.select_border as usize).copied().unwrap_or(f64::NAN);
                                    let item = EvalInfo {
                                        name: Some(after.clone()),
                                        rate,
                                        ratio,
                                        score: if config.simulation.select_type == 0.0 { rate } else { ratio },
                                        value,
                                        pure_mint: None,
                                    };
                                    let slot = entry.get_mut(key);
                                    if pure_mint {
                                        if let Some(s) = slot {
                                            s.pure_mint = Some(Box::new(item));
                                        }
                                    } else {
                                        *slot = Some(item);
                                    }
                                }
                                entry.food_num_list = after_pokemon.food_list.iter().map(|f| simulated.food_count[f.food]).collect();
                                let diag = crate::evaluate_table::Diagnostic {
                                    berry_energy: simulated.b_ep_d,
                                    food_num_map: simulated.food_list.iter().map(|f| (m.foods[f.food].name.clone(), simulated.food_count[f.food])).collect(),
                                    skill_activation_num: simulated.skill_per_day,
                                    pure_mint: None,
                                };
                                if pure_mint {
                                    if let Some(d) = &mut entry.diagnostic {
                                        d.pure_mint = Some(Box::new(diag));
                                    }
                                } else {
                                    entry.diagnostic = Some(diag);
                                }
                                let spec_src = match after_pokemon.specialty {
                                    Specialty::Berry => Some("berry"),
                                    Specialty::Food => Some("food"),
                                    Specialty::Skill => Some("skill"),
                                    Specialty::All => None,
                                };
                                if let Some(src) = spec_src {
                                    if !pure_mint {
                                        entry.specialty = entry.get(src).cloned();
                                    } else {
                                        let pm = entry.get(src).and_then(|x| x.pure_mint.clone());
                                        if let Some(s) = &mut entry.specialty {
                                            s.pure_mint = pm;
                                        }
                                    }
                                }

                                evaluate_max_score = js::max(evaluate_max_score, entry.energy.as_ref().map(|x| x.score).unwrap_or(f64::NAN));
                                if let Some(s) = &entry.specialty {
                                    evaluate_specialty_max_score = js::max(evaluate_specialty_max_score, s.score);
                                }
                            }
                        }

                        if let Some(&first_lv) = lv_list.first()
                            && er.has_lv(EvalLv::Lv(first_lv))
                        {
                            let names: Vec<String> = er.names(EvalLv::Lv(first_lv));
                            er.clear_lv(EvalLv::Max);
                            for name in names {
                                let max_entry = er.entry_mut(EvalLv::Max, &name).clone();
                                let mut max_entry = max_entry;
                                for key in EVALUATE_KEYS {
                                    let mut cands: Vec<EvalInfo> = lv_list
                                        .iter()
                                        .filter_map(|&lv| er.get(EvalLv::Lv(lv), &name).and_then(|e| e.get(key).cloned()))
                                        .collect();
                                    cands.sort_by(|a, b| js::cmp_num(b.score, a.score));
                                    *max_entry.get_mut(key) = cands.into_iter().next();
                                }
                                *er.entry_mut(EvalLv::Max, &name) = max_entry;
                            }
                        }

                        // チェックリスト(本家は同じ項目でも別オブジェクトのため重複して積まれる)
                        let this_check_hit = match &check_list {
                            Some(cl) => {
                                let food_combination: String =
                                    food_index_list.iter().map(|&x| char::from_u32((65 + x) as u32).unwrap_or('?')).collect();
                                let only = er.only(after, &lv_list);
                                let hit = cl.get_checked(&CheckTarget { base_name: after, food_combination: &food_combination, evaluate_result: &only });
                                let n = hit.len();
                                check_list_hit.extend(hit);
                                n
                            }
                            None => 0,
                        };
                        let this_fix = config.simulation.fix
                            && fixable_set.as_ref().is_some_and(|s| s.contains(&bx.index))
                            && ((config.simulation.fix_check_list && this_check_hit > 0)
                                || config.simulation.fix_border.is_some_and(|b| evaluate_max_score >= b / 100.0)
                                || config.simulation.fix_border_specialty.is_some_and(|b| evaluate_specialty_max_score >= b / 100.0));
                        fix |= this_fix;

                        if !config.cleaning
                            && this_fix
                            && (config.simulation.fix_evolve || config.simulation.fix_resource_mode == 1.0)
                            && (!config.simulation.fix_evolve_exclude_sleep
                                || !m.pokemon(after).and_then(|p| p.require_sleep.get(&bx.name)).copied().unwrap_or(false))
                        {
                            add_pokemon_list.push(after.clone());
                        }
                    }
                }
            }

            if add_pokemon_list.is_empty() {
                add_pokemon_list.push(bx.name.clone());
            }

            let mut this_result: Vec<BoxResult> = Vec::new();
            let n = add_pokemon_list.len();
            for (j, pokemon_name) in add_pokemon_list.iter().enumerate() {
                let base = m.pokemon(pokemon_name).ok_or_else(|| format!("知らないポケモン: {pokemon_name}"))?;
                let mut skill_lv = bx.skill_lv;
                if let Some(lv) = &mut skill_lv {
                    *lv += (base.evolve_lv - original_base.evolve_lv) as f64;
                    let max = m.pokemon_skill(base).max_lv() as f64;
                    if max < *lv {
                        *lv = max;
                    }
                }
                let use_candy = if config.simulation.fix_resource_mode == 0.0 {
                    0.0
                } else {
                    // 本家では evolve.candy が null の進化先(ドンカラス等)は null のまま渡り、加算時に +null = +0 として扱われる
                    base.evolve_candy_map.get(&bx.name).copied().flatten().unwrap_or(0.0)
                };
                let require_lv = if bx.name != *pokemon_name { base.evolve.lv.filter(|x| *x != 0.0).unwrap_or(0.0) } else { 0.0 };
                let mut modified = bx.clone();
                modified.name = pokemon_name.clone();
                modified.skill_lv = skill_lv;
                let mut simulated = self.simulator.from_box(&modified, fix, use_candy, require_lv)?;

                // 進化仮定を計算しても必要レベルに達していない場合はキャンセル
                if bx.name != *pokemon_name
                    && let Some(need) = base.evolve.lv
                    && simulated.lv < need
                {
                    if j == n - 1 && this_result.is_empty() {
                        simulated = self.simulator.from_box(bx, fix, 0.0, 0.0)?;
                    } else {
                        continue;
                    }
                }
                simulated.before_name = Some(bx.name.clone());
                simulated.after_list = after_list.clone();

                let mut er_out = None;
                if let Some(er) = &evaluate_result {
                    let mut out = if *pokemon_name == bx.name { er.clone() } else { er.only(pokemon_name, &lv_list) };
                    if lv_list.first().is_some_and(|&lv| out.has_lv(EvalLv::Lv(lv))) {
                        out.compute_best(&lv_list, config.pure_mint);
                    }
                    er_out = Some(out);
                }
                this_result.push(BoxResult {
                    pokemon: simulated,
                    evaluate_result: er_out,
                    before_name: bx.name.clone(),
                    hit_check_list: check_list_hit.clone(),
                });
            }
            result.extend(this_result);
        }

        if config.simulation.bag_over_operation {
            let extra: Vec<BoxResult> = result
                .iter()
                .filter(|x| x.pokemon.base().specialty.is_berry_like() || x.pokemon.has_sub_skill(SubSkillKind::BerryNumS))
                .map(|x| {
                    let mut c = x.clone();
                    c.pokemon.bag_over_operation = true;
                    c.pokemon.bag = -100.0;
                    c.pokemon.food_rate = 0.0;
                    c.pokemon.skill_rate = 0.0;
                    c.pokemon.ceil_skill_rate = 0.0;
                    c
                })
                .collect();
            result.extend(extra);
        }

        let field_bonus = config.simulation.field_bonus;
        let shard_to_energy = config.simulation.shard_to_energy.unwrap_or(config.select_evaluate.shard_energy_rate / 4.0);
        for r in &mut result {
            let p = &mut r.pokemon;
            let help_bonus = (if p.has_sub_skill(SubSkillKind::HelpBonus) { 1.0 } else { 0.0 }) + config.simulation.help_bonus.unwrap_or(0.0);
            let genki_bonus = if p.has_sub_skill(SubSkillKind::GenkiBonus) { 1.0 } else { 0.0 };
            self.simulator.calc_status(p, help_bonus, genki_bonus, None);
            self.simulator.calc_team_heal(std::slice::from_mut(p), &[0]);
            self.simulator.calc_help(p, &HelpOption { box_pokemon_list: pokemon_list, ..Default::default() })?;

            let mut tmp = p.energy_per_day * (100.0 + field_bonus) / 100.0;
            tmp += (tmp * if config.simulation.research_rank_max { 0.5 } else { 0.0 } + p.shard * shard_to_energy) * config.simulation.shard_weight / 100.0;
            p.tmp_score = Some(tmp);
        }
        Ok(result)
    }

    /// 本家の 'assist'。`targets` は `based` 内のインデックス。
    pub fn assist(&mut self, based: &[BoxResult], targets: std::ops::Range<usize>) -> Vec<BoxResult> {
        let config = self.config.clone();
        let list: Vec<&SimPokemon> = based.iter().map(|x| &x.pokemon).collect();
        let fix_list: Vec<&SimPokemon> = list.iter().copied().filter(|x| x.box_pokemon.as_ref().and_then(|b| b.fix) == Some(1.0)).collect();
        let kouho: Vec<&SimPokemon> = list.iter().copied().filter(|x| x.box_pokemon.as_ref().and_then(|b| b.fix).is_none()).collect();
        // JS の slice(0, n - 固定数)。負数は末尾から数える
        let js_slice = |list: &mut Vec<&SimPokemon>, n: usize| {
            let end = n as i64 - fix_list.len() as i64;
            let end = if end >= 0 { end as usize } else { (list.len() as i64 + end).max(0) as usize };
            list.truncate(end);
        };

        let top = |key: fn(&SimPokemon) -> f64, n: usize| -> Vec<&SimPokemon> {
            let mut sorted = kouho.clone();
            sorted.sort_by(|a, b| js::cmp_num(key(b), key(a)));
            js_slice(&mut sorted, n);
            sorted.extend(fix_list.iter().copied());
            sorted
        };
        let energy_top5 = top(|x| x.energy_per_day, 5);
        let help_bonus_top5 = top(|x| x.tmp_score.unwrap_or(f64::NAN), 5);
        let berry_energy_top5 = top(|x| x.berry_energy, 5);
        let pickup_top = top(|x| x.pickup_energy_per_help, 6);

        let mut heal_check: Vec<&SimPokemon> = kouho.iter().copied().filter(|x| x.heal_effect == 0.0).collect();
        heal_check.sort_by(|a, b| js::cmp_num(b.tmp_score.unwrap_or(f64::NAN), a.tmp_score.unwrap_or(f64::NAN)));
        js_slice(&mut heal_check, 6);
        let border = heal_check.last().and_then(|x| x.tmp_score).unwrap_or(0.0);
        heal_check.extend(kouho.iter().copied().filter(|x| x.heal_effect != 0.0 && x.tmp_score.unwrap_or(f64::NAN) > border));
        heal_check.extend(fix_list.iter().copied());
        heal_check.sort_by(|a, b| js::cmp_num(b.tmp_score.unwrap_or(f64::NAN), a.tmp_score.unwrap_or(f64::NAN)));

        let box_idx = |p: &SimPokemon| p.box_index();
        let field_bonus = config.simulation.field_bonus;
        let shard_to_energy = config.simulation.shard_to_energy.unwrap_or(config.select_evaluate.shard_energy_rate / 4.0);
        let shard_energy_rate = config.select_evaluate.shard_energy_rate;

        let mut out = Vec::with_capacity(targets.len());
        for i in targets {
            let mut r = based[i].clone();
            let me_idx = box_idx(&r.pokemon);
            let others = |l: &[&SimPokemon], n: usize| -> Vec<SimPokemon> {
                l.iter().filter(|x| box_idx(x) != me_idx).take(n).map(|x| (*x).clone()).collect()
            };
            let mut support_energy = 0.0;
            let mut support_shard = 0.0;
            let p = &r.pokemon;

            if p.has_sub_skill(SubSkillKind::HelpBonus) {
                for sub in others(&help_bonus_top5, 4) {
                    let effect = (1.0 - sub.speed_bonus) * (1.0 / 0.65 - 1.0 / 0.7);
                    support_energy += sub.energy_per_day * effect;
                    support_shard += sub.shard * (1.0 - effect) * (1.0 / 0.65 - 1.0 / 0.7);
                }
            }
            if p.has_sub_skill(SubSkillKind::ShardBonus) {
                let mut l = others(&energy_top5, 4);
                l.push(p.clone());
                for sub in l {
                    support_shard += sub.energy_per_day / shard_energy_rate * 0.06;
                }
            }
            if config.simulation.research_rank_max && p.has_sub_skill(SubSkillKind::ResearchExpBonus) {
                let mut l = others(&help_bonus_top5, 4);
                l.push(p.clone());
                for sub in l {
                    support_shard += sub.energy_per_day / shard_energy_rate / 2.0 * 0.09;
                }
            }

            // げんき回復系スキル評価
            if p.other_heal != 0.0 && !p.other_heal.is_nan() && !config.simulation.genki_full {
                let hr = self.simulator.get_help_rate(&p.other_heal_list);
                let mut list: Vec<(f64, f64, f64)> = heal_check
                    .iter()
                    .filter(|x| box_idx(x) != me_idx)
                    .map(|sub| {
                        let day = (24.0 - config.sleep_time) * 3600.0 / sub.speed * hr.day;
                        let night = config.sleep_time * 3600.0 / sub.speed * hr.night;
                        let add = js::max((day + night) / (sub.day_help_num + sub.night_help_num) - 1.0, 0.0);
                        (sub.tmp_score.unwrap_or(f64::NAN) * add, sub.energy_per_day * add, sub.shard * add)
                    })
                    .collect();
                list.sort_by(|a, b| js::cmp_num(b.0, a.0));
                list.truncate(4);
                support_energy += list.iter().fold(0.0, |a, x| a + x.1);
                support_shard += list.iter().fold(0.0, |a, x| a + x.2);
            }

            if p.bag > 0.0 {
                use crate::data::SkillKind::*;
                for sw in &p.skill_weight_list {
                    let skill = sw.skill();
                    let len = skill.max_lv() as f64;
                    let lv = if len >= p.fixed_skill_lv { p.fixed_skill_lv } else { len };
                    let effect = if lv >= 1.0 && lv.fract() == 0.0 { skill.effect.get(lv as usize - 1) } else { None };
                    let ev = |f: fn(&crate::data::SkillEffect) -> Option<f64>| effect.and_then(f).unwrap_or(f64::NAN);
                    let mut s = 0.0;
                    match skill.kind {
                        Bakenokawa | BerryBurst => {
                            let sum = others(&berry_energy_top5, 4).iter().fold(0.0, |a, x| a + x.berry_energy);
                            if skill.kind == Bakenokawa {
                                let spd = p.skill_per_day;
                                let success = 1.0 - (1.0 - skill.success.unwrap_or(f64::NAN)).powf(spd);
                                s += sum * p.burst_bonus * (success * (spd + 2.0) + (1.0 - success) * spd) / spd;
                            } else {
                                s += sum * p.burst_bonus;
                            }
                        }
                        Mikazukinoinori => {
                            let sum = others(&berry_energy_top5, 4).iter().fold(0.0, |a, x| a + x.berry_energy);
                            s += sum * p.burst_bonus;
                        }
                        Iyashinohadou | HelpSupportS | HelpBoost => {
                            let mut l = others(&pickup_top, 4);
                            l.push(p.clone());
                            let sum = l.iter().fold(0.0, |a, x| a + x.pickup_energy_per_help);
                            let help_num = match skill.kind {
                                HelpBoost => ev(|e| e.max),
                                HelpSupportS => ev(|e| e.value) / 5.0,
                                _ => ev(|e| e.help2) * 2.0 / 5.0,
                            };
                            s += sum * help_num;
                        }
                        _ => {}
                    }
                    if s != 0.0 && !s.is_nan() {
                        support_energy += s * p.skill_per_day * sw.weight;
                    }
                }
            }

            let p = &mut r.pokemon;
            p.support_energy_per_day = support_energy;
            p.support_shard_per_day = support_shard;
            let mut score = (p.energy_per_day + p.support_energy_per_day) * (100.0 + field_bonus) / 100.0;
            score += (score * if config.simulation.research_rank_max { 0.5 } else { 0.0 } + (p.shard + p.support_shard_per_day) * shard_to_energy)
                * config.simulation.shard_weight
                / 100.0;
            p.score = score;
            out.push(r);
        }
        out
    }
}

fn sub_skill_pos(id: u32) -> usize {
    master().sub_skills.iter().position(|s| s.id == id).expect("サブスキル")
}
