//! 旧形式の保存済み設定の変換(本家 `models/config.ts` の読み込み時の移行処理)
//!
//! 保存済みの設定(JSON)を、初期設定へマージする前に現在の形式へ直す。
//! 変換内容は `config.ts` / `evaluate-setting.ts` / `pokemon-target.ts` の `migrate*` と同じ。

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::data::master;

/// 本家の `EVALUATE_SETTING_MIGRATION_VERSION`
const EVALUATE_SETTING_MIGRATION_VERSION: f64 = 20260919.0;
/// 本家の `CHECK_LIST_POKEMON_TARGET_MIGRATION_VERSION`
const CHECK_LIST_POKEMON_TARGET_MIGRATION_VERSION: f64 = 20260928.0;
const SPECIALTY_LIST: [&str; 4] = ["きのみ", "食材", "スキル", "オール"];

fn truthy(v: Option<&Value>) -> bool {
    v.is_some_and(crate::config::js_truthy)
}

fn array_len(v: Option<&Value>) -> usize {
    v.and_then(|x| x.as_array()).map_or(0, |a| a.len())
}

/// 保存済み設定を現在の形式へ直す。変換できない項目はそのまま残して後続の処理を続ける
pub fn migrate_saved(saved: &mut Value) {
    let Some(root) = saved.as_object_mut() else { return };
    let saved_version = root.get("v").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // 旧の銀種設定(1つの真偽値)を、サブスキルごとの設定にする
    if let Some(select) = root.get_mut("selectEvaluate").and_then(|v| v.as_object_mut())
        && let Some(use_value) = present_owned(select, "silverSeedUse")
    {
        let map: Map<String, Value> = master().sub_skills.iter().filter(|s| s.next.is_some()).map(|s| (s.name.clone(), use_value.clone())).collect();
        select.insert("silverSeed".into(), Value::Object(map));
    }

    // ボックス整理のミント計算(旧: useMint の真偽値)
    let cleaning_missing = root.get("pokemonList").and_then(|v| v.get("cleaning")).and_then(|v| v.get("mintCalculation")).is_none_or(Value::is_null);
    if cleaning_missing {
        let list = root.entry("pokemonList").or_insert_with(|| json!({}));
        if !list.is_object() {
            *list = json!({});
        }
        let cleaning = list.as_object_mut().expect("object").entry("cleaning").or_insert_with(|| json!({}));
        if !cleaning.is_object() {
            *cleaning = json!({});
        }
        let use_mint = truthy(cleaning.get("useMint"));
        cleaning["mintCalculation"] = json!(if use_mint { 2 } else { 0 });
    }

    if saved_version < EVALUATE_SETTING_MIGRATION_VERSION {
        for key in ["selectEvaluate", "tmpEvaluate"] {
            if let Some(e) = root.get_mut(key) {
                migrate_evaluate_config(e);
            }
        }
    }
    for key in ["selectEvaluate", "tmpEvaluate"] {
        if let Some(list) = root.get_mut(key).and_then(|e| e.get_mut("ruleList")) {
            migrate_target_friend_point(list);
        }
    }

    if let Some(checklist) = root.get_mut("summary").and_then(|v| v.get_mut("checklist"))
        && (saved_version < CHECK_LIST_POKEMON_TARGET_MIGRATION_VERSION || needs_check_list_migration(checklist))
    {
        migrate_check_list_pokemon_condition(checklist);
    }
}

fn present_owned(map: &Map<String, Value>, key: &str) -> Option<Value> {
    map.get(key).filter(|v| !v.is_null()).cloned()
}

// ---- 評価設定(evaluate-setting.ts) ----

fn create_pokemon_target() -> Value {
    json!({
        "type": "all",
        "all": true,
        "specialties": { "きのみ": false, "食材": false, "スキル": false, "オール": false },
        "skillNameList": [],
        "friendPoint": null,
        "pokemonNameList": [],
    })
}

fn create_specialty_target(specialty: &str) -> Value {
    let mut t = create_pokemon_target();
    t["type"] = json!("condition");
    t["all"] = json!(false);
    t["specialties"][specialty] = json!(true);
    t
}

fn sorted_strings(v: Option<&Value>) -> Vec<String> {
    let mut list: Vec<String> = v.and_then(|x| x.as_array()).map(|a| a.iter().map(|s| s.as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default();
    list.sort();
    list
}

/// 旧とくい別スキルLv設定を、同じ結果になる対象条件付きルールへ移行する(本家 `migrateEvaluateConfig`)
pub fn migrate_evaluate_config(e: &mut Value) {
    let Some(obj) = e.as_object_mut() else { return };

    if obj.get("ruleList").is_some_and(|v| !v.is_null()) {
        if !obj.get("maxSkillLvSkillNameList").is_some_and(|v| !v.is_null()) {
            obj.insert("maxSkillLvSkillNameList".into(), json!([]));
        }
        let mut max_list: Vec<Value> = obj.get("maxSkillLvSkillNameList").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let old_rules: Vec<Value> = obj.get("ruleList").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut rule_list: Vec<Value> = Vec::new();
        let mut skill_lv_rule_map: HashMap<String, usize> = HashMap::new();

        for mut rule in old_rules {
            let Some(rule_obj) = rule.as_object_mut() else { continue };
            // 対象の種類を補う
            if let Some(target) = rule_obj.get_mut("target").and_then(|t| t.as_object_mut()) {
                let ty = target.get("type").cloned().unwrap_or(Value::Null);
                let all = truthy(target.get("all"));
                if ty.is_null() || (ty == "all" && !all) {
                    let new = if all {
                        "all"
                    } else if array_len(target.get("pokemonNameList")) > 0 {
                        "pokemon"
                    } else {
                        "condition"
                    };
                    target.insert("type".into(), json!(new));
                }
            }
            // setting → settingList
            if !rule_obj.get("settingList").is_some_and(Value::is_array) {
                let list = match rule_obj.get("setting") {
                    None | Some(Value::Null) => json!([]),
                    Some(s) => json!([s.clone()]),
                };
                rule_obj.insert("settingList".into(), list);
                rule_obj.remove("setting");
            }

            let ty = rule_obj.get("target").and_then(|t| t.get("type")).cloned().unwrap_or(Value::Null);
            if ty == "all" {
                let setting_list = rule_obj.get("settingList").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let is_max = |s: &Value| s.get("type") == Some(&json!("skillLv")) && s.get("skillLvType").and_then(|v| v.as_f64()) == Some(2.0);
                for s in setting_list.iter().filter(|s| is_max(s)) {
                    if let Some(name) = s.get("skillName").filter(|n| !n.is_null())
                        && !max_list.contains(name)
                    {
                        max_list.push(name.clone());
                    }
                }
                let rest: Vec<Value> = setting_list.into_iter().filter(|s| !is_max(s)).collect();
                rule_obj.insert("settingList".into(), Value::Array(rest));
            }
            if array_len(rule_obj.get("settingList")) == 0 {
                continue;
            }

            let setting_list = rule_obj.get("settingList").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let skill_lv_only = setting_list.iter().all(|s| s.get("type") == Some(&json!("skillLv")));
            let target = rule_obj.get("target").cloned().unwrap_or(Value::Null);
            let key = json!({
                "type": target.get("type"),
                "specialties": target.get("specialties"),
                "skillNameList": sorted_strings(target.get("skillNameList")),
                "pokemonNameList": sorted_strings(target.get("pokemonNameList")),
            })
            .to_string();
            if skill_lv_only
                && let Some(&idx) = skill_lv_rule_map.get(&key)
            {
                if let Some(list) = rule_list[idx].get_mut("settingList").and_then(|v| v.as_array_mut()) {
                    list.extend(setting_list);
                }
                continue;
            }
            rule_list.push(rule);
            if skill_lv_only {
                skill_lv_rule_map.insert(key, rule_list.len() - 1);
            }
        }
        obj.insert("maxSkillLvSkillNameList".into(), Value::Array(max_list));
        obj.insert("ruleList".into(), Value::Array(rule_list));
        return;
    }

    // ruleList が無い旧形式: とくいごとの skillLv 設定
    let Some(old) = obj.get("specialty").cloned().filter(Value::is_object) else { return };
    let mut old = old;
    if !obj.get("maxSkillLvSkillNameList").is_some_and(|v| !v.is_null()) {
        obj.insert("maxSkillLvSkillNameList".into(), json!([]));
    }
    let mut max_list: Vec<Value> = obj.get("maxSkillLvSkillNameList").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut rule_list: Vec<Value> = Vec::new();
    let mut rule_map: HashMap<String, usize> = HashMap::new();

    let mut add_skill_lv_rule = |rule_list: &mut Vec<Value>, key: &str, target: Value, skill_name: &str, setting: &Value| {
        let idx = *rule_map.entry(key.to_string()).or_insert_with(|| {
            rule_list.push(json!({ "target": target, "settingList": [] }));
            rule_list.len() - 1
        });
        if let Some(list) = rule_list[idx].get_mut("settingList").and_then(|v| v.as_array_mut()) {
            // 値が無い項目は JSON にしたとき消える(JS の undefined)
            let mut rule = json!({ "type": "skillLv", "skillName": skill_name, "value": null });
            if let Some(t) = setting.get("type") {
                rule["skillLvType"] = t.clone();
            }
            if let Some(lv) = setting.get("lv") {
                rule["skillLv"] = lv.clone();
            }
            list.push(rule);
        }
    };

    for skill in &master().skills {
        let settings: Vec<Option<&Value>> = SPECIALTY_LIST
            .iter()
            .map(|sp| old.get(*sp).and_then(|c| c.get("skillLv")).and_then(|l| l.get(&skill.name)).filter(|v| !v.is_null()))
            .collect();
        if settings.iter().any(|s| s.is_none()) {
            continue;
        }
        let settings: Vec<&Value> = settings.into_iter().flatten().collect();
        let ty = |s: &Value| s.get("type").and_then(|v| v.as_f64());
        let is_all_max = settings.iter().all(|s| ty(s) == Some(2.0));
        let first_lv = settings[0].get("lv");
        let is_all_same_level = settings.iter().all(|s| ty(s) == Some(3.0) && s.get("lv") == first_lv);
        if is_all_max {
            max_list.push(json!(skill.name));
            continue;
        }
        if is_all_same_level {
            add_skill_lv_rule(&mut rule_list, "all", create_pokemon_target(), &skill.name, settings[0]);
            continue;
        }
        for sp in ["きのみ", "食材"] {
            if let Some(s) = old.get(sp).and_then(|c| c.get("skillLv")).and_then(|l| l.get(&skill.name)).filter(|v| !v.is_null())
                && ty(s) != Some(1.0)
            {
                add_skill_lv_rule(&mut rule_list, sp, create_specialty_target(sp), &skill.name, s);
            }
        }
        for sp in ["スキル", "オール"] {
            if let Some(s) = old.get(sp).and_then(|c| c.get("skillLv")).and_then(|l| l.get(&skill.name)).filter(|v| !v.is_null())
                && ty(s) != Some(2.0)
            {
                add_skill_lv_rule(&mut rule_list, sp, create_specialty_target(sp), &skill.name, s);
            }
        }
    }

    for sp in SPECIALTY_LIST {
        if let Some(c) = old.get_mut(sp).and_then(|c| c.as_object_mut()) {
            c.insert("skillLvType".into(), json!(if sp == "きのみ" || sp == "食材" { 1 } else { 2 }));
            c.remove("skillLv");
        }
    }
    obj.insert("specialty".into(), old);
    obj.insert("maxSkillLvSkillNameList".into(), Value::Array(max_list));
    obj.insert("ruleList".into(), Value::Array(rule_list));
}

/// 対象条件を使う既存の評価ルールにも、未指定のFP条件を明示的に補う(本家 `migratePokemonTargetFriendPoint`)
pub fn migrate_target_friend_point(rule_list: &mut Value) {
    let Some(list) = rule_list.as_array_mut() else { return };
    for rule in list {
        if let Some(target) = rule.get_mut("target").and_then(|t| t.as_object_mut())
            && target.get("friendPoint").is_none_or(Value::is_null)
        {
            target.insert("friendPoint".into(), Value::Null);
        }
    }
}

// ---- チェックリスト(pokemon-target.ts) ----

fn needs_check_list_migration(checklist: &Value) -> bool {
    let Some(list) = checklist.get("pokemonCondition").and_then(|c| c.get("list")).and_then(|l| l.as_array()) else { return false };
    list.iter().any(|item| {
        let target = item.get("target");
        item.get("type").is_some_and(|t| !t.is_null())
            || target.is_none_or(Value::is_null)
            || !target.is_some_and(Value::is_object)
            || target.and_then(|t| t.get("specialties")).is_none_or(Value::is_null)
            || !target.and_then(|t| t.get("skillNameList")).is_some_and(Value::is_array)
            || target.and_then(|t| t.as_object()).is_none_or(|o| !o.contains_key("friendPoint"))
            || !target.and_then(|t| t.get("pokemonNameList")).is_some_and(Value::is_array)
    })
}

/// チェックリストの旧対象形式を、厳選設定と共通の対象条件へ移行する(本家 `migrateCheckListPokemonCondition`)
fn migrate_check_list_pokemon_condition(checklist: &mut Value) {
    let Some(list) = checklist.get_mut("pokemonCondition").and_then(|c| c.get_mut("list")).and_then(|l| l.as_array_mut()) else { return };
    let legacy_legend: Vec<String> =
        master().pokemon.iter().filter(|p| p.is_last && !p.field_list.is_empty() && p.legend).map(|p| p.name.clone()).collect();

    for item in list {
        let Some(item_obj) = item.as_object_mut() else { continue };
        let mut target = create_pokemon_target();
        let old_target = item_obj.get("target").cloned().unwrap_or(Value::Null);
        let item_type = item_obj.get("type").and_then(|v| v.as_f64());

        if old_target.is_object() {
            let ty = match old_target.get("type").filter(|t| !t.is_null()) {
                Some(t) => t.clone(),
                None => json!(if truthy(old_target.get("all")) {
                    "all"
                } else if array_len(old_target.get("pokemonNameList")) > 0 {
                    "pokemon"
                } else {
                    "condition"
                }),
            };
            target["all"] = json!(ty == "all");
            target["type"] = ty;
            for sp in SPECIALTY_LIST {
                let on = old_target.get("specialties").and_then(|s| s.get(sp)) == Some(&json!(true));
                target["specialties"][sp] = json!(on);
            }
            if let Some(l) = old_target.get("skillNameList").filter(|v| v.is_array()) {
                target["skillNameList"] = l.clone();
            }
            if let Some(fp) = old_target.get("friendPoint").filter(|v| v.is_number()) {
                target["friendPoint"] = fp.clone();
            }
            if let Some(l) = old_target.get("pokemonNameList").filter(|v| v.is_array()) {
                target["pokemonNameList"] = l.clone();
            }
        } else if item_type == Some(1.0) {
            target["type"] = json!("condition");
            target["all"] = json!(false);
            if let Some(name) = old_target.as_str() {
                target["specialties"][name] = json!(true);
            }
        } else if item_type == Some(2.0) {
            target["type"] = json!("pokemon");
            target["all"] = json!(false);
            target["pokemonNameList"] = if old_target.is_null() { json!([]) } else { json!([old_target]) };
        } else if item_type == Some(3.0) {
            target["type"] = json!("condition");
            target["all"] = json!(false);
            target["friendPoint"] = json!(30);
        }

        // v20260927 では伝説ポケモンを個別指定へ変換していたため、その結果もFP条件へ置き換える
        let names: Vec<String> = target["pokemonNameList"].as_array().map(|a| a.iter().filter_map(|n| n.as_str().map(String::from)).collect()).unwrap_or_default();
        if target["type"] == "pokemon" && names.len() == legacy_legend.len() && names.iter().all(|n| legacy_legend.contains(n)) {
            target["type"] = json!("condition");
            target["all"] = json!(false);
            target["friendPoint"] = json!(30);
            target["pokemonNameList"] = json!([]);
        }
        item_obj.insert("target".into(), target);
        item_obj.remove("type");
    }
}
