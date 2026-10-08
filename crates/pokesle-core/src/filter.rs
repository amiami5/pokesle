//! ボックスの絞り込み(本家 `models/pokemon-filter.js`)

use serde_json::Value;

use crate::config::FilterConfig;
use crate::data::master;
use crate::sim_pokemon::BoxPokemon;

/// 絞り込み条件の種類
pub const FILTER_TYPE_LIST: [(&str, &str); 5] = [
    ("pokemon", "指定ポケモン"),
    ("pokemon_tree", "指定ポケモン(進化前含む)"),
    ("lv_lte", "指定レベル以下"),
    ("lv_gte", "指定レベル以上"),
    ("skill", "指定スキル"),
];

#[derive(Debug, Clone)]
pub struct FilterStep {
    pub name: String,
    pub mode: bool,
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct FilterResult {
    pub step_list: Vec<FilterStep>,
    pub pokemon_list: Vec<BoxPokemon>,
    pub exclude_list: Vec<BoxPokemon>,
}

fn value_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

fn value_num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => s.trim().parse().unwrap_or(f64::NAN),
        Value::Null => 0.0,
        Value::Bool(b) => *b as i32 as f64,
        _ => f64::NAN,
    }
}

pub fn filter_pokemon(pokemon_box: &[BoxPokemon], filter: &FilterConfig) -> FilterResult {
    let m = master();
    let mut pokemon_list: Vec<BoxPokemon> = pokemon_box.to_vec();
    let mut exclude_list: Vec<BoxPokemon> = Vec::new();
    let mut step_list = Vec::new();

    if filter.enable {
        for condition in &filter.condition_list {
            let mode_name = if condition.mode { "追加" } else { "除外" };
            let v = &condition.value;
            let vs = value_str(v);
            let (func, step_name): (Option<Box<dyn Fn(&BoxPokemon) -> bool>>, String) = match condition.condition_type.as_str() {
                "pokemon" => {
                    let vs2 = vs.clone();
                    (Some(Box::new(move |x: &BoxPokemon| x.name == vs2)), format!("{vs}を{mode_name}"))
                }
                "pokemon_tree" => {
                    let vs2 = vs.clone();
                    (
                        Some(Box::new(move |x: &BoxPokemon| m.pokemon(&x.name).is_some_and(|p| p.after_list.contains(&vs2)))),
                        format!("最終進化が{vs}のポケモンを{mode_name}"),
                    )
                }
                "lv_lte" => {
                    let n = value_num(v);
                    (Some(Box::new(move |x: &BoxPokemon| x.lv <= n)), format!("Lv{vs}以下のポケモンを{mode_name}"))
                }
                "lv_gte" => {
                    let n = value_num(v);
                    (Some(Box::new(move |x: &BoxPokemon| x.lv >= n)), format!("Lv{vs}以上のポケモンを{mode_name}"))
                }
                "skill" => {
                    let vs2 = vs.clone();
                    (
                        Some(Box::new(move |x: &BoxPokemon| m.pokemon(&x.name).is_some_and(|p| m.pokemon_skill(p).name == vs2))),
                        format!("スキルが{vs}のポケモンを{mode_name}"),
                    )
                }
                _ => (None, String::new()),
            };

            if let Some(func) = &func
                && !condition.disabled
            {
                if condition.mode {
                    pokemon_list.extend(exclude_list.iter().filter(|x| func(x)).cloned());
                } else {
                    let mut kept = Vec::with_capacity(pokemon_list.len());
                    for x in pokemon_list.drain(..) {
                        if func(&x) {
                            exclude_list.push(x);
                        } else {
                            kept.push(x);
                        }
                    }
                    pokemon_list = kept;
                }
            }
            step_list.push(FilterStep {
                name: step_name,
                mode: condition.mode,
                count: if condition.disabled { None } else { Some(pokemon_list.len()) },
            });
        }
    }

    FilterResult { step_list, pokemon_list, exclude_list }
}
