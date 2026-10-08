//! 設定。
//!
//! 初期値は本家の `src/data/default-config.ts` を書き出した `data/default-config.json`。
//! 保存済み設定は本家の `mergeObject` と同じく「初期値に存在するキーだけを上書き、配列は丸ごと置換」で
//! マージしてから型付きの構造体へ変換する。計算で参照しない項目は `extra` にそのまま保持する。

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

static DEFAULT_CONFIG_JSON: &str = include_str!("../data/default-config.json");

/// 本家の初期設定(JSON)
pub fn default_config_value() -> Value {
    serde_json::from_str(DEFAULT_CONFIG_JSON).expect("default-config.json の読み込みに失敗しました")
}

/// 本家の `mergeObject`。`base` に存在するキーだけを `overlay` で上書きする。
pub fn merge_object(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (key, value) in b.iter_mut() {
                if let Some(ov) = o.get(key) {
                    merge_object(value, ov);
                }
            }
        }
        (Value::Object(_), _) => {}
        (b, o) => *b = o.clone(),
    }
}

/// JS の真偽判定(null/0/""/false が偽)で bool に変換する
pub fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn de_truthy<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(js_truthy(&Value::deserialize(d)?))
}

/// 数値・真偽値・null を数値にする(true=1, false/null=0)
fn de_js_num<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Bool(b) => b as i32 as f64,
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    })
}

/// 数値または null(空文字も null 扱い)
fn de_opt_num<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if !s.is_empty() => s.parse().ok(),
        Value::Bool(b) => Some(b as i32 as f64),
        _ => None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub v: f64,
    pub sleep_time: f64,
    pub check_freq: f64,
    pub version: VersionConfig,
    pub simulation: SimulationConfig,
    pub team_simulation: TeamSimulationConfig,
    #[serde(default)]
    pub food_default_num: HashMap<String, f64>,
    #[serde(default, deserialize_with = "de_truthy")]
    pub food_unlimited: bool,
    pub select_evaluate: EvaluateConfig,
    pub tmp_evaluate: EvaluateConfig,
    pub candy: CandyConfig,
    pub food_stock: Value,
    pub worker_num: f64,
    pub summary: SummaryConfig,
    /// 実行時にだけ指定される: 純粋なミントでの評価も行う
    #[serde(default, deserialize_with = "de_truthy", skip_serializing_if = "std::ops::Not::not")]
    pub pure_mint: bool,
    /// 実行時にだけ指定される: ボックス整理モード
    #[serde(default, deserialize_with = "de_truthy", skip_serializing_if = "std::ops::Not::not")]
    pub cleaning: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionConfig {
    pub history: Value,
    pub evaluate_table: Option<Value>,
    pub evaluate_table_sleep_time: f64,
    pub evaluate_table_check_freq: f64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBonusTarget {
    #[serde(default)]
    pub types: HashMap<String, bool>,
    #[serde(default)]
    pub specialties: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBonus {
    pub target: EventBonusTarget,
    pub berry: f64,
    pub food: f64,
    pub skill_rate: f64,
    pub skill_lv: f64,
    pub bag: f64,
    pub bag_rate: f64,
}

impl Default for EventBonus {
    /// 本家の `createEventBonus()`
    fn default() -> Self {
        let m = crate::data::master();
        EventBonus {
            target: EventBonusTarget {
                types: m.berries.iter().map(|b| (b.type_name.clone(), false)).collect(),
                specialties: ["きのみ", "食材", "スキル"].iter().map(|s| (s.to_string(), false)).collect(),
            },
            berry: 0.0,
            food: 0.0,
            skill_rate: 1.0,
            skill_lv: 0.0,
            bag: 0.0,
            bag_rate: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEventBonus {
    pub berry_burst: f64,
    pub food_get: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBonusGlobal {
    pub skill: SkillEventBonus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CookingSetting {
    #[serde(default)]
    pub lv: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCondition {
    #[serde(rename = "type", default)]
    pub condition_type: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default, deserialize_with = "de_truthy")]
    pub mode: bool,
    #[serde(default, deserialize_with = "de_truthy")]
    pub disabled: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterConfig {
    #[serde(default, deserialize_with = "de_truthy")]
    pub enable: bool,
    #[serde(default)]
    pub condition_list: Vec<FilterCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationConfig {
    pub select_type: f64,
    pub select_border: f64,
    pub field: String,
    pub field_ex: f64,
    pub field_bonus: f64,
    pub berry_list: Vec<String>,
    pub berry_energy_rate: HashMap<String, f64>,
    /// 空文字は「全カテゴリ」
    #[serde(default)]
    pub cooking_type: Option<String>,
    pub cooking_weight: f64,
    pub shard_weight: f64,
    #[serde(deserialize_with = "de_truthy")]
    pub camp_ticket: bool,
    #[serde(deserialize_with = "de_truthy")]
    pub genki_full: bool,
    pub event_bonus_list: Vec<EventBonus>,
    pub event_bonus: EventBonusGlobal,
    pub pot_size: f64,
    #[serde(deserialize_with = "de_truthy")]
    pub bag_over_operation: bool,
    #[serde(deserialize_with = "de_truthy")]
    pub research_rank_max: bool,

    #[serde(deserialize_with = "de_truthy")]
    pub fix: bool,
    #[serde(deserialize_with = "de_opt_num")]
    pub fix_lv: Option<f64>,
    #[serde(deserialize_with = "de_truthy")]
    pub fix_evolve: bool,
    #[serde(deserialize_with = "de_truthy")]
    pub fix_evolve_exclude_sleep: bool,
    pub fix_resource_mode: f64,
    #[serde(deserialize_with = "de_truthy")]
    pub fix_sub_skill_seed: bool,
    /// false/0: なし、1: 厳選設定のスキルレベル、2: 最大
    #[serde(deserialize_with = "de_js_num")]
    pub fix_skill_seed: f64,
    #[serde(deserialize_with = "de_truthy")]
    pub fix_check_list: bool,
    #[serde(deserialize_with = "de_opt_num")]
    pub fix_border: Option<f64>,
    #[serde(deserialize_with = "de_opt_num")]
    pub fix_border_specialty: Option<f64>,
    pub fix_filter: FilterConfig,

    pub enable_cooking: HashMap<String, bool>,
    pub cooking_settings: HashMap<String, CookingSetting>,
    #[serde(deserialize_with = "de_truthy")]
    pub cooking_recipe_lv_type1: bool,
    #[serde(deserialize_with = "de_truthy")]
    pub cooking_recipe_lv_type2: bool,
    #[serde(deserialize_with = "de_truthy")]
    pub cooking_recipe_lv_type3: bool,
    #[serde(deserialize_with = "de_truthy")]
    pub cooking_recipe_lv_type4: bool,
    pub cooking_recipe_fix_lv: f64,
    pub cooking_recipe_repeat_lv: f64,
    pub cooking_recipe_repeat_lv2: f64,
    pub remain_food_mode: f64,
    pub remain_food_rate: f64,
    pub mode: f64,
    pub filter: FilterConfig,
    pub expect_type: HashMap<String, f64>,
    pub skill_rate: HashMap<String, f64>,

    // 以下は実行時にだけ指定される
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub berry_energy_weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub food_energy_weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub skill_energy_ignore: Option<f64>,
    #[serde(default, deserialize_with = "de_truthy", skip_serializing_if = "std::ops::Not::not")]
    pub sunday_prepare: bool,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub help_bonus: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub shard_to_energy: Option<f64>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl SimulationConfig {
    /// 料理カテゴリ(空文字・未指定は None)
    pub fn cooking_type(&self) -> Option<crate::data::CookingType> {
        self.cooking_type.as_deref().and_then(crate::data::CookingType::from_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRequire {
    pub day_help_rate: f64,
    pub night_help_rate: f64,
    pub suimin_exp: f64,
    pub specialty_num: HashMap<String, f64>,
    pub type_num: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSimulationConfig {
    pub max_rank_berry: f64,
    pub max_rank_food: f64,
    pub max_rank_not_support: f64,
    pub max_rank_all: f64,
    pub result_num: f64,
    pub cooking_num: f64,
    pub night_cap_pikachu: f64,
    pub food_get_evaluate_type: f64,
    pub food_get_evaluate_rate: f64,
    pub require: TeamRequire,

    // 以下は実行時にだけ指定される
    /// 曜日(0:月曜～6:日曜)。None は1週間
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub day: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub time_length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub initial_cooking_change: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub initial_cooking_power_up: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub before_energy: Option<f64>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecialtyEvaluateConfig {
    pub berry_energy_rate: f64,
    pub food_energy_rate: f64,
    pub food_get_rate: f64,
    #[serde(default, deserialize_with = "de_opt_num")]
    pub skill_lv_type: Option<f64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PokemonTarget {
    #[serde(rename = "type", default)]
    pub target_type: Option<String>,
    #[serde(default, deserialize_with = "de_truthy")]
    pub all: bool,
    #[serde(default)]
    pub specialties: BTreeMap<String, bool>,
    #[serde(default)]
    pub skill_name_list: Vec<String>,
    #[serde(default, deserialize_with = "de_opt_num")]
    pub friend_point: Option<f64>,
    #[serde(default)]
    pub pokemon_name_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateRuleSetting {
    #[serde(rename = "type")]
    pub setting_type: String,
    #[serde(default)]
    pub skill_name: Option<String>,
    #[serde(default, deserialize_with = "de_opt_num")]
    pub value: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_num")]
    pub skill_lv_type: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_num")]
    pub skill_lv: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateRule {
    pub target: PokemonTarget,
    #[serde(default)]
    pub setting_list: Vec<EvaluateRuleSetting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRate {
    pub add: f64,
    pub rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubSkillEvaluate {
    pub suimin_exp_bonus: AddRate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NatureEvaluate {
    pub exp_up: AddRate,
    pub exp_down: AddRate,
}

/// 厳選評価の設定(`selectEvaluate` / `tmpEvaluate`)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateConfig {
    pub shard_energy_rate: f64,
    pub shard_energy: f64,
    pub shard_bonus: f64,
    pub silver_seed: HashMap<String, bool>,
    pub help_bonus: f64,
    pub team_help_bonus: f64,
    pub support_border: f64,
    pub support_rank_num: f64,
    pub cooking_power_up_type: f64,
    pub cooking_power_up_rate: f64,
    pub healer: f64,
    pub expect_type: HashMap<String, f64>,
    #[serde(deserialize_with = "de_truthy")]
    pub genki_full_if_self_heal: bool,
    pub level_list: BTreeMap<u32, bool>,
    pub specialty: HashMap<String, SpecialtyEvaluateConfig>,
    #[serde(default)]
    pub max_skill_lv_skill_name_list: Vec<String>,
    #[serde(default)]
    pub rule_list: Vec<EvaluateRule>,
    pub sub_skill: SubSkillEvaluate,
    pub nature: NatureEvaluate,
    pub skill_energy: HashMap<String, Vec<Option<f64>>>,
    pub pokemon_sleep_time: f64,
    pub energy_per_candy: f64,

    // tmpEvaluate のみ
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub sleep_time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub check_freq: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub worker_num: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub score_for_healer_evaluate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_num")]
    pub score_for_support_evaluate: Option<f64>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl EvaluateConfig {
    /// 有効なレベル(昇順)
    pub fn enabled_levels(&self) -> Vec<u32> {
        self.level_list.iter().filter(|(_, e)| **e).map(|(lv, _)| *lv).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandyConfig {
    #[serde(deserialize_with = "de_opt_num")]
    pub shard: Option<f64>,
    pub boost_multiply: f64,
    pub boost_shard: f64,
    /// s/m/l(アメS/M/L) と、各ポケモンのアメ名
    pub bag: HashMap<String, f64>,
}

impl CandyConfig {
    /// アメS/M/L を個別アメ換算した数
    pub fn free_candy(&self) -> f64 {
        let g = |k: &str| self.bag.get(k).copied().unwrap_or(0.0);
        g("s") * 3.0 + g("m") * 20.0 + g("l") * 100.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryConfig {
    pub checklist: Value,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Config {
    /// 本家の初期設定
    pub fn default_config() -> Config {
        Config::from_value(&default_config_value()).expect("初期設定の変換に失敗しました")
    }

    /// 保存済み設定を初期設定へマージして読み込む
    pub fn from_saved(saved: &Value) -> Result<Config, String> {
        let mut base = default_config_value();
        merge_object(&mut base, saved);
        Self::from_value(&base)
    }

    pub fn from_value(value: &Value) -> Result<Config, String> {
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("設定のシリアライズに失敗しました")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrip() {
        let config = Config::default_config();
        assert_eq!(config.sleep_time, 8.5);
        assert_eq!(config.select_evaluate.enabled_levels(), vec![30, 50, 60, 70]);
        let v = config.to_value();
        let back = Config::from_value(&v).unwrap();
        assert_eq!(back.check_freq, config.check_freq);
    }

    #[test]
    fn merge_keeps_only_default_keys() {
        let mut base = serde_json::json!({ "a": 1, "b": { "c": [1, 2], "d": 3 } });
        merge_object(&mut base, &serde_json::json!({ "a": 2, "x": 9, "b": { "c": [5], "e": 1 } }));
        assert_eq!(base, serde_json::json!({ "a": 2, "b": { "c": [5], "d": 3 } }));
    }
}
