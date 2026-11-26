//! ポケモンの主要計算(本家 `models/simulation/pokemon-simulator.ts`)

use std::collections::HashMap;

use crate::config::{Config, EventBonus};
use crate::cooking::{self, EvaluatedCooking};
use crate::data::{berry_energy_at, master, ExBuff, Field, Nature, NatureStat, Pokemon, Skill, SkillKind, Specialty, SubSkillKind};
use crate::evaluate_setting::{get_pokemon_evaluate_setting, resolve_skill_lv};
use crate::help_rate::{HelpRate, HelpRateResult};
use crate::js;
use crate::prob_border::ProbBorder;
use crate::sim_pokemon::{BoxPokemon, FoodProb, HealEvent, SimFood, SimPokemon, SkillWeight};

const GENKI_EFFECT: [f64; 6] = [0.0, 0.45, 0.52, 0.62, 0.71, 1.00];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 概算
    About = 1,
    /// チーム
    Team = 2,
    /// 厳選
    Select = 3,
}

#[derive(Debug, Clone, Copy)]
struct FoodEnergy {
    min: f64,
    max: f64,
}

/// 鍋に入る中で一番良い料理
#[derive(Debug, Clone, Copy)]
struct BestCooking {
    last_energy: f64,
}

/// チームモードの文脈。`pool[self_idx]` は計算中のポケモンで、取り出し済みのため参照しない。
pub struct TeamCtx<'a> {
    pub pool: &'a [SimPokemon],
    pub members: &'a [usize],
    pub self_idx: usize,
}

impl TeamCtx<'_> {
    /// チームメンバーを、計算中のポケモン(`me`)を含めて順に返す
    fn each<'s>(&'s self, me: &'s SimPokemon) -> impl Iterator<Item = (bool, &'s SimPokemon)> + 's {
        self.members.iter().map(move |&i| if i == self.self_idx { (true, me) } else { (false, &self.pool[i]) })
    }
}

/// `calcHelp` の追加オプション
#[derive(Default)]
pub struct HelpOption<'a> {
    pub team: Option<TeamCtx<'a>>,
    pub box_pokemon_list: &'a [BoxPokemon],
    pub help_boost_count: Option<f64>,
    pub score_for_healer_evaluate: Option<f64>,
    pub score_for_support_evaluate: Option<f64>,
    pub additional_skill_per_day: f64,
}

pub struct PokemonSimulator {
    pub config: Config,
    pub mode: Mode,
    best_cooking_cache: HashMap<i64, Option<BestCooking>>,
    pub cooking_list: Vec<EvaluatedCooking>,
    default_best_cooking: Option<BestCooking>,
    food_energy_map: Vec<FoodEnergy>,
    pub default_help_rate: HelpRateResult,

    expect_type: HashMap<String, f64>,
    prob_border: ProbBorder,
    berry_energy_weight: f64,
    food_energy_weight: f64,
    skill_energy_ignore: f64,
    help_rate: HelpRate,
    free_candy: f64,
    cooking_power_up_energy: f64,
    night_length: f64,
    day_length: f64,
    add_heal: Option<Vec<HealEvent>>,
    field: &'static Field,
    metronome_weight_list: Vec<SkillWeight>,
}

impl PokemonSimulator {
    pub fn new(config: Config, mode: Mode) -> PokemonSimulator {
        let m = master();
        let mut night_length = js::round(config.sleep_time * 3600.0);
        let mut day_length = 86400.0 - night_length;

        if let Some(time_length) = config.team_simulation.time_length
            && mode != Mode::Select
        {
            let length = 86400.0 * time_length;
            if day_length > length {
                day_length = length;
                night_length = 0.0;
            } else {
                night_length = length - day_length;
            }
        }

        let fixed_pot_size = if mode == Mode::Select {
            m.cooking_stats.pot_max
        } else {
            config.simulation.event_bonus.pot.apply(config.simulation.pot_size, 0.0, config.simulation.camp_ticket)
        };

        // 今週の料理タイプのリスト
        let cooking_list: Vec<EvaluatedCooking> = if mode == Mode::Select {
            cooking::evaluate_lv_list(None)
        } else {
            let cooking_type = config.simulation.cooking_type();
            cooking::evaluate_lv_list(Some(&config.simulation))
                .into_iter()
                .filter(|c| (cooking_type.is_none() || Some(c.cooking().cooking_type) == cooking_type) && (c.enable || c.cooking().food_num == 0.0))
                .collect()
        };

        let day = config.team_simulation.day;
        let mut food_common_rate = if mode == Mode::Select || day.is_none() {
            ((2.0 * 0.1 + 0.9) * 6.0 + (3.0 * 0.3 + 0.7)) / 7.0
        } else if day == Some(6.0) {
            1.3
        } else {
            1.1
        };
        let initial_change = config.team_simulation.initial_cooking_change.unwrap_or(0.0);
        if initial_change != 0.0 && !initial_change.is_nan() && mode != Mode::Select {
            food_common_rate = 0.0;
            let length = if day.is_some() { 3 } else { 21 };
            let mut no_success_rate = 1.0;
            for i in 0..length {
                let success = (if i >= 18 || day == Some(6.0) { 0.3 } else { 0.1 }) + initial_change * no_success_rate;
                no_success_rate *= 1.0 - success;
                food_common_rate += success;
            }
            food_common_rate /= length as f64;
        }
        if mode == Mode::About {
            food_common_rate *= config.simulation.cooking_weight;
        }

        // 有効な料理に対しての食材のエナジー評価
        let mut food_energy_map: Vec<FoodEnergy> =
            m.foods.iter().map(|f| FoodEnergy { min: f.energy * food_common_rate, max: f.energy }).collect();
        for c in &cooking_list {
            let cooking = c.cooking();
            for cf in &cooking.food_list {
                let e = &mut food_energy_map[cf.food];
                e.max = js::max(e.max, m.foods[cf.food].energy * cooking.rate * c.recipe_lv_bonus * c.event_food_rate * food_common_rate);
            }
        }

        let mut help_rate = HelpRate::new(&config, Some(mode));
        let default_help_rate = help_rate.get_help_rate(&[]);

        let expect_type = if mode == Mode::Select {
            config.select_evaluate.expect_type.clone()
        } else {
            config.simulation.expect_type.clone()
        };
        let prob_border = ProbBorder::new(expect_type.get("border").copied().unwrap_or(f64::NAN) / 100.0);
        let free_candy = config.candy.free_candy();

        let add_heal = if mode == Mode::Select {
            let n = config.check_freq.max(0.0) as usize;
            Some(
                (0..n)
                    .map(|i| HealEvent {
                        effect: config.select_evaluate.healer / config.check_freq,
                        time: js::round(day_length * i as f64 / (config.check_freq - 1.0)),
                        night: false,
                    })
                    .collect(),
            )
        } else {
            None
        };

        let se = &config.select_evaluate;
        let cooking_power_up_energy = (if se.cooking_power_up_type == 0.0 {
            m.cooking_stats.cooking_power_up_energy
        } else {
            m.cooking_stats.cooking_power_up_energy_average
        }) * se.cooking_power_up_rate
            / 100.0;

        let field = m.field(&config.simulation.field).unwrap_or(&m.fields[0]);
        let metronome_weight_list = m
            .metronome_target
            .iter()
            .map(|&skill| SkillWeight { skill, weight: 1.0 / m.metronome_target.len() as f64, skill_lv: None, copy: None })
            .collect();

        let mut sim = PokemonSimulator {
            berry_energy_weight: config.simulation.berry_energy_weight.unwrap_or(1.0),
            food_energy_weight: config.simulation.food_energy_weight.unwrap_or(1.0),
            skill_energy_ignore: config.simulation.skill_energy_ignore.unwrap_or(0.0),
            config,
            mode,
            best_cooking_cache: HashMap::new(),
            cooking_list,
            default_best_cooking: None,
            food_energy_map,
            default_help_rate,
            expect_type,
            prob_border,
            help_rate,
            free_candy,
            cooking_power_up_energy,
            night_length,
            day_length,
            add_heal,
            field,
            metronome_weight_list,
        };
        sim.default_best_cooking = sim.get_best_cooking(fixed_pot_size);
        sim
    }

    pub fn day_length(&self) -> f64 {
        self.day_length
    }

    pub fn night_length(&self) -> f64 {
        self.night_length
    }

    fn get_best_cooking(&mut self, pot_size: f64) -> Option<BestCooking> {
        get_best_cooking(&mut self.best_cooking_cache, &self.cooking_list, pot_size)
    }
}

fn get_best_cooking(cache: &mut HashMap<i64, Option<BestCooking>>, cooking_list: &[EvaluatedCooking], pot_size: f64) -> Option<BestCooking> {
    {
        let key = js::round(pot_size) as i64;
        if let Some(Some(cached)) = cache.get(&key) {
            return Some(*cached);
        }
        let average = master().food_stats.average_energy;
        let mut best: Option<BestCooking> = None;
        for c in cooking_list {
            let cooking = c.cooking();
            if cooking.food_num <= pot_size {
                let last_energy = c.fix_energy + (pot_size - cooking.food_num) * average;
                // 降順の安定ソートの先頭 = 最初に現れた最大値
                if best.is_none_or(|b| last_energy > b.last_energy) {
                    best = Some(BestCooking { last_energy });
                }
            }
        }
        cache.insert(key, best);
        best
    }
}

impl PokemonSimulator {

    fn matching_event_bonus(&self, base: &Pokemon) -> Vec<EventBonus> {
        self.config
            .simulation
            .event_bonus_list
            .iter()
            .filter(|eb| {
                let sp = |name: &str| eb.target.specialties.get(name).copied().unwrap_or(false);
                eb.target.types.get(&base.type_name).copied().unwrap_or(false)
                    || sp(base.specialty.name())
                    || (base.specialty == Specialty::All && (sp("きのみ") || sp("食材") || sp("スキル")))
            })
            .cloned()
            .collect()
    }

    /// ボックスのポケモンから計算する(本家 `fromBox`)
    pub fn from_box(&mut self, bx: &BoxPokemon, fixable: bool, use_candy: f64, require_lv: f64) -> Result<SimPokemon, String> {
        let m = master();
        let base = m.pokemon(&bx.name).ok_or_else(|| format!("知らないポケモン: {}", bx.name))?;
        let mut lv = bx.lv;
        let mut use_shard = 0.0;
        let mut use_candy = use_candy;
        let nature = m.nature(&bx.nature);
        let sim = &self.config.simulation;

        if fixable {
            let fix_lv = sim.fix_lv.unwrap_or(0.0);
            if sim.fix_resource_mode == 0.0 && (fix_lv != 0.0 || require_lv != 0.0) {
                lv = js::max(js::max(fix_lv, bx.lv), require_lv);
            }
            if sim.fix_resource_mode == 1.0 || sim.fix_resource_mode == 2.0 {
                let exp = &m.exp;
                let exp_len = exp.len() as f64;
                let lv_limit = if sim.fix_resource_mode == 1.0 { exp_len } else { js::max(fix_lv, require_lv) };
                let exp_total = |lv: f64| exp.get(lv as usize).map(|x| x.total).unwrap_or(f64::NAN);
                let mut total_exp = match bx.next_exp {
                    Some(next) if next != 0.0 => {
                        js::round(exp_total(js::min(bx.lv + 1.0, exp_len) - 1.0) * base.exp) - next
                    }
                    _ => js::round(exp_total(js::min(bx.lv, exp_len) - 1.0) * base.exp),
                };
                let candy_exp = Nature::pick_exp(nature);
                let candy_limit =
                    self.config.candy.bag.get(&base.candy_name).copied().unwrap_or(f64::NAN) + self.free_candy;
                let shard_limit = self.config.candy.shard.filter(|x| *x != 0.0);

                while lv < lv_limit {
                    let next_total = js::round(exp_total(lv) * base.exp);
                    let require_candy = ((next_total - total_exp) / candy_exp).ceil();
                    let require_shard = exp.get(lv as usize - 1).and_then(|x| x.shard).unwrap_or(0.0) * require_candy;
                    total_exp += candy_exp * require_candy;
                    if use_candy + require_candy <= candy_limit
                        && shard_limit.is_none_or(|limit| require_shard + use_shard <= limit)
                    {
                        use_candy += require_candy;
                        use_shard += require_shard;
                        lv += 1.0;
                    } else {
                        break;
                    }
                }
            }
        }

        // 有効なサブスキル
        let enable_len = enabled_sub_skill_len(lv);
        let mut sub_skill_names: Vec<String> = bx.sub_skill_list.clone();
        if fixable && sim.fix_sub_skill_seed {
            loop {
                let mut hit = false;
                for i in 0..enable_len.min(sub_skill_names.len()) {
                    if let Some(next) = m.sub_skill(&sub_skill_names[i]).and_then(|s| s.next.clone())
                        && !sub_skill_names.contains(&next)
                    {
                        hit = true;
                        sub_skill_names[i] = next;
                    }
                }
                if !hit {
                    break;
                }
            }
        }
        let enable_sub_skills: Vec<Option<usize>> = sub_skill_names
            .iter()
            .take(enable_len)
            .map(|n| m.sub_skill(n).map(|s| sub_skill_index(s.id)))
            .collect();

        // きのみ倍率
        let field = m.field(&sim.field).ok_or_else(|| format!("知らないフィールド: {}", sim.field))?;
        let berry_name = &m.pokemon_berry(base).name;
        let berry_match = if field.berry_option_list.is_some() {
            sim.berry_list.contains(berry_name)
        } else {
            field.berry_list.as_ref().is_some_and(|l| l.contains(berry_name))
        };
        let berry_rate = if berry_match { if field.ex && sim.field_ex == 1.0 { 240.0 } else { 200.0 } } else { 100.0 };

        let mut skill_lv = bx.skill_lv;
        if fixable {
            if sim.fix_skill_seed == 1.0 {
                let setting = get_pokemon_evaluate_setting(&self.config.select_evaluate, base).skill_lv;
                skill_lv = resolve_skill_lv(setting, base, skill_lv);
            }
            if sim.fix_skill_seed == 2.0 {
                skill_lv = Some(m.pokemon_skill(base).max_lv() as f64);
            }
        }

        let sleep_time = if fixable && sim.fix_sleep_ribbon {
            Some(bx.sleep_time.unwrap_or(0.0).max(2000.0))
        } else {
            bx.sleep_time
        };

        let event_bonus_list = self.matching_event_bonus(base);
        let mut pokemon = self.init_simulated_pokemon(
            base,
            lv,
            &bx.food_list,
            skill_lv,
            event_bonus_list,
            sleep_time,
            use_candy,
            use_shard,
            berry_match,
        )?;
        pokemon.box_pokemon = Some(bx.clone());
        pokemon.fixable = fixable;

        // 銀種が使用できるかどうかの一覧
        pokemon.next_sub_skill_list = bx
            .sub_skill_list
            .iter()
            .map(|name| m.sub_skill(name).and_then(|s| s.next.as_ref()).is_some_and(|next| !bx.sub_skill_list.contains(next)))
            .collect();

        self.calc_parameter(&mut pokemon, &enable_sub_skills, nature.map(|n| nature_index(&n.name)), berry_rate);
        Ok(pokemon)
    }

    /// 厳選評価用(本家 `fromEvaluate`)
    pub fn from_evaluate(&mut self, base: &Pokemon, lv: f64, food_name_list: &[String]) -> Result<SimPokemon, String> {
        let setting = get_pokemon_evaluate_setting(&self.config.select_evaluate, base).skill_lv;
        let skill_lv = resolve_skill_lv(setting, base, Some(1.0));
        let sleep_time = self.config.select_evaluate.pokemon_sleep_time;
        self.init_simulated_pokemon(base, lv, food_name_list, skill_lv, Vec::new(), Some(sleep_time), 0.0, 0.0, false)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn init_simulated_pokemon(
        &mut self,
        base: &Pokemon,
        lv: f64,
        food_name_list: &[String],
        skill_lv: Option<f64>,
        event_bonus_list: Vec<EventBonus>,
        sleep_time: Option<f64>,
        use_candy: f64,
        use_shard: f64,
        berry_match: bool,
    ) -> Result<SimPokemon, String> {
        let m = master();
        let first_food = food_name_list
            .first()
            .and_then(|n| m.food(n))
            .ok_or_else(|| format!("{}:不正な食べ物", base.name))?;
        let first_food_energy = first_food.energy * if base.specialty.is_food_like() { 2.0 } else { 1.0 };

        let mut pokemon = SimPokemon {
            base: base.index,
            lv,
            skill_lv,
            event_bonus_list,
            sleep_time,
            use_candy,
            use_shard,
            food_count: vec![0.0; m.foods.len()],
            ..Default::default()
        };

        let food_unlock = if lv >= 60.0 { 3 } else if lv >= 30.0 { 2 } else { 1 };
        let event_food: f64 = pokemon.event_bonus_list.iter().fold(0.0, |t, e| t + e.food);
        let food_use_rate = if self.mode == Mode::Select {
            get_pokemon_evaluate_setting(&self.config.select_evaluate, base).food_energy_rate / 100.0
        } else {
            1.0
        };

        for (index, name) in food_name_list.iter().enumerate().take(food_unlock) {
            if name.is_empty() {
                continue;
            }
            let food_idx = m.food_index(name).ok_or_else(|| format!("{}:不正な食べ物({name})", base.name))?;
            let food = &m.foods[food_idx];
            let mut num = js::round(first_food_energy * [1.0, 2.25, 3.6][index] / food.energy);
            let base_num = num;
            num += event_food;

            // EXモードの食材+1
            if self.mode != Mode::Select && self.field.ex && self.config.simulation.field_ex == 2.0 && berry_match {
                num += 1.0;
                if base.specialty == Specialty::Food {
                    num += 0.5;
                }
            }

            let fe = self.food_energy_map[food_idx];
            pokemon.food_list.push(SimFood {
                food: food_idx,
                num,
                base_num,
                energy: fe.max * food_use_rate + fe.min * (1.0 - food_use_rate),
            });
        }

        pokemon.food_prob_list = calc_food_prob_list(&pokemon.food_list);

        // 発動するスキルの一覧(ゆびをふる用)
        let skill = m.pokemon_skill(base);
        pokemon.skill_weight_list = if skill.kind == SkillKind::Metronome {
            self.metronome_weight_list.clone()
        } else {
            vec![SkillWeight { skill: skill.index, weight: 1.0, skill_lv: None, copy: None }]
        };
        Ok(pokemon)
    }

    fn ex_buff(&self, pokemon: &SimPokemon) -> Option<&'static ExBuff> {
        if self.mode == Mode::Select || !self.field.ex {
            return None;
        }
        let berry_name = &master().pokemon_berry(pokemon.base()).name;
        let berry_list = &self.config.simulation.berry_list;
        let buffs = self.field.ex_buff.as_ref()?;
        if berry_list.first() == Some(berry_name) {
            Some(&buffs.match_)
        } else if !berry_list.contains(berry_name) {
            Some(&buffs.not_match)
        } else {
            None
        }
    }

    /// おてスピや所持数、各種確率等の基本的な計算(本家 `calcParameter`)
    pub fn calc_parameter(&self, pokemon: &mut SimPokemon, sub_skill_list: &[Option<usize>], nature: Option<usize>, berry_rate: f64) {
        let m = master();
        let base = pokemon.base();
        let skill = m.pokemon_skill(base);
        pokemon.cooking_power_up_effect = 0.0;
        pokemon.cooking_chance_effect = 0.0;
        pokemon.shard = 0.0;
        pokemon.sub_skill_list.clear();
        pokemon.sub_skill_list.extend_from_slice(sub_skill_list);
        pokemon.sub_skill_mask = sub_skill_list.iter().flatten().fold(0, |mask, &i| mask | m.sub_skills[i].kind.bit());
        pokemon.nature = nature;
        let nature = pokemon.nature();
        let has = |k: SubSkillKind| pokemon.sub_skill_mask & k.bit() != 0;

        let ex_buff = self.ex_buff(pokemon);
        let berry = m.pokemon_berry(base);

        // きのみエナジー/手伝い
        let berry_energy_rate = if matches!(self.mode, Mode::About | Mode::Team) {
            self.config.simulation.berry_energy_rate.get(&berry.name).copied().unwrap_or(1.0)
        } else {
            1.0
        };
        pokemon.berry_energy =
            berry_energy_at(berry.energy, pokemon.lv) * berry_rate / 100.0 * berry_energy_rate * self.berry_energy_weight;
        pokemon.berry_rate = berry_rate;

        // きのみの個数
        pokemon.berry_num = (if base.specialty.is_berry_like() { 2.0 } else { 1.0 })
            + if has(SubSkillKind::BerryNumS) { 1.0 } else { 0.0 };

        // おてつだいサポート用のきのみエナジー/手伝い(イベントボーナス加算前)
        let berry_energy_per_help_for_support = pokemon.berry_energy * pokemon.berry_num;
        pokemon.berry_num += pokemon.event_bonus_list.iter().fold(0.0, |t, e| t + e.berry);
        pokemon.b_ep_h4_spt = berry_energy_per_help_for_support;
        pokemon.b_ep_h = pokemon.berry_energy * pokemon.berry_num;

        // 食材確率
        pokemon.food_rate = base.food_rate
            * (1.0
                + if has(SubSkillKind::FoodRateS) { 0.18 } else { 0.0 }
                + if has(SubSkillKind::FoodRateM) { 0.36 } else { 0.0 })
            * Nature::pick(nature, NatureStat::FoodRate, 1.2, 0.8, 1.0);

        let food_len = pokemon.food_list.len() as f64;
        pokemon.food_num = pokemon.food_list.iter().fold(0.0, |a, x| a + x.num) / food_len;
        let event_food_name = if self.mode != Mode::Select && base.specialty.is_food_like() {
            self.config.simulation.event_bonus.food.as_deref().filter(|s| !s.is_empty())
        } else {
            None
        };
        let event_food_energy = event_food_name.and_then(|n| m.food(n)).map_or(0.0, |f| f.energy);
        if event_food_name.is_some() {
            pokemon.food_num += 1.0;
        }
        pokemon.f_ep_h = (pokemon.food_list.iter().fold(0.0, |a, x| a + x.energy * x.num) / food_len + event_food_energy) * self.food_energy_weight;
        let food_energy_per_help_for_support =
            pokemon.food_list.iter().fold(0.0, |a, x| a + x.energy * x.base_num) / food_len * self.food_energy_weight;

        pokemon.pickup_energy_per_help = berry_energy_per_help_for_support * (1.0 - pokemon.food_rate)
            + food_energy_per_help_for_support * pokemon.food_rate;

        // 最大所持数
        let mut bag = base.bag
            + if has(SubSkillKind::BagS) { 6.0 } else { 0.0 }
            + if has(SubSkillKind::BagM) { 12.0 } else { 0.0 }
            + if has(SubSkillKind::BagL) { 18.0 } else { 0.0 };
        let sleep = pokemon.sleep_time.unwrap_or(f64::NAN);
        if sleep >= 200.0 {
            bag += 1.0;
        }
        if sleep >= 500.0 {
            bag += 2.0;
        }
        if sleep >= 1000.0 {
            bag += 3.0;
        }
        if sleep >= 2000.0 {
            bag += 2.0;
        }
        if self.mode != Mode::Select {
            bag += pokemon.event_bonus_list.iter().fold(0.0, |t, e| t + e.bag);
            bag *= pokemon.event_bonus_list.iter().fold(1.0, |t, e| t * e.bag_rate);
        }
        if self.mode != Mode::Select && self.config.simulation.camp_ticket {
            bag = (bag * 1.2).floor();
        }
        if let Some(b) = ex_buff.and_then(|x| x.bag)
            && b != 0.0
        {
            bag += b;
        }
        pokemon.bag = bag.ceil();

        // いつ育到達は所持数がいっぱい＋4回(キュー消化分)以降
        pokemon.bag_full_help_num = js::max(
            pokemon.bag / (pokemon.berry_num * (1.0 - pokemon.food_rate) + pokemon.food_num * pokemon.food_rate) + 4.0,
            0.0,
        );

        // スキルレベル
        let skill_lv_bonus = if has(SubSkillKind::SkillLvS) { 1.0 } else { 0.0 } + if has(SubSkillKind::SkillLvM) { 2.0 } else { 0.0 };
        if self.mode == Mode::Select {
            pokemon.fixed_skill_lv = pokemon.skill_lv.unwrap_or(f64::NAN) + skill_lv_bonus;
        } else {
            pokemon.fixed_skill_lv = pokemon.skill_lv.unwrap_or(base.evolve_lv as f64 + skill_lv_bonus);
            if let Some(s) = ex_buff.and_then(|x| x.skill_lv)
                && s != 0.0
            {
                pokemon.fixed_skill_lv += s;
            }
        }
        if self.mode != Mode::Select {
            pokemon.fixed_skill_lv += pokemon.event_bonus_list.iter().fold(0.0, |t, e| t + e.skill_lv);
        }
        if pokemon.fixed_skill_lv < 1.0 {
            pokemon.fixed_skill_lv = 1.0;
        }
        let max_lv = skill.max_lv() as f64;
        if pokemon.fixed_skill_lv > max_lv {
            pokemon.fixed_skill_lv = max_lv;
        }

        // スキル確率
        pokemon.skill_rate = base.skill_rate
            * (1.0
                + if has(SubSkillKind::SkillRateS) { 0.18 } else { 0.0 }
                + if has(SubSkillKind::SkillRateM) { 0.36 } else { 0.0 })
            * Nature::pick(nature, NatureStat::SkillRate, 1.2, 0.8, 1.0);
        if self.mode != Mode::Select {
            pokemon.skill_rate *= pokemon.event_bonus_list.iter().fold(1.0, |t, e| t * e.skill_rate);
        }
        if self.mode != Mode::Select && self.field.ex && self.config.simulation.field_ex == 3.0 && berry_rate_is_match(pokemon.berry_rate) {
            pokemon.skill_rate *= 1.25;
        }
        if self.skill_energy_ignore != 0.0 && !self.skill_energy_ignore.is_nan() && skill.energy_only {
            pokemon.skill_rate = 0.0;
        }

        // 天井を考慮したスキル確率
        pokemon.skill_ceil = if base.specialty.is_skill_like() { 40.0 * 3600.0 / base.help } else { 78.0 };
        pokemon.ceil_skill_rate = if pokemon.skill_rate > 0.0 {
            pokemon.skill_rate / (1.0 - (1.0 - pokemon.skill_rate).powf(pokemon.skill_ceil))
        } else {
            0.0
        };

        // 性格のげんき回復係数
        pokemon.nature_genki_multiplier = Nature::pick(nature, NatureStat::Genki, 1.2, 0.88, 1.0);
    }

    /// チームによって決まる値の計算(本家 `calcStatus`)。
    /// `team` を渡す場合、`pool[team.self_idx]` は `pokemon` として取り出しておくこと。
    pub fn calc_status(&self, pokemon: &mut SimPokemon, help_bonus: f64, genki_heal_bonus: f64, team: Option<&TeamCtx>) {
        let m = master();
        let base = pokemon.base();
        let skill = m.pokemon_skill(base);
        let has = |k: SubSkillKind| pokemon.sub_skill_mask & k.bit() != 0;

        let mut speed_bonus = 0.0;
        if has(SubSkillKind::SpeedS) {
            speed_bonus += 0.07;
        }
        if has(SubSkillKind::SpeedM) {
            speed_bonus += 0.14;
        }
        speed_bonus += help_bonus * 0.05;
        pokemon.speed_bonus = js::min(speed_bonus, 0.35);

        let nature = pokemon.nature();
        let camp = if self.mode != Mode::Select && self.config.simulation.camp_ticket { 1.2 } else { 1.0 };
        pokemon.speed = (base.help
            * (1.0 - (pokemon.lv - 1.0) * 0.002)
            * (1.0 - pokemon.speed_bonus)
            * Nature::pick(nature, NatureStat::Speed, 0.9, 1.075, 1.0)
            / camp)
            .floor();
        let sleep = pokemon.sleep_time.unwrap_or(f64::NAN);
        if base.remain_evolve_lv == 1 {
            if sleep >= 2000.0 {
                pokemon.speed *= 0.88;
            } else if sleep >= 500.0 {
                pokemon.speed *= 0.95;
            }
        } else if base.remain_evolve_lv == 2 {
            if sleep >= 2000.0 {
                pokemon.speed *= 0.75;
            } else if sleep >= 500.0 {
                pokemon.speed *= 0.89;
            }
        }

        // EXモードによるおてつだいスピードのバフ・デバフ
        if let Some(buff) = self.ex_buff(pokemon).and_then(|b| b.speed)
            && buff != 0.0
        {
            pokemon.speed *= buff;
        }

        // 日中の基本手伝い回数(げんき回復なし)
        let mut base_day_help_num = 0.0;
        let mut day_remain = self.day_length;
        for i in 0..4 {
            if day_remain > 0.0 {
                let time = 12000.0 - pokemon.speed * GENKI_EFFECT[i] / 2.0 + pokemon.speed * GENKI_EFFECT[i + 1] / 2.0;
                base_day_help_num += js::min(day_remain, time) / GENKI_EFFECT[i + 1] / pokemon.speed;
                day_remain -= time;
            } else {
                break;
            }
        }
        if day_remain > 0.0 {
            base_day_help_num += day_remain / pokemon.speed;
        }
        pokemon.base_day_help_num = base_day_help_num;

        // スキルコピーはチームのスキル一覧に変換
        if let Some(team) = team
            && skill.kind.is_skill_copy()
        {
            let mut list = Vec::new();
            let energy_charge_s = m.skill("エナジーチャージS").expect("エナジーチャージS").index;
            for &i in team.members {
                if i == team.self_idx {
                    continue;
                }
                let sub = &team.pool[i];
                if !sub.skill().is_copyable() {
                    list.push(SkillWeight { skill: energy_charge_s, weight: 1.0 / 4.0, skill_lv: None, copy: Some(i) });
                } else {
                    for sw in &sub.skill_weight_list {
                        list.push(SkillWeight { skill: sw.skill, weight: sw.weight / 4.0, skill_lv: None, copy: Some(i) });
                    }
                }
            }
            pokemon.skill_weight_list = list;
        }

        // 起床時の回復量
        pokemon.morning_heal_genki = js::min(
            js::min(self.config.sleep_time / 8.5, 1.0) * 100.0 * pokemon.nature_genki_multiplier * (1.0 + genki_heal_bonus * 0.14),
            100.0,
        );

        // げんき回復系スキル
        if pokemon.bag > 0.0 && pokemon.skill_weight_list.iter().any(|x| x.skill().genki) {
            let plus_minus_count = team.map(|t| {
                t.each(pokemon)
                    .filter(|(_, p)| matches!(p.skill().kind, SkillKind::Plus | SkillKind::Minus))
                    .count()
            });
            let mut self_sum = 0.0;
            let mut other_sum = 0.0;
            for sw in &pokemon.skill_weight_list {
                let s = sw.skill();
                if !s.genki {
                    continue;
                }
                // マイナスは条件を満たさないと追加効果を発動しない
                if s.kind == SkillKind::Minus && self.mode == Mode::Team && plus_minus_count.unwrap_or(0) < 2 {
                    continue;
                }
                let effect = effect_at(s, pokemon.fixed_skill_lv);
                self_sum += effect.and_then(|e| e.self_).unwrap_or(0.0) * sw.weight;
                other_sum += effect.and_then(|e| e.other).unwrap_or(0.0) * sw.weight;
            }
            pokemon.self_heal = self_sum;
            pokemon.other_heal = other_sum;
        }
    }

    /// ほっぺすりすりの追加分を含まない通常のスキル発動回数(本家 `calcSkillPerDay`)
    pub fn calc_skill_per_day(&mut self, pokemon: &SimPokemon) -> f64 {
        let day_help_num = self.day_length / pokemon.speed * pokemon.day_help_rate;
        let night_help_num = self.night_length / pokemon.speed * pokemon.night_help_rate;

        if pokemon.bag <= 0.0 {
            return 0.0;
        }

        let cf = self.config.check_freq;
        let day_skillable = js::min(day_help_num / (cf - 1.0), pokemon.bag_full_help_num);
        let night_skillable = js::min(night_help_num, pokemon.bag_full_help_num);
        let csr = pokemon.ceil_skill_rate;
        let skill_name = &pokemon.skill().name;
        let normal_expect = self.expect_type.get(skill_name) == Some(&0.0);

        if pokemon.base().specialty.is_skill_like() {
            let night_no = (1.0 - csr).powf(night_skillable);
            let night_one = if night_skillable >= 1.0 { (1.0 - csr).powf(night_skillable - 1.0) * csr * night_skillable } else { 0.0 };
            let night_two = if night_skillable >= 2.0 { 1.0 - night_no - night_one } else { 0.0 };

            if normal_expect {
                let day_no = (1.0 - csr).powf(day_skillable);
                let day_one = if day_skillable >= 1.0 { (1.0 - csr).powf(day_skillable - 1.0) * csr * day_skillable } else { 0.0 };
                let day_two = if day_skillable >= 2.0 { 1.0 - day_no - day_one } else { 0.0 };
                return (day_one + day_two * 2.0) * (cf - 1.0) + night_one + night_two * 2.0;
            }
            return self.prob_border.skill(csr, day_skillable, cf - 1.0, 2.0) + night_one + night_two * 2.0;
        }

        if normal_expect {
            return (1.0 - (1.0 - csr).powf(day_skillable)) * (cf - 1.0) + (1.0 - (1.0 - csr).powf(night_skillable));
        }
        self.prob_border.skill(csr, day_skillable, cf - 1.0, 1.0) + (1.0 - (1.0 - csr).powf(night_skillable))
    }

    /// 本家 `calcHelp`
    pub fn calc_help(&mut self, pokemon: &mut SimPokemon, opt: &HelpOption) -> Result<(), String> {
        let m = master();
        let cf = self.config.check_freq;

        pokemon.day_help_num = self.day_length / pokemon.speed * pokemon.day_help_rate;
        pokemon.night_help_num = self.night_length / pokemon.speed * pokemon.night_help_rate;
        pokemon.average_help_rate = (pokemon.day_help_num + pokemon.night_help_num) / (24.0 * 3600.0 / pokemon.speed);

        pokemon.normal_day_help_num = js::min(pokemon.day_help_num / (cf - 1.0), pokemon.bag_full_help_num) * (cf - 1.0);
        pokemon.normal_night_help_num = js::min(pokemon.night_help_num, pokemon.bag_full_help_num);
        pokemon.normal_help_num = pokemon.normal_day_help_num + pokemon.normal_night_help_num;
        pokemon.berry_help_num = js::max(pokemon.day_help_num + pokemon.night_help_num - pokemon.normal_help_num, 0.0);

        // きのみエナジー/日
        let berry_help_num = pokemon.normal_help_num * (1.0 - pokemon.food_rate) + pokemon.berry_help_num;
        pokemon.b_ep_d = pokemon.b_ep_h * berry_help_num;
        pokemon.berry_num_per_day = pokemon.berry_num * berry_help_num;

        pokemon.food_count.iter_mut().for_each(|x| *x = 0.0);
        let event_food_idx = if self.mode != Mode::Select && pokemon.base().specialty.is_food_like() {
            self.config.simulation.event_bonus.food.as_deref().and_then(|n| m.food_index(n))
        } else {
            None
        };
        if self.expect_type.get("food") == Some(&0.0) {
            let food_get_chance = pokemon.normal_help_num * pokemon.food_rate;
            pokemon.f_ep_d = pokemon.f_ep_h * food_get_chance;
            pokemon.food_num_per_day = pokemon.food_num * food_get_chance;
            let len = pokemon.food_list.len() as f64;
            for f in &pokemon.food_list {
                pokemon.food_count[f.food] += f.num / len * food_get_chance;
            }
            if let Some(ef_idx) = event_food_idx {
                pokemon.food_count[ef_idx] += food_get_chance;
            }
        } else {
            pokemon.f_ep_d = 0.0;
            pokemon.food_num_per_day = 0.0;
            let food_probs = std::mem::take(&mut pokemon.food_prob_list);
            for fp in &food_probs {
                let num = self.prob_border.get(pokemon.food_rate * fp.weight, pokemon.normal_help_num) * fp.num;
                pokemon.f_ep_d += num * fp.energy;
                pokemon.food_num_per_day += num;
                pokemon.food_count[fp.food] += num;
            }
            if let Some(ef_idx) = event_food_idx {
                let food_get_num: f64 = food_probs
                    .iter()
                    .map(|fp| {
                        if fp.num != 0.0 && !fp.num.is_nan() {
                            self.prob_border.get(pokemon.food_rate * fp.weight, pokemon.normal_help_num)
                        } else {
                            0.0
                        }
                    })
                    .sum();
                pokemon.f_ep_d += food_get_num * m.foods[ef_idx].energy * self.food_energy_weight;
                pokemon.food_num_per_day += food_get_num;
                pokemon.food_count[ef_idx] += food_get_num;
            }
            pokemon.food_prob_list = food_probs;
        }

        pokemon.pickup_energy_per_day = pokemon.b_ep_d + pokemon.f_ep_d;

        pokemon.skill_per_day = self.calc_skill_per_day(pokemon) + opt.additional_skill_per_day;

        // スキルエナジー/日
        pokemon.skill_energy = 0.0;
        pokemon.shard = 0.0;
        pokemon.skill_energy_map.clear();
        let mut total_cooking_power_up_effect = 0.0;
        let event_berry_burst = self.config.simulation.event_bonus.skill.berry_burst;
        let event_food_get = self.config.simulation.event_bonus.skill.food_get;
        let sunday_prepare = self.config.simulation.sunday_prepare;
        let cooking_weight = self.config.simulation.cooking_weight;
        let own_skill_kind = pokemon.skill().kind;
        let yache = m.berry("ヤチェ").map(|b| b.energy).unwrap_or(f64::NAN);
        let mago = m.berry("マゴ").map(|b| b.energy).unwrap_or(f64::NAN);
        static ALL_FOODS: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();
        let all_foods: &[usize] = ALL_FOODS.get_or_init(|| (0..m.foods.len()).collect()).as_slice();

        let sw_list = std::mem::take(&mut pokemon.skill_weight_list);
        let skill_loop_res: Result<(), String> = (|| {
            for &sw in &sw_list {
                let skill = sw.skill();
                let mut weight = sw.weight;
                let copy = sw.copy.map(|i| &opt.team.as_ref().expect("コピー元にはチームが必要です").pool[i]);
                let skill_lv_index: f64 = match sw.skill_lv {
                    None => (if (skill.max_lv() as f64) >= pokemon.fixed_skill_lv { pokemon.fixed_skill_lv } else { skill.max_lv() as f64 }) - 1.0,
                    Some(lv) => lv - 1.0,
                };
                let effect = effect_index(skill, skill_lv_index);
                let ev = |f: fn(&crate::data::SkillEffect) -> Option<f64>| effect.and_then(f).unwrap_or(f64::NAN);
                let mut energy_per_skill = 0.0;
                let mut food_get: Option<f64> = None;
                let mut food_get_list: Option<std::borrow::Cow<'_, [usize]>> = None;
                let mut cooking_power_up_effect = 0.0;
                let mut cooking_chance: Option<f64> = None;

            if self.mode != Mode::Select {
                weight *= self.config.simulation.skill_rate.get(&skill.name).copied().unwrap_or(f64::NAN);
            }

            match skill.kind {
                SkillKind::Hoppesurisuri => {
                    // 追加発動はチーム計算側(TeamSimulator)で反復計算するので、チーム計算時はここでは評価しない
                    if matches!(self.mode, Mode::Select | Mode::About) {
                        energy_per_skill = self.select_skill_energy(skill, skill_lv_index);
                    }
                }
                SkillKind::EnergyChargeS | SkillKind::EnergyChargeSRandom | SkillKind::EnergyChargeM => {
                    energy_per_skill = ev(|e| e.value);
                }
                SkillKind::Takuwaeru => {
                    // スキルコピーやゆびをふるの場合はたくわえない
                    energy_per_skill = if copy.is_some() || own_skill_kind == SkillKind::Metronome { ev(|e| e.base) } else { ev(|e| e.expect) };
                }
                SkillKind::Nightmare => energy_per_skill = ev(|e| e.energy),
                SkillKind::Bakenokawa | SkillKind::BerryBurst => {
                    let (s, o) = (ev(|e| e.self_), ev(|e| e.other));
                    match self.mode {
                        Mode::About => {
                            energy_per_skill = pokemon.berry_energy * s * event_berry_burst;
                            pokemon.burst_bonus = o * weight * event_berry_burst;
                        }
                        Mode::Team => {
                            let team = opt.team.as_ref().expect("チームが必要です");
                            energy_per_skill = 0.0;
                            for (is_self, _) in team.each(pokemon) {
                                energy_per_skill += pokemon.berry_energy * if is_self { s } else { o } * event_berry_burst;
                            }
                        }
                        Mode::Select => {
                            energy_per_skill = pokemon.berry_energy * s + berry_energy_at(yache, pokemon.lv) * o * 4.0;
                        }
                    }
                    if skill.kind == SkillKind::Bakenokawa && copy.is_none() && own_skill_kind != SkillKind::Metronome {
                        let spd = pokemon.skill_per_day;
                        let success = 1.0 - (1.0 - skill.success.unwrap_or(f64::NAN)).powf(spd);
                        energy_per_skill *= (success * (spd + 2.0) + (1.0 - success) * spd) / spd;
                    }
                }
                SkillKind::Ryuseigun => {
                    let bonus = ev(|e| e.bonus);
                    match self.mode {
                        Mode::Select => {
                            let p = effect_pair(effect, opt.help_boost_count);
                            energy_per_skill = pokemon.berry_energy * (p.self_ + bonus) + berry_energy_at(yache, pokemon.lv) * p.other * 4.0;
                        }
                        Mode::About => {
                            let p = effect.map(|e| e.team_last_pair()).unwrap_or(nan_pair());
                            energy_per_skill = pokemon.berry_energy * (p.self_ + bonus) * event_berry_burst;
                            pokemon.burst_bonus = p.other * event_berry_burst;
                        }
                        Mode::Team => {
                            let p = effect_pair(effect, opt.help_boost_count);
                            let team = opt.team.as_ref().expect("チームが必要です");
                            let with_latias = team.each(pokemon).any(|(_, x)| x.base().name == "ラティアス");
                            energy_per_skill = 0.0;
                            for (is_self, _) in team.each(pokemon) {
                                energy_per_skill += pokemon.berry_energy
                                    * if is_self { if with_latias { p.self_ } else { p.self_ + bonus } } else { p.other };
                            }
                            energy_per_skill *= event_berry_burst;
                        }
                    }
                }
                SkillKind::Mikazukinoinori => match self.mode {
                    Mode::Select => {
                        let p = effect_pair(effect, opt.help_boost_count);
                        energy_per_skill = pokemon.berry_energy * p.self_ + berry_energy_at(mago, pokemon.lv) * p.other * 4.0;
                    }
                    Mode::About => {
                        let p = effect.map(|e| e.team_last_pair()).unwrap_or(nan_pair());
                        energy_per_skill = pokemon.berry_energy * p.self_ * event_berry_burst;
                        pokemon.burst_bonus = p.other * event_berry_burst;
                    }
                    Mode::Team => {
                        let p = effect_pair(effect, opt.help_boost_count);
                        let team = opt.team.as_ref().expect("チームが必要です");
                        energy_per_skill = 0.0;
                        for (is_self, _) in team.each(pokemon) {
                            energy_per_skill += pokemon.berry_energy * if is_self { p.self_ } else { p.other };
                        }
                        energy_per_skill *= event_berry_burst;
                    }
                },
                SkillKind::Iyashinohadou => match self.mode {
                    Mode::Select => {
                        energy_per_skill = opt.score_for_support_evaluate.unwrap_or(f64::NAN) * ev(|e| e.help2) * 2.0;
                    }
                    Mode::About => {}
                    Mode::Team => {
                        let team = opt.team.as_ref().expect("チームが必要です");
                        let mut help_count = ev(|e| e.help1);
                        if team.each(pokemon).any(|(_, x)| x.base().name == "ラティオス") {
                            help_count = ev(|e| e.help2);
                        }
                        help_count = help_count * 2.0 / 5.0;
                        energy_per_skill = 0.0;
                        let mut adds: Vec<(usize, f64)> = Vec::new();
                        for (_, sub) in team.each(pokemon) {
                            energy_per_skill += sub.b_ep_h * help_count * (1.0 - sub.food_rate);
                            let len = sub.food_list.len() as f64;
                            for f in &sub.food_list {
                                adds.push((f.food, f.num / len * help_count * sub.food_rate * pokemon.skill_per_day * weight));
                            }
                        }
                        for (food, v) in adds {
                            pokemon.food_count[food] += v;
                        }
                    }
                },
                SkillKind::HelpSupportS | SkillKind::HelpBoost => match self.mode {
                    Mode::Select => {
                        let v = if skill.kind == SkillKind::HelpBoost { ev(|e| e.max) * 5.0 } else { ev(|e| e.value) };
                        energy_per_skill = opt.score_for_support_evaluate.unwrap_or(f64::NAN) * v;
                    }
                    Mode::About => {}
                    Mode::Team => {
                        let help_count = if skill.kind == SkillKind::HelpSupportS {
                            ev(|e| e.value) / 5.0
                        } else {
                            let idx = opt.help_boost_count.unwrap_or(f64::NAN) - 1.0;
                            ev(|e| e.fix) + effect.map(|e| team_num_at(e, idx)).unwrap_or(f64::NAN)
                        };
                        let team = opt.team.as_ref().expect("チームが必要です");
                        energy_per_skill = 0.0;
                        let mut adds: Vec<(usize, f64)> = Vec::new();
                        for (_, sub) in team.each(pokemon) {
                            energy_per_skill += sub.b_ep_h4_spt * help_count * (1.0 - sub.food_rate);
                            let len = sub.food_list.len() as f64;
                            for f in &sub.food_list {
                                adds.push((f.food, f.base_num / len * help_count * sub.food_rate * pokemon.skill_per_day * weight));
                            }
                        }
                        for (food, v) in adds {
                            pokemon.food_count[food] += v;
                        }
                    }
                },
                SkillKind::FoodGetS => {
                    food_get = Some(ev(|e| e.value));
                    food_get_list = Some(std::borrow::Cow::Borrowed(all_foods));
                }
                SkillKind::Present => {
                    food_get = Some(ev(|e| e.value));
                    food_get_list = Some(std::borrow::Cow::Borrowed(all_foods));
                    if self.mode == Mode::Select {
                        energy_per_skill += self.config.select_evaluate.energy_per_candy * 4.0;
                    }
                }
                SkillKind::BuildUp => {
                    food_get = Some(ev(|e| e.main));
                    food_get_list = Some(std::borrow::Cow::Borrowed(all_foods));
                    cooking_chance = Some(ev(|e| e.sub));
                }
                SkillKind::Plus => {
                    food_get = Some(ev(|e| e.main));
                    food_get_list = Some(std::borrow::Cow::Borrowed(all_foods));
                    let sub = ev(|e| e.sub);
                    let energy_food_idx = pokemon.food_list.first().map(|f| f.food).ok_or("食材がありません")?;
                    let energy_food = &m.foods[energy_food_idx];
                    let get_num = js::round(sub / energy_food.energy) + 6.0;
                    match self.mode {
                        Mode::Select => {
                            energy_per_skill = energy_food.energy * energy_food.best_rate * m.cooking_stats.max_recipe_bonus * get_num;
                        }
                        Mode::About => {
                            if self.food_energy_weight > 0.0 {
                                let num = get_num * pokemon.skill_per_day * weight;
                                pokemon.food_count[energy_food_idx] += num;
                                energy_per_skill = self.food_energy_map[energy_food_idx].max * get_num * self.food_energy_weight;
                            }
                        }
                        Mode::Team => {
                            let team = opt.team.as_ref().expect("チームが必要です");
                            let count = team.each(pokemon).filter(|(_, x)| matches!(x.skill().kind, SkillKind::Plus | SkillKind::Minus)).count();
                            if count >= 2 {
                                // スキルコピーの場合、獲得数はコピー先、獲得するのは発動者のもの
                                let get_food = match copy {
                                    Some(c) => c.food_list.first().map(|f| f.food).ok_or("食材がありません")?,
                                    None => energy_food_idx,
                                };
                                let num = get_num * pokemon.skill_per_day * weight;
                                pokemon.food_count[get_food] += num;
                            }
                        }
                    }
                }
                SkillKind::FoodSelectS | SkillKind::Kairikibasami | SkillKind::Kyouun => {
                    if skill.kind == SkillKind::Kyouun {
                        pokemon.shard += ev(|e| e.shard) * pokemon.skill_per_day * weight;
                    }
                    food_get = Some(ev(|e| e.food));
                    food_get_list = Some(match copy {
                        Some(c) => std::borrow::Cow::Owned(c.base().food_list.iter().map(|f| f.food).collect()),
                        None => match &skill.food_list {
                            Some(list) => std::borrow::Cow::Borrowed(list.as_slice()),
                            None => std::borrow::Cow::Owned(pokemon.base().food_list.iter().map(|f| f.food).collect()),
                        },
                    });
                }
                SkillKind::ShardGetS | SkillKind::ShardGetSRandom => {
                    pokemon.shard += ev(|e| e.value) * pokemon.skill_per_day * weight;
                }
                SkillKind::Hadoudan => {
                    pokemon.shard += ev(|e| e.shard) * pokemon.skill_per_day * weight;
                    energy_per_skill = ev(|e| e.energy);
                }
                SkillKind::CookingPowerUpS => cooking_power_up_effect = ev(|e| e.value),
                SkillKind::Minus => cooking_power_up_effect = ev(|e| e.main),
                SkillKind::CookingChanceS => cooking_chance = Some(ev(|e| e.value)),
                SkillKind::MagicalFlame => {
                    cooking_power_up_effect = ev(|e| e.main);
                    cooking_chance = Some(ev(|e| e.sub));
                }
                SkillKind::Henshin | SkillKind::Monomane => match self.mode {
                    Mode::Select | Mode::About => energy_per_skill = self.select_skill_energy(skill, skill_lv_index),
                    Mode::Team => {
                        // スキルコピーがスキルコピーをコピーした場合はエナジーチャージS相当(公式)
                        energy_per_skill = m
                            .skill("エナジーチャージS")
                            .and_then(|s| effect_index(s, skill_lv_index))
                            .and_then(|e| e.value)
                            .unwrap_or(f64::NAN);
                    }
                },
                SkillKind::PsychoBreak => {
                    let zone = ev(|e| e.zone);
                    let energy = ev(|e| e.energy);
                    let berry = m.pokemon_berry(pokemon.base());
                    if self.mode == Mode::Select {
                        // このタイプのきのみが最も稼げるポケモン上位5匹の1日あたりのきのみの数
                        let mut list: Vec<f64> = m
                            .pokemon
                            .iter()
                            .filter(|x| x.berry == pokemon.base().berry)
                            .map(|x| {
                                (if x.specialty.is_berry_like() { 3.0 } else { 2.0 }) * 86400.0
                                    / (x.help * 0.65 * (1.0 - (pokemon.lv - 1.0) * 0.002))
                                    / 0.45
                            })
                            .collect();
                        let top5 = sum_top5(&mut list);
                        energy_per_skill = berry.energy * 2.0 * 1.025_f64.powf(pokemon.lv - 1.0) * top5 * 21.0 / 7.0 * zone / 100.0 + energy;
                    } else {
                        let day_rate = match self.config.team_simulation.day {
                            Some(d) if d >= 0.0 => (6.0 - d) / 2.0,
                            _ => 1.5,
                        };
                        let mut list: Vec<f64> = opt
                            .box_pokemon_list
                            .iter()
                            .filter_map(|x| {
                                let p = m.pokemon(&x.name)?;
                                (p.berry == pokemon.base().berry).then(|| {
                                    (if p.specialty.is_berry_like() { 3.0 } else { 2.0 }) * 86400.0
                                        / (p.help * 0.65 * (1.0 - (x.lv - 1.0) * 0.002))
                                        / 0.45
                                })
                            })
                            .collect();
                        let top5 = sum_top5(&mut list);
                        energy_per_skill = berry.energy * pokemon.berry_rate / 100.0 * 1.025_f64.powf(pokemon.lv - 1.0) * top5 * day_rate * zone / 100.0 + energy;
                    }
                }
                SkillKind::GenkiAllS | SkillKind::BerryJuice | SkillKind::GenkiYellS | SkillKind::GenkiChargeS | SkillKind::Tsukinohikari => {
                    // 別の場所で計算
                }
                SkillKind::Metronome | SkillKind::Unknown => {
                    return Err(format!("未実装のスキル: {}", skill.name));
                }
            }

            // 食材ゲットの処理
            if let (Some(food_get), Some(list)) = (food_get, food_get_list.as_deref())
                && food_get != 0.0
                && !food_get.is_nan()
            {
                let len = list.len() as f64;
                let ty = if list.len() == m.foods.len() { self.config.team_simulation.food_get_evaluate_type } else { 1.0 };
                let ts = &self.config.team_simulation;
                match self.mode {
                    Mode::Select => {
                        let num = food_get / len * pokemon.skill_per_day * weight;
                        let food_get_rate = get_pokemon_evaluate_setting(&self.config.select_evaluate, pokemon.base()).food_get_rate;
                        let mut food_energy = 0.0;
                        for &f in list {
                            pokemon.food_count[f] += num;
                            let food = &m.foods[f];
                            food_energy += food.energy * ((food.best_rate * m.cooking_stats.max_recipe_bonus - 1.0) * food_get_rate / 100.0 + 1.0);
                        }
                        energy_per_skill += food_energy / len * food_get;
                    }
                    Mode::About => {
                        if self.food_energy_weight > 0.0 {
                            let one = food_get * event_food_get / len * ts.food_get_evaluate_rate;
                            let num = pokemon.skill_per_day * one * weight;
                            let mut food_energy = 0.0;
                            for &f in list {
                                if ty == 1.0 {
                                    pokemon.food_count[f] += num;
                                }
                                food_energy += self.food_energy_map[f].max;
                            }
                            let mut energy = food_energy * self.food_energy_weight * one;
                            if ty == 3.0
                                && let Some(day) = ts.day
                            {
                                energy *= (7.0 - (day + 1.0) % 7.0) / (7.0 - day);
                            }
                            energy_per_skill += energy;
                        }
                    }
                    Mode::Team => {
                        if !sunday_prepare {
                            let one = food_get * event_food_get / len * ts.food_get_evaluate_rate;
                            if ty == 1.0 {
                                let num = pokemon.skill_per_day * one * weight;
                                for &f in list {
                                    pokemon.food_count[f] += num;
                                }
                            } else {
                                let mut energy = 0.0;
                                for &f in list {
                                    energy += self.food_energy_map[f].max;
                                }
                                energy *= self.food_energy_weight * one;
                                if ty == 3.0
                                    && let Some(day) = ts.day
                                {
                                    energy *= (7.0 - (day + 1.0) % 7.0) / (7.0 - day);
                                }
                                energy_per_skill += energy;
                            }
                        }
                    }
                }
            }

            // 料理パワーアップの処理
            if cooking_power_up_effect != 0.0 && !cooking_power_up_effect.is_nan() {
                let effect_num = pokemon.skill_per_day * weight;
                let mut this_energy = 0.0;
                match self.mode {
                    Mode::Select => {
                        let limit = m.cooking_stats.max_food_num - m.cooking_stats.pot_max;
                        for add in split_three(effect_num, cooking_power_up_effect) {
                            this_energy += self.cooking_power_up_energy * js::min(add, limit);
                            if add > limit {
                                this_energy += (add - limit) * m.food_stats.average_energy;
                            }
                        }
                    }
                    Mode::About => {
                        for add in split_three(effect_num, cooking_power_up_effect) {
                            let after_pot = self.config.simulation.event_bonus.pot.apply(
                                self.config.simulation.pot_size,
                                add,
                                self.config.simulation.camp_ticket,
                            );
                            if let Some(after) = get_best_cooking(&mut self.best_cooking_cache, &self.cooking_list, after_pot) {
                                let default = self.default_best_cooking.map(|b| b.last_energy).unwrap_or(f64::NAN);
                                this_energy += (after.last_energy - default) * self.config.simulation.cooking_weight;
                            }
                        }
                    }
                    Mode::Team => total_cooking_power_up_effect += cooking_power_up_effect,
                }
                energy_per_skill += this_energy / effect_num;
            }

            // 料理チャンスの処理
            if let Some(chance) = cooking_chance
                && chance != 0.0
                && !chance.is_nan()
            {
                match self.mode {
                    Mode::Select => {
                        let total = chance / 100.0 * pokemon.skill_per_day * weight * 7.0;
                        energy_per_skill += (cooking::get_chance_week_effect(total, None, 0.0).total - 24.6) / 7.0
                            * m.cooking_stats.max_energy
                            / pokemon.skill_per_day
                            / weight;
                    }
                    Mode::About => {
                        if !sunday_prepare && pokemon.skill_per_day != 0.0 && !pokemon.skill_per_day.is_nan() {
                            let total = chance / 100.0 * pokemon.skill_per_day * weight * 7.0;
                            let default = self.default_best_cooking.map(|b| b.last_energy).unwrap_or(f64::NAN);
                            energy_per_skill += (cooking::get_chance_week_effect(total, None, 0.0).total - 24.6) / 7.0 * default
                                / pokemon.skill_per_day
                                * cooking_weight
                                / weight;
                        }
                    }
                    Mode::Team => {
                        if !sunday_prepare {
                            pokemon.cooking_chance_effect = chance / 100.0 * pokemon.skill_per_day * weight;
                        }
                    }
                }
            }

            if energy_per_skill != 0.0 && !energy_per_skill.is_nan() {
                energy_per_skill *= weight;
                pokemon.set_skill_energy(skill.index, energy_per_skill);
                pokemon.skill_energy += energy_per_skill;
            }
            }
            Ok(())
        })();
        pokemon.skill_weight_list = sw_list;
        skill_loop_res?;
        pokemon.cooking_power_up_effect = total_cooking_power_up_effect;

        // コピーやゆびをふるを含め、全スキルの獲得分へイベント倍率を一度だけ適用する。
        if self.mode != Mode::Select {
            pokemon.shard *= self.config.simulation.event_bonus.skill.shard;
        }

        // チーム全体のげんき回復による増加エナジー
        if pokemon.other_heal > 0.0 && self.mode == Mode::Select {
            let hr = self.help_rate.get_help_rate(&pokemon.other_heal_list);
            let st = self.config.sleep_time;
            let d = self.default_help_rate;
            let energy = opt.score_for_healer_evaluate.unwrap_or(f64::NAN)
                * ((hr.day * (24.0 - st) + hr.night * st) / (d.day * (24.0 - st) + d.night * st) - 1.0)
                * 4.0
                / pokemon.skill_per_day;
            let skill_index = pokemon.skill().index;
            let current = pokemon.skill_energy_of(skill_index).unwrap_or(0.0);
            pokemon.set_skill_energy(skill_index, current + energy);
            pokemon.skill_energy += energy;
        }

        pokemon.skill_energy_per_day = pokemon.skill_energy * pokemon.skill_per_day;
        pokemon.energy_per_day = pokemon.pickup_energy_per_day + pokemon.skill_energy_per_day;
        Ok(())
    }

    /// スキルコピー・ほっぺすりすりの厳選/概算用エナジー
    fn select_skill_energy(&self, skill: &Skill, skill_lv_index: f64) -> f64 {
        let i = skill_lv_index;
        let configured = self
            .config
            .select_evaluate
            .skill_energy
            .get(&skill.name)
            .and_then(|list| index_f(list, i))
            .flatten();
        configured.unwrap_or_else(|| skill.evaluate_energy.as_ref().and_then(|l| index_f(l, i)).unwrap_or(f64::NAN))
    }

    pub fn calc_team_heal(&mut self, pool: &mut [SimPokemon], members: &[usize]) {
        self.help_rate.calc_team_heal(pool, members, self.add_heal.as_deref());
    }

    pub fn get_help_rate(&mut self, heal_list: &[HealEvent]) -> HelpRateResult {
        self.help_rate.get_help_rate(heal_list)
    }

    /// 並列タスク間で計算結果が直前の評価順に依存しないよう、丸めキーを使うキャッシュを初期化する
    pub fn clear_select_caches(&mut self) {
        self.prob_border.clear_cache();
        self.help_rate.clear_team_heal_cache();
    }

    /// 厳選用評価(本家 `selectEvaluate`)。`pokemon` を書き換えて結果を入れる。
    pub fn select_evaluate(
        &mut self,
        pokemon: &mut SimPokemon,
        sub_skill_list: &[Option<usize>],
        nature: Option<usize>,
        score_for_healer_evaluate: f64,
        score_for_support_evaluate: f64,
    ) -> Result<(), String> {
        let berry_rate = get_pokemon_evaluate_setting(&self.config.select_evaluate, pokemon.base()).berry_energy_rate;
        self.calc_parameter(pokemon, sub_skill_list, nature, berry_rate);

        let has_help_bonus = pokemon.has_sub_skill(SubSkillKind::HelpBonus);
        let team_help_bonus = if has_help_bonus {
            self.config.select_evaluate.team_help_bonus
        } else {
            self.config.select_evaluate.team_help_bonus_without_self
        };
        let help_bonus = team_help_bonus + if has_help_bonus { 1.0 } else { 0.0 };
        let genki_bonus = if pokemon.has_sub_skill(SubSkillKind::GenkiBonus) { 1.0 } else { 0.0 };
        self.calc_status(pokemon, help_bonus, genki_bonus, None);

        let should_calc_team_heal = if self.mode == Mode::Select {
            pokemon.self_heal > 0.0 || pokemon.other_heal > 0.0 || self.config.select_evaluate.healer > 0.0
        } else {
            pokemon.self_heal > 0.0 || pokemon.other_heal > 0.0
        };
        if should_calc_team_heal {
            self.calc_team_heal(std::slice::from_mut(pokemon), &[0]);
        } else {
            pokemon.day_help_rate = self.default_help_rate.day;
            pokemon.night_help_rate = self.default_help_rate.night;
        }

        self.calc_help(
            pokemon,
            &HelpOption {
                score_for_healer_evaluate: Some(score_for_healer_evaluate),
                score_for_support_evaluate: Some(score_for_support_evaluate),
                help_boost_count: Some(5.0),
                ..Default::default()
            },
        )?;

        // おてボの残り評価はシンプルに倍率にする
        if has_help_bonus {
            let thb = team_help_bonus;
            let base = self.config.select_evaluate.team_help_bonus_base / 100.0;
            pokemon.energy_per_day += pokemon.energy_per_day
                * (1.0 - pokemon.speed_bonus)
                * (1.0 / (1.0 - ((thb + 1.0) * 0.05 + base).min(0.35)) - 1.0 / (1.0 - base))
                / (thb + 1.0)
                * 4.0;
        }
        Ok(())
    }

    /// 本家 `selectEvaluateToScore` の `(apply_experience_bonus=false, true)` を同時に計算する
    #[inline]
    pub fn select_evaluate_scores(&self, pokemon: &SimPokemon, yumebo: bool, risabo: bool) -> (f64, f64) {
        let se = &self.config.select_evaluate;
        let mut raw = pokemon.energy_per_day;
        if yumebo {
            raw *= 1.0 + 0.06 * se.shard_bonus / 100.0;
        }
        if risabo {
            raw *= 1.0 + 0.09 * se.shard_bonus / 100.0 / 2.0;
        }
        raw += pokemon.shard * se.shard_energy * se.shard_bonus / 100.0;

        let mut score = raw;
        if pokemon.has_sub_skill(SubSkillKind::SleepExpBonus) {
            score += se.sub_skill.suimin_exp_bonus.add;
            score += raw * (se.sub_skill.suimin_exp_bonus.rate - 1.0);
        }
        let nature = pokemon.nature();
        if nature.is_some_and(|n| n.good_stat == Some(NatureStat::Exp)) {
            score += se.nature.exp_up.add;
            score += raw * (se.nature.exp_up.rate - 1.0);
        }
        if nature.is_some_and(|n| n.weak_stat == Some(NatureStat::Exp)) {
            score += se.nature.exp_down.add;
            score += raw * (se.nature.exp_down.rate - 1.0);
        }
        (raw, score)
    }

    /// 本家 `selectEvaluateToScore`
    /// `apply_experience_bonus` が true のときは、睡眠EXPボーナス・性格(EXP獲得量)の補正も掛ける
    pub fn select_evaluate_to_score(&self, pokemon: &SimPokemon, yumebo: bool, risabo: bool, apply_experience_bonus: bool) -> f64 {
        let (raw, score) = self.select_evaluate_scores(pokemon, yumebo, risabo);
        if apply_experience_bonus { score } else { raw }
    }
}

impl crate::data::Nature {
    /// アメ1個の経験値(性格補正)
    fn pick_exp(nature: Option<&crate::data::Nature>) -> f64 {
        crate::data::Nature::pick(nature, NatureStat::Exp, 30.0, 21.0, 25.0)
    }
}

/// レベルで有効になるサブスキルの数
pub fn enabled_sub_skill_len(lv: f64) -> usize {
    [10.0, 25.0, 50.0, 70.0, 80.0].iter().filter(|&&x| lv >= x).count()
}

fn sub_skill_index(id: u32) -> usize {
    let m = master();
    m.sub_skills.iter().position(|s| s.id == id).expect("サブスキルID")
}

pub fn nature_index(name: &str) -> usize {
    master().natures.iter().position(|n| n.name == name).expect("性格")
}

fn berry_rate_is_match(berry_rate: f64) -> bool {
    berry_rate != 100.0
}

/// スキルレベルに応じた効果(本家 `skill.effect[(len >= lv ? lv : len) - 1]`)
fn effect_at(skill: &Skill, fixed_skill_lv: f64) -> Option<&crate::data::SkillEffect> {
    let len = skill.max_lv() as f64;
    let lv = if len >= fixed_skill_lv { fixed_skill_lv } else { len };
    effect_index(skill, lv - 1.0)
}

fn effect_index(skill: &Skill, index: f64) -> Option<&crate::data::SkillEffect> {
    if index.is_nan() || index < 0.0 || index.fract() != 0.0 {
        return None;
    }
    skill.effect.get(index as usize)
}

fn index_f<T: Clone>(list: &[T], index: f64) -> Option<T> {
    if index.is_nan() || index < 0.0 || index.fract() != 0.0 {
        return None;
    }
    list.get(index as usize).cloned()
}

fn nan_pair() -> crate::data::SelfOther {
    crate::data::SelfOther { self_: f64::NAN, other: f64::NAN }
}

fn effect_pair(effect: Option<&crate::data::SkillEffect>, help_boost_count: Option<f64>) -> crate::data::SelfOther {
    let idx = help_boost_count.unwrap_or(f64::NAN) - 1.0;
    match effect {
        Some(e) if idx >= 0.0 && idx.fract() == 0.0 => e.team_pair(idx as usize),
        _ => nan_pair(),
    }
}

fn team_num_at(e: &crate::data::SkillEffect, idx: f64) -> f64 {
    if idx >= 0.0 && idx.fract() == 0.0 { e.team_num(idx as usize) } else { f64::NAN }
}

/// 降順に並べた上位5件の合計
fn sum_top5(list: &mut [f64]) -> f64 {
    list.sort_by(|a, b| js::cmp_num(*b, *a));
    list.iter().take(5).fold(0.0, |s, x| s + x)
}

/// 1日の発動回数を3回の料理に振り分けたときの鍋の拡張量
fn split_three(effect_num: f64, one_effect: f64) -> [f64; 3] {
    let min_values = (effect_num / 3.0).floor();
    let mut remain = effect_num - min_values * 3.0;
    let mut result = [0.0; 3];
    for r in &mut result {
        if remain >= 1.0 {
            *r = (min_values + 1.0) * one_effect;
            remain -= 1.0;
        } else {
            *r = (min_values + remain) * one_effect;
            remain = 0.0;
        }
    }
    result
}

/// 本家 `calcFoodProbList`。AAA を「2個得る確率と追加で3個得る確率…」の形に変換する。
pub fn calc_food_prob_list(food_list: &[SimFood]) -> Vec<FoodProb> {
    let len = food_list.len() as f64;
    let mut list: Vec<FoodProb> = Vec::new();
    for f in food_list {
        let mut num = f.num;
        for fp in &mut list {
            if fp.food == f.food {
                num -= fp.num;
                fp.weight += 1.0 / len;
            }
        }
        list.push(FoodProb { food: f.food, num, energy: f.energy, weight: 1.0 / len });
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ribbon_2000h_does_not_stack_with_500h() {
        let m = master();
        let config = Config::default_config();
        let mut sim = PokemonSimulator::new(config, Mode::About);
        let pikachu = m.pokemon("ピカチュウ").unwrap(); // remain_evolve_lv == 1
        let mut base_p = |sleep: f64| {
            let mut p = sim
                .from_box(
                    &BoxPokemon {
                        name: "ピカチュウ".into(),
                        lv: 10.0,
                        sleep_time: Some(sleep),
                        food_list: vec!["とくせんリンゴ".into(); 3],
                        sub_skill_list: vec![String::new(); 5],
                        nature: "まじめ".into(),
                        ..Default::default()
                    },
                    false,
                    0.0,
                    0.0,
                )
                .unwrap();
            sim.calc_status(&mut p, 0.0, 0.0, None);
            p.speed
        };
        let s0 = base_p(0.0);
        let s500 = base_p(500.0);
        let s2000 = base_p(2000.0);
        assert!((s500 - s0 * 0.95).abs() < 1e-6);
        assert!((s2000 - s0 * 0.88).abs() < 1e-6, "remain_evolve_lv=1 at 2000h should be 0.88x, got {s2000} vs {}", s0 * 0.88);
        assert_eq!(pikachu.remain_evolve_lv, 1);
    }
}
