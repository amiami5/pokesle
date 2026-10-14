//! データ閲覧画面(本家 `pages/data/*.vue`)の派生値

use crate::box_sim::BoxResult;
use crate::config::Config;
use crate::cooking::evaluate_lv_list;
use crate::data::{master, CookingType, Pokemon};
use crate::sim_pokemon::BoxPokemon;
use crate::team_sim::simulate_box;

// ---- ポケモン ----

/// ポケモンの表の派生値(本家 `data/pokemon.vue` の convert)
#[derive(Debug, Clone, Copy)]
pub struct PokemonDerived {
    /// スキル天井
    pub ceil: f64,
    /// スキル確率(天井込)
    pub skill_rate_with_ceil: f64,
    /// スキル/日(げんき0)
    pub skill_num: f64,
}

pub fn pokemon_derived(p: &Pokemon) -> PokemonDerived {
    let ceil = if p.specialty.name() == "スキル" { 144000.0 / p.help } else { 78.0 };
    let skill_rate_with_ceil = p.skill_rate / (1.0 - (1.0 - p.skill_rate).powf(ceil));
    PokemonDerived { ceil, skill_rate_with_ceil, skill_num: skill_rate_with_ceil * 86400.0 / p.help }
}

// ---- 食材 ----

/// 食材の表の1行(本家 `data/food.vue`)。料理の種類は カレー・サラダ・デザート の順
#[derive(Debug, Clone)]
pub struct FoodRow {
    pub food: usize,
    /// 種類ごとの必要最大数
    pub require: [f64; 3],
    /// 種類ごとの最大補正
    pub best_type_rate: [f64; 3],
    /// 種類ごとの最大料理エナジー
    pub max_cooking_energy: [f64; 3],
    /// 種類ごとの最大単品エナジー(レシピLv込)
    pub max_energy: [f64; 3],
    pub max_cooking_energy_average: f64,
    pub max_cooking_energy_geometric_mean: f64,
    /// 総合の最大単品エナジー
    pub max_energy_all: f64,
}

pub fn food_rows(config: &Config) -> Vec<FoodRow> {
    let m = master();
    let enabled: Vec<_> = evaluate_lv_list(Some(&config.simulation)).into_iter().filter(|c| c.enable).collect();
    m.foods
        .iter()
        .enumerate()
        .map(|(fi, food)| {
            let mut row = FoodRow {
                food: fi,
                require: [0.0; 3],
                best_type_rate: [1.0; 3],
                max_cooking_energy: [0.0; 3],
                max_energy: [0.0; 3],
                max_cooking_energy_average: 0.0,
                max_cooking_energy_geometric_mean: 0.0,
                max_energy_all: 0.0,
            };
            for (t, ty) in CookingType::ALL.iter().enumerate() {
                let list = enabled.iter().map(|c| c.cooking()).filter(|c| c.cooking_type == *ty && c.food_list.iter().any(|f| f.name == food.name));
                for c in list {
                    let num = c.food_list.iter().find(|f| f.name == food.name).map_or(0.0, |f| f.num);
                    row.require[t] = row.require[t].max(num);
                    row.best_type_rate[t] = row.best_type_rate[t].max(c.rate);
                    row.max_cooking_energy[t] = row.max_cooking_energy[t].max(c.energy);
                }
                let rate = row.best_type_rate[t];
                row.max_energy[t] = food.energy * rate * if rate > 1.0 { m.cooking_stats.max_recipe_bonus } else { 1.0 };
            }
            let e = row.max_cooking_energy;
            row.max_cooking_energy_average = (e[0] + e[1] + e[2]) / 3.0;
            row.max_cooking_energy_geometric_mean = (e[0] * e[1] * e[2]).cbrt();
            row.max_energy_all = row.max_energy[0].max(row.max_energy[1]).max(row.max_energy[2]);
            row
        })
        .collect()
}

// ---- 食材期待値・スキル期待値 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectKind {
    Food,
    Skill,
}

/// ポケモン(食材期待値・スキル期待値)の条件
#[derive(Debug, Clone)]
pub struct ExpectParam {
    pub kind: ExpectKind,
    pub lv: f64,
    pub nature: String,
    /// 0: なし、1: S、2: M、3: S と M
    pub sub_skill: u8,
    /// おてつだいスピード短縮(%)
    pub speed: f64,
    pub genki_full: bool,
}

impl ExpectParam {
    /// 本家の初期値(食材期待値は れいせい・げんき常時、スキル期待値は なまいき)
    pub fn default_for(kind: ExpectKind) -> ExpectParam {
        match kind {
            ExpectKind::Food => ExpectParam { kind, lv: 60.0, nature: "れいせい".into(), sub_skill: 3, speed: 35.0, genki_full: true },
            ExpectKind::Skill => ExpectParam { kind, lv: 60.0, nature: "なまいき".into(), sub_skill: 3, speed: 35.0, genki_full: false },
        }
    }
}

/// 最終進化のポケモンを食材構成ごとに並べたボックス(Lv に応じて解放されている枠まで)
pub fn expect_box_list(param: &ExpectParam) -> Vec<BoxPokemon> {
    let m = master();
    let combos: &[&str] = if param.lv < 30.0 {
        &["0"]
    } else if param.lv < 60.0 {
        &["00", "01"]
    } else {
        &["000", "001", "002", "010", "011", "012"]
    };
    let (s, mid) = match param.kind {
        ExpectKind::Food => ("食材確率アップS", "食材確率アップM"),
        ExpectKind::Skill => ("スキル確率アップS", "スキル確率アップM"),
    };
    let sub_skill_list: Vec<String> = match param.sub_skill {
        1 => vec![s.into()],
        2 => vec![mid.into()],
        3 => vec![s.into(), mid.into()],
        _ => vec![],
    };
    let mut out = Vec::new();
    for base in m.pokemon.iter().filter(|p| p.after_list.len() == 1 && p.after_list[0] == p.name) {
        for combo in combos {
            let names: Option<Vec<String>> = combo
                .chars()
                .map(|c| base.food_list.get(c as usize - '0' as usize).filter(|f| m.food(&f.name).is_some()).map(|f| f.name.clone()))
                .collect();
            if let Some(food_list) = names {
                out.push(BoxPokemon { name: base.name.clone(), lv: param.lv, food_list, sub_skill_list: sub_skill_list.clone(), nature: param.nature.clone(), ..Default::default() });
            }
        }
    }
    out
}

pub fn expect_simulate(config: &Config, param: &ExpectParam) -> Result<Vec<BoxResult>, String> {
    let mut c = config.clone();
    c.simulation.expect_type = [("food".to_string(), 0.0), ("skill".to_string(), 0.0)].into_iter().collect();
    c.simulation.help_bonus = Some(param.speed / 5.0);
    c.simulation.genki_full = param.genki_full;
    simulate_box(&c, &expect_box_list(param), None, true)
}
