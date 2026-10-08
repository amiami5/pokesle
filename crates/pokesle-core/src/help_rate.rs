//! げんきとおてつだい倍率(本家 `models/help-rate.ts`)

use std::collections::HashMap;

use crate::config::Config;
use crate::data::{SkillKind, SubSkillKind};
use crate::js;
use crate::sim_pokemon::{HealEvent, SimPokemon};
use crate::simulator::Mode;

const GENKI_LIST: [(f64, f64); 4] = [(80.0, 0.45), (60.0, 0.52), (40.0, 0.58), (0.0, 0.66)];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HelpRateResult {
    pub day: f64,
    pub night: f64,
}

#[derive(Default)]
struct HealCacheNode {
    self_list: Vec<Vec<HealEvent>>,
    other_list: Vec<Vec<HealEvent>>,
    sub: HashMap<String, HealCacheNode>,
}

#[derive(Default)]
struct TeamHealCache {
    key: String,
    cache: HashMap<String, HealCacheNode>,
}

struct Info {
    stock_limit: u8,
    has_heal: bool,
    morning_genki: f64,
    before_night_help: f64,
    morning_limit: f64,
}

pub struct HelpRate {
    mode: Option<Mode>,
    day_length: f64,
    night_length: f64,
    check_freq: f64,
    genki_full: bool,
    genki_full_if_self_heal: bool,
    team_heal_cache: TeamHealCache,
    help_rate_cache: HashMap<Vec<u64>, HelpRateResult>,
}

const HELP_RATE_CACHE_LIMIT: usize = 4096;

impl HelpRate {
    /// `mode` が None の場合は本家の `new HelpRate(config)`(モード0)
    pub fn new(config: &Config, mode: Option<Mode>) -> HelpRate {
        let night_length = js::round(config.sleep_time * 3600.0);
        HelpRate {
            mode,
            day_length: 86400.0 - night_length,
            night_length,
            check_freq: config.check_freq,
            genki_full: config.simulation.genki_full,
            genki_full_if_self_heal: config.select_evaluate.genki_full_if_self_heal,
            team_heal_cache: TeamHealCache::default(),
            help_rate_cache: HashMap::new(),
        }
    }

    fn is_select(&self) -> bool {
        self.mode == Some(Mode::Select)
    }

    /// チーム全員のげんき回復量を計算する。`members` は `pool` 内のインデックス。
    pub fn calc_team_heal(&mut self, pool: &mut [SimPokemon], members: &[usize], add_heal: Option<&[HealEvent]>) {
        // げんき100%前提計算ならスキップ
        if !self.is_select() && self.genki_full {
            for &i in members {
                pool[i].day_help_rate = 2.22;
                pool[i].night_help_rate = 2.22;
            }
            return;
        }

        // 厳選シミュレーションでかつ自分で回復できる場合はスキップ
        if self.is_select() && pool[members[0]].skill().self_heal && self.genki_full_if_self_heal {
            for &i in members {
                pool[i].day_help_rate = 2.22;
                pool[i].night_help_rate = 2.22;
            }
            return;
        }

        let mut info_list: Vec<Info> = Vec::with_capacity(members.len());
        let mut healer_key = String::new();
        let mut healer_full_key = String::new();
        let mut sub_healer_key = String::new();
        let mut other_healer_list: Vec<usize> = Vec::new(); // members 内の位置
        let mut self_healer_list: Vec<usize> = Vec::new();

        for (k, &i) in members.iter().enumerate() {
            let p = &mut pool[i];
            p.self_heal_list.clear();
            p.other_heal_list.clear();
            let base = p.base();
            let info = Info {
                stock_limit: if base.specialty.is_skill_like() { 2 } else { 1 },
                has_heal: truthy(p.self_heal) || truthy(p.other_heal),
                morning_genki: 0.0,
                before_night_help: (self.night_length / p.speed).floor(),
                morning_limit: if p.has_sub_skill(SubSkillKind::GenkiBonus) { 105.0 } else { 100.0 },
            };

            if info.has_heal {
                let key = format!(
                    "{},{},{},{},{},{},{},{},",
                    base.name,
                    base.type_name,
                    js::to_fixed(p.nature_genki_multiplier, 2),
                    js::to_fixed(p.self_heal, 3),
                    js::to_fixed(p.other_heal, 3),
                    js::to_fixed(p.ceil_skill_rate, 3),
                    js::to_fixed(p.bag_full_help_num, 3),
                    info.morning_limit,
                );
                let sub_key = format!("{},{},", js::to_fixed(p.morning_heal_genki, 3), p.speed);
                if truthy(p.other_heal) {
                    healer_key.push_str(&key);
                    healer_full_key.push_str(&key);
                    healer_full_key.push_str(&sub_key);
                    other_healer_list.push(k);
                } else {
                    sub_healer_key.push_str(&key);
                    sub_healer_key.push_str(&sub_key);
                    self_healer_list.push(k);
                }
            }
            info_list.push(info);
        }

        if self.team_heal_cache.key != healer_key {
            self.team_heal_cache = TeamHealCache { key: healer_key, cache: HashMap::new() };
        }

        let day_length = self.day_length;
        let check_freq = self.check_freq;
        let mut caches = &mut self.team_heal_cache.cache;

        for lp in 0..2 {
            let (key, healer_list) = if lp == 0 {
                (healer_full_key.clone(), &other_healer_list)
            } else {
                (sub_healer_key.clone(), &self_healer_list)
            };

            if !caches.contains_key(&key) {
                for _ in 0..2 {
                    for &k in healer_list.iter() {
                        let pi = members[k];
                        pool[pi].self_heal_list.clear();
                        pool[pi].other_heal_list.clear();

                        let pokemon_is_dark = pool[pi].base().type_name == "あく";
                        let mut all_heal_list: Vec<HealEvent> = Vec::new();
                        for &si in members {
                            if pool[si].skill().kind == SkillKind::Nightmare && pokemon_is_dark {
                                continue;
                            }
                            all_heal_list.extend_from_slice(&pool[si].other_heal_list);
                        }
                        all_heal_list.sort_by(|a, b| js::cmp_num(a.time, b.time));

                        simulate_day(&mut pool[pi], &mut info_list[k], &all_heal_list, day_length, check_freq);
                    }
                }

                let node = HealCacheNode {
                    self_list: healer_list.iter().map(|&k| pool[members[k]].self_heal_list.clone()).collect(),
                    other_list: healer_list.iter().map(|&k| pool[members[k]].other_heal_list.clone()).collect(),
                    sub: HashMap::new(),
                };
                caches.insert(key.clone(), node);
            } else {
                let node = &caches[&key];
                for (n, &k) in healer_list.iter().enumerate() {
                    pool[members[k]].self_heal_list = node.self_list[n].clone();
                    pool[members[k]].other_heal_list = node.other_list[n].clone();
                }
            }
            caches = &mut caches.get_mut(&key).expect("キャッシュが存在しません").sub;
        }

        // 手伝い回数の増加率を計算
        for (j, &pi) in members.iter().enumerate() {
            let pokemon_is_dark = pool[pi].base().type_name == "あく";
            let mut all_heal_list: Vec<HealEvent> = Vec::new();
            for &si in members {
                if si == pi {
                    all_heal_list.extend_from_slice(&pool[si].self_heal_list);
                } else {
                    if pool[si].skill().kind == SkillKind::Nightmare && pokemon_is_dark {
                        continue;
                    }
                    all_heal_list.extend_from_slice(&pool[si].other_heal_list);
                }
            }
            let p = &pool[pi];
            if let Some(add) = add_heal
                && p.self_heal <= 0.0
                && p.other_heal <= 0.0
            {
                all_heal_list.extend_from_slice(add);
            }
            all_heal_list.sort_by(|a, b| js::cmp_num(a.time, b.time));

            let result = self.get_help_rate_with(
                &all_heal_list,
                0.0,
                p.morning_heal_genki,
                info_list[j].morning_limit,
                p.nature_genki_multiplier,
            );
            let p = &mut pool[pi];
            p.day_help_rate = result.day;
            p.night_help_rate = result.night;
            p.all_heal_list = all_heal_list;
        }
    }

    /// 本家 `getHelpRate(healList)`(他の引数は既定値)
    pub fn get_help_rate(&mut self, heal_list: &[HealEvent]) -> HelpRateResult {
        self.get_help_rate_with(heal_list, 0.0, 100.0, 100.0, 1.0)
    }

    pub fn get_help_rate_with(
        &mut self,
        heal_list: &[HealEvent],
        morning_genki: f64,
        morning_heal_genki: f64,
        morning_limit: f64,
        genki_multiplier: f64,
    ) -> HelpRateResult {
        if !self.is_select() && self.genki_full {
            return HelpRateResult { day: 2.22, night: 2.22 };
        }

        let mut cache_key = Vec::with_capacity(4 + heal_list.len() * 2);
        cache_key.extend([
            js::num_key(morning_genki),
            js::num_key(morning_heal_genki),
            js::num_key(morning_limit),
            js::num_key(genki_multiplier),
        ]);
        for h in heal_list {
            cache_key.push(js::num_key(h.effect));
            cache_key.push(js::num_key(h.time));
        }
        if let Some(&cached) = self.help_rate_cache.get(&cache_key) {
            return cached;
        }

        let mut list: Vec<HealEvent> = heal_list.to_vec();
        if list.last().is_none_or(|h| h.time != self.day_length) {
            list.push(HealEvent { effect: 0.0, time: self.day_length, night: false });
        }
        list.push(HealEvent { effect: 0.0, time: 86400.0, night: true });

        let mut genki = morning_genki;
        let mut day_rate = 0.0;
        let mut night_rate = 0.0;
        for _ in 0..3 {
            genki = js::max(genki, js::min(genki + morning_heal_genki * genki_multiplier, morning_limit));
            day_rate = 0.0;
            night_rate = 0.0;
            let mut t = 0.0;
            for h in &list {
                let mut past = h.time - t;
                let mut add = 0.0;
                for (border, effect) in GENKI_LIST {
                    if genki > border {
                        let this_time = js::min((genki - border) * 600.0, past);
                        past -= this_time;
                        genki -= this_time / 600.0;
                        add += this_time / effect;
                    }
                }
                if past > 0.0 {
                    add += past;
                }
                if h.night {
                    night_rate += add;
                } else {
                    day_rate += add;
                }
                t = h.time;
                if h.effect > 0.0 {
                    genki = js::min(js::max(0.0, genki + h.effect * genki_multiplier), 150.0);
                } else {
                    genki = js::min(js::max(0.0, genki + h.effect), 150.0);
                }
            }
        }

        let result = HelpRateResult { day: day_rate / self.day_length, night: night_rate / self.night_length };
        if self.help_rate_cache.len() >= HELP_RATE_CACHE_LIMIT {
            self.help_rate_cache.clear();
        }
        self.help_rate_cache.insert(cache_key, result);
        result
    }
}

fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

/// ヒーラー1匹の1日をシミュレーションし、回復イベントを記録する
fn simulate_day(pokemon: &mut SimPokemon, info: &mut Info, heal_list: &[HealEvent], day_length: f64, check_freq: f64) {
    let speed = pokemon.speed;
    let is_nightmare = pokemon.skill().kind == SkillKind::Nightmare;
    let ngm = pokemon.nature_genki_multiplier;
    let csr = pokemon.ceil_skill_rate;

    let mut heal_iter = 0usize;
    let mut un_pick_help_num = info.before_night_help;
    let mut before_help = 0.0;
    let mut check_num = 0.0;
    let mut next_check = 0.0;
    let mut next_heal = heal_list.first().map(|h| h.time).unwrap_or(86400.0);
    let mut time = 0.0;
    let mut genki = js::max(
        info.morning_genki,
        js::min(info.morning_genki + pokemon.morning_heal_genki * ngm, info.morning_limit),
    );

    loop {
        let next_time = js::min(js::min(next_check, next_heal), 86400.0);
        let mut before_genki = genki;
        genki = js::max(genki - (next_time - time) / 600.0, 0.0);

        for (border, effect) in GENKI_LIST {
            if before_genki <= border {
                continue;
            }
            let fixed_speed = speed * effect;
            if before_help + fixed_speed <= next_time {
                let remain_time = js::min(next_time, time + (before_genki - border) * 600.0 + fixed_speed) - before_help;
                let num = (remain_time / fixed_speed).floor();
                let past_time = num * fixed_speed;
                un_pick_help_num += num;
                before_help += past_time;
                before_genki = js::max(before_genki - past_time / 600.0, 0.0);
                time = before_help;
            } else {
                break;
            }
        }
        if before_help + speed <= next_time {
            let remain_time = next_time - before_help;
            let num = (remain_time / speed).floor();
            let past_time = num * speed;
            un_pick_help_num += num;
            before_help += past_time;
        }

        time = next_time;

        if time == 86400.0 {
            info.morning_genki = genki;
            break;
        }

        if time >= next_check && check_num < check_freq && info.has_heal {
            let skillable = js::min(un_pick_help_num, pokemon.bag_full_help_num + 4.0);
            let night_no_hit = (1.0 - csr).powf(skillable);
            let skill_num = if info.stock_limit == 2 {
                let one = if skillable >= 1.0 { (1.0 - csr).powf(skillable - 1.0) * csr * skillable } else { 0.0 };
                let two = if skillable >= 2.0 { 1.0 - night_no_hit - one } else { 0.0 };
                two * 2.0 + one
            } else {
                1.0 - night_no_hit
            };

            if !is_nightmare {
                genki = js::max(js::min(genki + skill_num * (pokemon.self_heal + pokemon.other_heal) * ngm, 150.0), 0.0);
            }
            if truthy(pokemon.other_heal) {
                pokemon.other_heal_list.push(HealEvent { effect: skill_num * pokemon.other_heal, time, night: false });
            }
            if !is_nightmare {
                pokemon.self_heal_list.push(HealEvent {
                    effect: skill_num * (pokemon.self_heal + pokemon.other_heal),
                    time,
                    night: false,
                });
            }

            un_pick_help_num = 0.0;
            check_num += 1.0;
            next_check = if check_num < check_freq { day_length * check_num / (check_freq - 1.0) } else { 86400.0 };
        }

        if time >= next_heal {
            let effect = heal_list[heal_iter].effect;
            heal_iter += 1;
            genki = js::max(js::min(genki + effect * ngm, 150.0), 0.0);
            next_heal = heal_list.get(heal_iter).map(|h| h.time).unwrap_or(86400.0);
        }
    }

    info.before_night_help = un_pick_help_num;
}
