//! シミュレーション中のポケモンの状態(本家 `type.ts` の `SimulatedPokemon`)

use serde::{Deserialize, Serialize};

use crate::config::EventBonus;
use crate::data::{master, Nature, Pokemon, Skill, SubSkill, SubSkillKind};

/// ボックスのポケモン(本家 `PokemonBoxType`)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoxPokemon {
    pub name: String,
    pub lv: f64,
    #[serde(default)]
    pub skill_lv: Option<f64>,
    #[serde(default)]
    pub food_list: Vec<String>,
    #[serde(default)]
    pub sub_skill_list: Vec<String>,
    pub nature: String,
    #[serde(default)]
    pub shiny: bool,
    #[serde(default)]
    pub fix: Option<f64>,
    #[serde(default)]
    pub sleep_time: Option<f64>,
    #[serde(default)]
    pub training: Option<f64>,
    #[serde(default)]
    pub next_exp: Option<f64>,
    #[serde(default)]
    pub memo: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    /// ボックス内の位置(保存時は含めない)
    #[serde(default, skip_serializing)]
    pub index: usize,
}

/// げんき回復イベント
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HealEvent {
    pub effect: f64,
    pub time: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub night: bool,
}

/// 発動するスキルと重み(ゆびをふる・スキルコピーでは複数になる)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillWeight {
    /// スキルのインデックス
    pub skill: usize,
    pub weight: f64,
    pub skill_lv: Option<f64>,
    /// スキルコピーのコピー元(チームのプール内インデックス)
    pub copy: Option<usize>,
}

impl SkillWeight {
    pub fn skill(&self) -> &'static Skill {
        &master().skills[self.skill]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimFood {
    /// 食材のインデックス
    pub food: usize,
    pub num: f64,
    pub base_num: f64,
    pub energy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoodProb {
    pub food: usize,
    pub num: f64,
    pub energy: f64,
    pub weight: f64,
}

/// シミュレーション結果の1ポケモン分
#[derive(Debug, Clone, Default)]
pub struct SimPokemon {
    pub box_pokemon: Option<BoxPokemon>,
    /// ポケモンのインデックス
    pub base: usize,
    pub lv: f64,
    pub food_list: Vec<SimFood>,
    pub food_prob_list: Vec<FoodProb>,
    pub skill_lv: Option<f64>,
    pub event_bonus_list: Vec<EventBonus>,
    pub sleep_time: Option<f64>,
    pub fixable: bool,
    pub before_name: Option<String>,
    pub after_list: Vec<String>,
    pub use_shard: f64,
    pub use_candy: f64,
    pub bag_over_operation: bool,

    /// 有効なサブスキル(未知の名前は None)
    pub sub_skill_list: Vec<Option<usize>>,
    /// 有効なサブスキルのビット集合
    pub sub_skill_mask: u32,
    pub next_sub_skill_list: Vec<bool>,
    /// 性格のインデックス(None は補正なし扱い)
    pub nature: Option<usize>,
    pub skill_weight_list: Vec<SkillWeight>,
    pub self_heal_list: Vec<HealEvent>,
    pub other_heal_list: Vec<HealEvent>,
    pub all_heal_list: Vec<HealEvent>,

    pub cooking_power_up_effect: f64,
    pub cooking_chance_effect: f64,
    pub support_energy_per_day: f64,
    pub support_shard_per_day: f64,
    pub shard: f64,

    pub berry_num: f64,
    pub berry_energy: f64,
    pub berry_rate: f64,
    /// きのみエナジー/手伝い(サポート計算用)
    pub b_ep_h4_spt: f64,
    pub b_ep_h: f64,
    pub food_rate: f64,
    pub food_num: f64,
    pub f_ep_h: f64,
    pub pickup_energy_per_help: f64,
    pub bag: f64,
    pub bag_full_help_num: f64,
    pub fixed_skill_lv: f64,
    pub skill_rate: f64,
    pub skill_ceil: f64,
    pub ceil_skill_rate: f64,
    pub nature_genki_multiplier: f64,

    pub speed_bonus: f64,
    pub speed: f64,
    pub base_day_help_num: f64,
    pub morning_heal_genki: f64,
    pub self_heal: f64,
    pub other_heal: f64,

    pub heal_effect: f64,
    pub day_help_num: f64,
    pub night_help_num: f64,
    pub day_help_rate: f64,
    pub night_help_rate: f64,
    pub average_help_rate: f64,
    pub normal_day_help_num: f64,
    pub normal_night_help_num: f64,
    pub normal_help_num: f64,
    pub berry_help_num: f64,
    pub b_ep_d: f64,
    pub berry_num_per_day: f64,
    pub f_ep_d: f64,
    pub food_num_per_day: f64,
    pub pickup_energy_per_day: f64,
    pub skill_per_day: f64,
    pub skill_energy: f64,
    /// スキル名ごとのエナジー(挿入順を保持)
    pub skill_energy_map: Vec<(usize, f64)>,
    pub burst_bonus: f64,
    pub skill_energy_per_day: f64,
    pub energy_per_day: f64,

    /// 食材ごとの1日あたりの獲得数(食材インデックス順)
    pub food_count: Vec<f64>,

    pub tmp_score: Option<f64>,
    pub score: f64,
}

impl SimPokemon {
    pub fn base(&self) -> &'static Pokemon {
        &master().pokemon[self.base]
    }

    pub fn skill(&self) -> &'static Skill {
        master().pokemon_skill(self.base())
    }

    pub fn nature(&self) -> Option<&'static Nature> {
        self.nature.map(|i| &master().natures[i])
    }

    pub fn has_sub_skill(&self, kind: SubSkillKind) -> bool {
        self.sub_skill_mask & kind.bit() != 0
    }

    pub fn sub_skills(&self) -> impl Iterator<Item = Option<&'static SubSkill>> + '_ {
        self.sub_skill_list.iter().map(|x| x.map(|i| &master().sub_skills[i]))
    }

    pub fn sub_skill_names(&self) -> Vec<Option<&'static str>> {
        self.sub_skills().map(|x| x.map(|s| s.name.as_str())).collect()
    }

    pub fn box_index(&self) -> Option<usize> {
        self.box_pokemon.as_ref().map(|b| b.index)
    }

    /// 本家 `skillEnergyMap[name] = value`
    pub fn set_skill_energy(&mut self, skill: usize, value: f64) {
        if let Some(entry) = self.skill_energy_map.iter_mut().find(|(s, _)| *s == skill) {
            entry.1 = value;
        } else {
            self.skill_energy_map.push((skill, value));
        }
    }

    pub fn skill_energy_of(&self, skill: usize) -> Option<f64> {
        self.skill_energy_map.iter().find(|(s, _)| *s == skill).map(|x| x.1)
    }

    /// チームシミュレーションへ渡す前の JSON 化(本家 `JSON.parse(JSON.stringify(...))`)と同様に、
    /// 非有限値を null(=数値演算では 0)として扱う。
    pub fn sanitize_non_finite(&mut self) {
        let fields = [
            &mut self.lv, &mut self.use_shard, &mut self.use_candy, &mut self.cooking_power_up_effect,
            &mut self.cooking_chance_effect, &mut self.support_energy_per_day, &mut self.support_shard_per_day,
            &mut self.shard, &mut self.berry_num, &mut self.berry_energy, &mut self.berry_rate,
            &mut self.b_ep_h4_spt, &mut self.b_ep_h, &mut self.food_rate, &mut self.food_num, &mut self.f_ep_h,
            &mut self.pickup_energy_per_help, &mut self.bag, &mut self.bag_full_help_num, &mut self.fixed_skill_lv,
            &mut self.skill_rate, &mut self.skill_ceil, &mut self.ceil_skill_rate, &mut self.nature_genki_multiplier,
            &mut self.speed_bonus, &mut self.speed, &mut self.base_day_help_num, &mut self.morning_heal_genki,
            &mut self.self_heal, &mut self.other_heal, &mut self.heal_effect, &mut self.day_help_num,
            &mut self.night_help_num, &mut self.day_help_rate, &mut self.night_help_rate, &mut self.average_help_rate,
            &mut self.normal_day_help_num, &mut self.normal_night_help_num, &mut self.normal_help_num,
            &mut self.berry_help_num, &mut self.b_ep_d, &mut self.berry_num_per_day, &mut self.f_ep_d,
            &mut self.food_num_per_day, &mut self.pickup_energy_per_day, &mut self.skill_per_day,
            &mut self.skill_energy, &mut self.burst_bonus, &mut self.skill_energy_per_day, &mut self.energy_per_day,
            &mut self.score,
        ];
        for f in fields {
            if !f.is_finite() {
                *f = 0.0;
            }
        }
        for f in &mut self.food_count {
            if !f.is_finite() {
                *f = 0.0;
            }
        }
        if let Some(s) = &mut self.tmp_score
            && !s.is_finite()
        {
            *s = 0.0;
        }
    }
}
