//! 厳選評価の対象別設定(本家 `models/evaluate-setting.ts`, `models/pokemon-target.ts`)

use crate::config::{EvaluateConfig, PokemonTarget};
use crate::data::{master, Pokemon, Specialty};

/// 対象条件で使うとくいの一覧
pub const TARGET_SPECIALTY_LIST: [&str; 4] = ["きのみ", "食材", "スキル", "オール"];

/// スキルレベルの決め方
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillLvSetting {
    /// 1: 最大進化時Lv、2: 最大Lv、3: 指定Lv
    pub setting_type: f64,
    pub lv: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PokemonEvaluateSetting {
    pub berry_energy_rate: f64,
    pub food_energy_rate: f64,
    pub food_get_rate: f64,
    pub skill_lv: SkillLvSetting,
}

/// 対象条件の判定で使う進化元のポケモン(本家 `getPokemonTargetBasePokemon`)
pub fn target_base_pokemon(pokemon: &Pokemon) -> &Pokemon {
    pokemon.seed.as_deref().and_then(|s| master().pokemon(s)).unwrap_or(pokemon)
}

/// 本家 `matchesPokemonTarget`
pub fn matches_pokemon_target(target: Option<&PokemonTarget>, pokemon: &Pokemon) -> bool {
    let Some(target) = target else { return false };
    if target.all {
        return true;
    }
    let selected: Vec<&str> = target.specialties.iter().filter(|(_, e)| **e).map(|(s, _)| s.as_str()).collect();
    if !selected.is_empty() && !selected.contains(&pokemon.specialty.name()) {
        return false;
    }
    let skill_name = &master().pokemon_skill(pokemon).name;
    if !target.skill_name_list.is_empty() && !target.skill_name_list.contains(skill_name) {
        return false;
    }
    if let Some(fp) = target.friend_point {
        // undefined < x は false になるため、FP未設定は条件を満たす扱い
        if target_base_pokemon(pokemon).fp.is_some_and(|x| x < fp) {
            return false;
        }
    }
    if !target.pokemon_name_list.is_empty() && !target.pokemon_name_list.contains(&pokemon.name) {
        return false;
    }
    !selected.is_empty() || !target.skill_name_list.is_empty() || target.friend_point.is_some() || !target.pokemon_name_list.is_empty()
}

/// 本家 `getPokemonEvaluateSetting`
pub fn get_pokemon_evaluate_setting(config: &EvaluateConfig, pokemon: &Pokemon) -> PokemonEvaluateSetting {
    let sp = config.specialty.get(pokemon.specialty.name());
    let mut result = PokemonEvaluateSetting {
        berry_energy_rate: sp.map(|x| x.berry_energy_rate).unwrap_or(f64::NAN),
        food_energy_rate: sp.map(|x| x.food_energy_rate).unwrap_or(f64::NAN),
        food_get_rate: sp.map(|x| x.food_get_rate).unwrap_or(f64::NAN),
        skill_lv: SkillLvSetting {
            setting_type: sp.and_then(|x| x.skill_lv_type).unwrap_or(
                if matches!(pokemon.specialty, Specialty::Berry | Specialty::Food) { 1.0 } else { 2.0 },
            ),
            lv: 1.0,
        },
    };

    let skill_name = &master().pokemon_skill(pokemon).name;
    if config.max_skill_lv_skill_name_list.contains(skill_name) {
        result.skill_lv = SkillLvSetting { setting_type: 2.0, lv: 1.0 };
    }

    for rule in &config.rule_list {
        if !matches_pokemon_target(Some(&rule.target), pokemon) {
            continue;
        }
        for setting in &rule.setting_list {
            let value = setting.value.unwrap_or(f64::NAN);
            match setting.setting_type.as_str() {
                "berryEnergyRate" => result.berry_energy_rate = value,
                "foodEnergyRate" => result.food_energy_rate = value,
                "foodGetRate" => result.food_get_rate = value,
                "skillLv" if setting.skill_name.as_deref() == Some(skill_name.as_str()) => {
                    result.skill_lv = SkillLvSetting {
                        setting_type: setting.skill_lv_type.unwrap_or(f64::NAN),
                        lv: setting.skill_lv.unwrap_or(f64::NAN),
                    };
                }
                _ => {}
            }
        }
    }
    result
}

/// 厳選設定のスキルレベル(fromEvaluate / fixSkillSeed で使う)。
/// 該当する種類がなければ `default` を返す。
pub fn resolve_skill_lv(setting: SkillLvSetting, pokemon: &Pokemon, default: Option<f64>) -> Option<f64> {
    let skill = master().pokemon_skill(pokemon);
    let mut lv = default;
    if setting.setting_type == 1.0 {
        lv = Some(pokemon.evolve_lv as f64);
    }
    if setting.setting_type == 2.0 {
        lv = Some(skill.max_lv() as f64);
    }
    if setting.setting_type == 3.0 {
        lv = Some(setting.lv);
    }
    lv
}
