//! 本家(upstream)を Node で実行した結果との突き合わせ。
//! 正解データは `tools/upstream-bridge/golden.mjs` で生成する。

use pokesle_core::box_sim::BoxSimulator;
use pokesle_core::config::Config;
use pokesle_core::data::master;
use pokesle_core::evaluate_table::{evaluate_pokemon, sub_skill_combination_list};
use pokesle_core::sim_pokemon::{BoxPokemon, SimPokemon};
use serde_json::Value;

fn load(name: &str) -> Value {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("golden file")).expect("json")
}

/// 相対誤差 1e-9 まで許容(Math.pow 等の実装差による最終桁の違い)
fn close(a: f64, b: f64) -> bool {
    if a.is_nan() && b.is_nan() {
        return true;
    }
    let diff = (a - b).abs();
    diff <= 1e-9 || diff <= 1e-9 * a.abs().max(b.abs())
}

fn expected_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if s == "NaN" => Some(f64::NAN),
        Value::String(s) if s == "Infinity" => Some(f64::INFINITY),
        Value::String(s) if s == "-Infinity" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

fn actual_field(p: &SimPokemon, key: &str) -> Option<f64> {
    Some(match key {
        "lv" => p.lv,
        "berryEnergy" => p.berry_energy,
        "berryNum" => p.berry_num,
        "bEpH" => p.b_ep_h,
        "foodRate" => p.food_rate,
        "foodNum" => p.food_num,
        "fEpH" => p.f_ep_h,
        "pickupEnergyPerHelp" => p.pickup_energy_per_help,
        "bag" => p.bag,
        "bagFullHelpNum" => p.bag_full_help_num,
        "fixedSkillLv" => p.fixed_skill_lv,
        "skillRate" => p.skill_rate,
        "ceilSkillRate" => p.ceil_skill_rate,
        "speed" => p.speed,
        "speedBonus" => p.speed_bonus,
        "baseDayHelpNum" => p.base_day_help_num,
        "morningHealGenki" => p.morning_heal_genki,
        "selfHeal" => p.self_heal,
        "otherHeal" => p.other_heal,
        "dayHelpRate" => p.day_help_rate,
        "nightHelpRate" => p.night_help_rate,
        "normalHelpNum" => p.normal_help_num,
        "berryHelpNum" => p.berry_help_num,
        "bEpD" => p.b_ep_d,
        "fEpD" => p.f_ep_d,
        "foodNumPerDay" => p.food_num_per_day,
        "skillPerDay" => p.skill_per_day,
        "skillEnergy" => p.skill_energy,
        "skillEnergyPerDay" => p.skill_energy_per_day,
        "energyPerDay" => p.energy_per_day,
        "shard" => p.shard,
        "burstBonus" => p.burst_bonus,
        "cookingChanceEffect" => p.cooking_chance_effect,
        "tmpScore" => p.tmp_score?,
        "score" => p.score,
        "supportEnergyPerDay" => p.support_energy_per_day,
        "supportShardPerDay" => p.support_shard_per_day,
        _ => return None,
    })
}

#[test]
fn box_simulation_matches_upstream() {
    let golden = load("box.json");
    let box_list: Vec<BoxPokemon> = serde_json::from_value(golden["box"].clone()).unwrap();
    let m = master();
    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0;

    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let config = Config::from_value(&case["config"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut sim = BoxSimulator::new(config);
        let basic = sim.basic(&box_list, None).unwrap_or_else(|e| panic!("{name}: {e}"));
        let result = sim.assist(&basic, 0..basic.len());
        let expected = case["result"].as_array().unwrap();
        assert_eq!(result.len(), expected.len(), "{name}: 件数");

        for (i, (r, e)) in result.iter().zip(expected).enumerate() {
            let p = &r.pokemon;
            let label = format!("{name}[{i}] {}{}", e["name"].as_str().unwrap(), if p.bag_over_operation { "(いつ育)" } else { "" });
            assert_eq!(p.base().name, e["name"].as_str().unwrap(), "{label}");
            for (key, ev) in e.as_object().unwrap() {
                if matches!(key.as_str(), "name" | "boxIndex" | "bagOverOperation" | "foods") {
                    continue;
                }
                let Some(exp) = expected_num(ev) else { continue };
                let act = actual_field(p, key).unwrap_or(f64::NAN);
                checked += 1;
                if !close(act, exp) {
                    errors.push(format!("{label}.{key}: rust={act} js={exp}"));
                }
            }
            for (food, ev) in e["foods"].as_object().unwrap() {
                let exp = expected_num(ev).unwrap_or(0.0);
                let act = p.food_count[m.food_index(food).unwrap()];
                checked += 1;
                if !close(act, exp) {
                    errors.push(format!("{label}.{food}: rust={act} js={exp}"));
                }
            }
        }
    }
    eprintln!("checked={checked}");
    if !errors.is_empty() {
        panic!("{} / {checked} 件不一致\n{}", errors.len(), errors.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }
}

#[test]
fn sub_skill_combination_matches_upstream() {
    let golden = load("evaluate.json");
    let config = Config::from_value(&golden["config"]).unwrap();
    let list = sub_skill_combination_list(&config.select_evaluate.silver_seed);
    for n in 1..=5 {
        let expected = golden["subSkillCombinationList"][n.to_string()].as_array().unwrap();
        assert_eq!(list[n].len(), expected.len(), "サブスキル{n}個の組み合わせ数");
        for (a, e) in list[n].iter().zip(expected) {
            let s: Vec<u32> = serde_json::from_value(e["s"].clone()).unwrap();
            assert_eq!(a.s, s);
            for code in 0..4 {
                let ev = &e[code.to_string()];
                match (&a.weights[code], ev) {
                    (None, Value::Null) => {}
                    (Some((count, ids)), Value::Array(arr)) => {
                        assert_eq!(*count, arr[0].as_f64().unwrap());
                        let eids: Vec<u32> = arr[1..].iter().map(|x| x.as_u64().unwrap() as u32).collect();
                        assert_eq!(*ids, eids);
                    }
                    other => panic!("不一致: {other:?}"),
                }
            }
        }
    }
}

/// 基準確認の詳細: 逆引きした個体の値が、同条件で作ったテーブルの指定厳選度の値に一致する
#[test]
fn evaluate_energy_detail_reproduces_table() {
    use pokesle_core::evaluate_table::{evaluate_energy_detail, EvaluateTable};
    let golden = load("evaluate.json");
    let config = Config::from_value(&golden["config"]).unwrap();
    let combination_list = sub_skill_combination_list(&config.select_evaluate.silver_seed);
    let m = master();
    let mut checked = 0;
    for case in golden["cases"].as_array().unwrap().iter().take(3) {
        let lv = case["lv"].as_u64().unwrap() as u32;
        let foods: Vec<String> = serde_json::from_value(case["foodCombinationList"].clone()).unwrap();
        let names: Vec<String> = serde_json::from_value(case["pokemon"].clone()).unwrap();
        let pokemon: Vec<_> = names.iter().take(2).map(|n| m.pokemon(n).unwrap()).collect();
        let sub_skill_num = pokemon_count_sub_skill(lv);
        let mut table = EvaluateTable::default();
        table.healer.insert(lv, 10000.0);
        table.support.insert(lv, 150.0);
        for base in &pokemon {
            let name = &base.name;
            for food in foods.iter().take(2) {
                // 本家の詳細表示と同じく、1ポケモン・1食材構成だけを新しいシミュレーターで評価した値と比べる
                let single = evaluate_pokemon(&config, lv, &[*base], &[food.clone()], &combination_list[sub_skill_num], 10000.0, 150.0, || {}).unwrap();
                let out = &single.result[0].1[0].1;
                for p in [0usize, 1, 37, 50, 99, 100] {
                    let d = evaluate_energy_detail(&config, &table, &combination_list, name, lv, food, p).unwrap();
                    assert!(!d.sub_skill_list.is_empty() || lv < 10);
                    let expected = out.percentile.energy[p];
                    assert!(close(d.score, expected), "Lv{lv} {name} {food} p={p}: {} != {expected}", d.score);
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0);
}

#[test]
fn evaluate_matches_upstream() {
    let golden = load("evaluate.json");
    let config = Config::from_value(&golden["config"]).unwrap();
    let combination_list = sub_skill_combination_list(&config.select_evaluate.silver_seed);
    let m = master();
    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0;

    for case in golden["cases"].as_array().unwrap() {
        let lv = case["lv"].as_u64().unwrap() as u32;
        let foods: Vec<String> = serde_json::from_value(case["foodCombinationList"].clone()).unwrap();
        let names: Vec<String> = serde_json::from_value(case["pokemon"].clone()).unwrap();
        let pokemon: Vec<_> = names.iter().map(|n| m.pokemon(n).unwrap()).collect();
        let sub_skill_num = pokemon_count_sub_skill(lv);
        let result = evaluate_pokemon(&config, lv, &pokemon, &foods, &combination_list[sub_skill_num], 10000.0, 150.0, || {})
            .unwrap_or_else(|e| panic!("Lv{lv}: {e}"));

        let cmp_list = |label: &str, act: &[f64], exp: &Value, errors: &mut Vec<String>, checked: &mut usize| {
            let exp = exp.as_array().unwrap();
            if act.len() != exp.len() {
                errors.push(format!("{label}: 件数 rust={} js={}", act.len(), exp.len()));
                return;
            }
            for (i, (a, e)) in act.iter().zip(exp).enumerate() {
                let e = expected_num(e).unwrap_or(f64::NAN);
                *checked += 1;
                if !close(*a, e) {
                    errors.push(format!("{label}[{i}]: rust={a} js={e}"));
                }
            }
        };
        cmp_list(&format!("Lv{lv}.healer"), &result.score_for_healer_evaluate_list, &case["healer"], &mut errors, &mut checked);
        cmp_list(&format!("Lv{lv}.support"), &result.score_for_support_evaluate_list, &case["support"], &mut errors, &mut checked);

        for (name, per_food) in &result.result {
            for (food, out) in per_food {
                let exp = &case["table"][name][food];
                for key in ["energy", "berry", "food", "skill", "food1", "food2", "food3"] {
                    cmp_list(&format!("Lv{lv}.{name}.{food}.{key}"), out.percentile.get(key), &exp[key], &mut errors, &mut checked);
                }
            }
        }
    }
    eprintln!("checked={checked}");
    if !errors.is_empty() {
        panic!("{} / {checked} 件不一致\n{}", errors.len(), errors.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }
}

fn pokemon_count_sub_skill(lv: u32) -> usize {
    if lv < 50 { 2 } else if lv < 70 { 3 } else if lv < 80 { 4 } else { 5 }
}

#[test]
fn team_simulation_matches_upstream() {
    use pokesle_core::team_sim::{run_team_simulation, TeamMember, TeamSimulationRequest};
    let golden = load("team.json");
    let m = master();
    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0;

    for case in golden.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let config = Config::from_value(&case["config"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut box_list: Vec<BoxPokemon> = serde_json::from_value(case["box"].clone()).unwrap();
        for (i, b) in box_list.iter_mut().enumerate() {
            b.index = i;
        }
        let req = &case["req"];
        let day = req["targetDay"].as_f64().unwrap();
        let request = TeamSimulationRequest {
            target_day: if day == -1.0 { None } else { Some(day) },
            target_hour: req["targetHour"].as_f64().unwrap(),
            initial_cooking_change: req["initialCookingChange"].as_f64().unwrap(),
            initial_cooking_power_up: req["initialCookingPowerUp"].as_f64().unwrap(),
            before_energy: req["beforeEnergy"].as_f64().unwrap(),
        };
        let result = run_team_simulation(&config, &box_list, None, &request, &|_| {}).unwrap_or_else(|e| panic!("{name}: {e}"));
        let expected = case["result"].as_array().unwrap();
        if result.team_list.len() != expected.len() {
            errors.push(format!("{name}: 件数 rust={} js={}", result.team_list.len(), expected.len()));
            continue;
        }
        for (i, (r, e)) in result.team_list.iter().zip(expected).enumerate() {
            let label = format!("{name}[{i}]");
            for (key, act) in [
                ("score", r.score),
                ("energy", r.energy),
                ("rawEnergy", r.raw_energy),
                ("skillShard", r.skill_shard),
                ("bonusShard", r.bonus_shard),
                ("remainFoodEnergy", r.remain_food_energy),
            ] {
                let exp = expected_num(&e[key]).unwrap_or(f64::NAN);
                checked += 1;
                if !close(act, exp) {
                    errors.push(format!("{label}.{key}: rust={act} js={exp}"));
                }
            }
            let act_names: Vec<String> = r
                .pokemon_list
                .iter()
                .map(|x| match x {
                    TeamMember::Pokemon { pokemon, .. } => format!("{}{}", pokemon.base().name, if pokemon.bag_over_operation { "(いつ育)" } else { "" }),
                    TeamMember::NightCapPikachu(_) => "ナイトキャップピカチュウ".into(),
                })
                .collect();
            let exp_names: Vec<String> = e["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| format!("{}{}", x["name"].as_str().unwrap(), if x["bagOverOperation"].as_bool().unwrap() { "(いつ育)" } else { "" }))
                .collect();
            checked += 1;
            if act_names != exp_names {
                errors.push(format!("{label}.members: rust={act_names:?} js={exp_names:?}"));
            }
            let act_cook: Vec<&str> = r.cooking_list.iter().map(|c| m.cookings[c.cooking].name.as_str()).collect();
            let exp_cook: Vec<&str> = e["cookingList"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
            checked += 1;
            if act_cook != exp_cook {
                errors.push(format!("{label}.cookingList: rust={act_cook:?} js={exp_cook:?}"));
            }
            for (f, food) in m.foods.iter().enumerate() {
                let exp = expected_num(&e["foodNum"][&food.name]).unwrap_or(0.0);
                checked += 1;
                if !close(r.food_num[f], exp) {
                    errors.push(format!("{label}.foodNum.{}: rust={} js={exp}", food.name, r.food_num[f]));
                }
            }
        }
    }
    eprintln!("checked={checked}");
    if !errors.is_empty() {
        panic!("{} / {checked} 件不一致\n{}", errors.len(), errors.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }
}

/// 本家形式の厳選テーブル(JSON)を Rust の型へ変換する
fn table_from_js(v: &Value) -> pokesle_core::evaluate_table::EvaluateTable {
    use pokesle_core::evaluate_table::{EvaluateTable, PercentileSet};
    let mut table = EvaluateTable::default();
    for (name, by_lv) in v.as_object().unwrap() {
        for (lv, by_food) in by_lv.as_object().unwrap() {
            let lv: u32 = lv.parse().unwrap();
            match name.as_str() {
                "scoreForHealerEvaluate" => {
                    table.healer.insert(lv, by_food[""]["energy"].as_f64().unwrap());
                }
                "scoreForSupportEvaluate" => {
                    table.support.insert(lv, by_food[""]["energy"].as_f64().unwrap());
                }
                _ => {
                    for (food, p) in by_food.as_object().unwrap() {
                        let p: PercentileSet = serde_json::from_value(p.clone()).unwrap();
                        table.pokemon.entry(name.clone()).or_default().entry(lv).or_default().insert(food.clone(), p);
                    }
                }
            }
        }
    }
    table
}

#[test]
fn box_with_evaluate_table_matches_upstream() {
    use pokesle_core::evaluate_table::EvalLv;
    let golden = load("box-fix.json");
    let box_list: Vec<BoxPokemon> = serde_json::from_value(golden["box"].clone()).unwrap();
    let table = table_from_js(&golden["table"]);
    let m = master();
    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0;

    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let config = Config::from_value(&case["config"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut sim = BoxSimulator::new(config);
        let result = sim.basic(&box_list, Some(&table)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let expected = case["result"].as_array().unwrap();
        if result.len() != expected.len() {
            errors.push(format!("{name}: 件数 rust={} js={}", result.len(), expected.len()));
            continue;
        }
        for (i, (r, e)) in result.iter().zip(expected).enumerate() {
            let p = &r.pokemon;
            let label = format!("{name}[{i}] {}", e["name"].as_str().unwrap());
            checked += 1;
            if p.base().name != e["name"].as_str().unwrap() {
                errors.push(format!("{label}: name rust={}", p.base().name));
                continue;
            }
            let mut fields: Vec<(&str, f64)> = Vec::new();
            for key in ["lv", "energyPerDay", "skillPerDay", "bag", "speed", "fixedSkillLv", "tmpScore", "dayHelpRate"] {
                fields.push((key, actual_field(p, key).unwrap_or(f64::NAN)));
            }
            fields.push(("useCandy", p.use_candy));
            fields.push(("useShard", p.use_shard));
            for (key, act) in fields {
                let Some(exp) = expected_num(&e[key]) else { continue };
                checked += 1;
                if !close(act, exp) {
                    errors.push(format!("{label}.{key}: rust={act} js={exp}"));
                }
            }
            for (f, food) in m.foods.iter().enumerate() {
                let exp = expected_num(&e["foods"][&food.name]).unwrap_or(0.0);
                checked += 1;
                if !close(p.food_count[f], exp) {
                    errors.push(format!("{label}.{}: rust={} js={exp}", food.name, p.food_count[f]));
                }
            }
            // 各レベルの最良の進化先
            let exp_best = e["best"].as_object().unwrap();
            let act_best = r.evaluate_result.as_ref().map(|x| &x.best);
            for (lv, keys) in exp_best {
                let lv_key = if lv == "max" { EvalLv::Max } else { EvalLv::Lv(lv.parse().unwrap()) };
                let act = act_best.and_then(|b| b.get(&lv_key));
                for (key, ev) in keys.as_object().unwrap() {
                    checked += 1;
                    let a = act.and_then(|x| x.get(key));
                    match (a, ev) {
                        (None, Value::Null) => {}
                        (Some(a), Value::Object(_)) => {
                            let exp_score = expected_num(&ev["score"]).unwrap_or(f64::NAN);
                            let exp_value = expected_num(&ev["value"]).unwrap_or(f64::NAN);
                            if a.name.as_deref() != ev["name"].as_str() || !close(a.score, exp_score) || !close(a.value, exp_value) {
                                errors.push(format!(
                                    "{label}.best[{lv}].{key}: rust=({:?},{},{}) js=({},{exp_score},{exp_value})",
                                    a.name, a.score, a.value, ev["name"]
                                ));
                            }
                        }
                        (a, e) => errors.push(format!("{label}.best[{lv}].{key}: rust={a:?} js={e}")),
                    }
                }
            }
        }
    }
    eprintln!("checked={checked}");
    if !errors.is_empty() {
        panic!("{} / {checked} 件不一致\n{}", errors.len(), errors.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }
}

#[test]
fn check_list_matches_upstream() {
    use pokesle_core::check_list::{food_combination, CheckHit, CheckList, CheckTarget};
    use pokesle_core::team_sim::simulate_box;
    let golden = load("check-list.json");
    let box_list: Vec<BoxPokemon> = serde_json::from_value(golden["box"].clone()).unwrap();
    let table = table_from_js(&golden["table"]);
    let m = master();
    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0;
    let num = |v: &Value| expected_num(v).unwrap_or(f64::NAN);
    let opt_idx = |v: &Value| v.as_u64().map(|x| x as usize);
    let strings = |v: &Value| -> Vec<String> { serde_json::from_value(v.clone()).unwrap() };
    let mut cmp = |errors: &mut Vec<String>, label: String, act: f64, exp: f64| {
        checked += 1;
        if !close(act, exp) {
            errors.push(format!("{label}: rust={act} js={exp}"));
        }
    };
    let same = |errors: &mut Vec<String>, label: String, ok: bool, detail: String| {
        if !ok {
            errors.push(format!("{label}: {detail}"));
        }
    };

    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let config = Config::from_value(&case["config"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let e_ = &mut errors;

        // 一覧
        let mut page_config = config.clone();
        page_config.simulation.bag_over_operation = false;
        page_config.simulation.fix = false;
        let all = simulate_box(&page_config, &box_list, Some(&table), true).unwrap();
        let list: Vec<_> = all.iter().collect();
        let empty = pokesle_core::evaluate_table::EvaluateResult::default();
        let combos: Vec<String> = list
            .iter()
            .map(|r| food_combination(&r.pokemon.base().food_name_list, &r.pokemon.box_pokemon.as_ref().unwrap().food_list))
            .collect();
        let targets: Vec<CheckTarget> = list
            .iter()
            .zip(&combos)
            .map(|(r, c)| CheckTarget { base_name: &r.pokemon.base().name, food_combination: c, evaluate_result: r.evaluate_result.as_ref().unwrap_or(&empty) })
            .collect();
        let page = &case["page"];
        let act_box: Vec<usize> = list.iter().map(|r| r.pokemon.box_index().unwrap()).collect();
        let exp_box: Vec<usize> = serde_json::from_value(page["boxIndexList"].clone()).unwrap();
        assert_eq!(act_box, exp_box, "{name}: 一覧の対象");

        let cl = CheckList::new(&config, Some(&table));
        let food = cl.food_check_list(&targets, None);
        let skill = cl.skill_check_list(&targets, None);
        let pokemon = cl.pokemon_check_list(&targets, None, Some(&food), Some(&skill));

        let exp_rows = page["pokemon"]["rows"].as_array().unwrap();
        same(e_, format!("{name}.pokemon"), pokemon.rows.len() == exp_rows.len(), format!("件数 rust={} js={}", pokemon.rows.len(), exp_rows.len()));
        for (r, e) in pokemon.rows.iter().zip(exp_rows) {
            let pname = &m.pokemon[r.pokemon].name;
            let label = format!("{name}.pokemon.{pname}");
            let act = (pname.as_str(), r.food_name.as_str(), r.checked, r.hit, r.target.as_str());
            let exp = (e["name"].as_str().unwrap(), e["subName"].as_str().unwrap(), e["checked"].as_bool().unwrap(), opt_idx(&e["hit"]), e["target"].as_str().unwrap());
            same(e_, label.clone(), act == exp, format!("rust={act:?} js={exp:?}"));
            cmp(e_, format!("{label}.energyScore"), r.energy_score.unwrap_or(f64::NAN), num(&e["energyScore"]));
            cmp(e_, format!("{label}.specialtyScore"), r.specialty_score.unwrap_or(f64::NAN), num(&e["specialtyScore"]));
            let el = e["list"].as_array().unwrap();
            same(e_, format!("{label}.list"), r.list.len() == el.len(), format!("件数 rust={} js={}", r.list.len(), el.len()));
            for ((i, s), x) in r.list.iter().zip(el) {
                same(e_, format!("{label}.list"), Some(*i) == opt_idx(&x[0]), format!("rust={i} js={}", x[0]));
                cmp(e_, format!("{label}.list[{i}]"), *s, num(&x[1]));
            }
        }
        same(e_, format!("{name}.pokemon.target"), pokemon.target_pokemon_list == strings(&page["pokemon"]["targetPokemonList"]), String::new());

        let exp_rows = page["food"]["rows"].as_array().unwrap();
        same(e_, format!("{name}.food"), food.rows.len() == exp_rows.len(), format!("件数 rust={} js={}", food.rows.len(), exp_rows.len()));
        for (r, e) in food.rows.iter().zip(exp_rows) {
            let label = format!("{name}.food.{}", m.foods[r.food].name);
            let act = (m.foods[r.food].name.as_str(), r.lv as u64, r.checked, r.hit, r.note(), r.checked_pokemon_list.clone());
            let exp = (
                e["food"].as_str().unwrap(),
                e["lv"].as_u64().unwrap(),
                e["checked"].as_bool().unwrap(),
                e["hit"].as_u64().unwrap() as usize,
                e["note"].as_str().unwrap().to_string(),
                strings(&e["checkedPokemonList"]),
            );
            same(e_, label.clone(), act == exp, format!("rust={act:?} js={exp:?}"));
            for (k, a) in [("bestFoodScore", r.best_food_score), ("maxNum", r.max_num * 3.0), ("border", r.border), ("score", r.score), ("rate", r.rate)] {
                cmp(e_, format!("{label}.{k}"), a, num(&e[k]));
            }
            let el = e["list"].as_array().unwrap();
            same(e_, format!("{label}.list"), r.list.len() == el.len(), format!("件数 rust={} js={}", r.list.len(), el.len()));
            for (x, y) in r.list.iter().zip(el) {
                same(e_, format!("{label}.list"), Some(x.index) == opt_idx(&y[0]) && Some(x.lv as u64) == y[2].as_u64(), format!("rust={x:?} js={y}"));
                cmp(e_, format!("{label}.list"), x.score, num(&y[1]));
            }
        }
        same(e_, format!("{name}.food.target"), food.target_pokemon_list == strings(&page["food"]["targetPokemonList"]), String::new());

        let exp_rows = page["skill"]["rows"].as_array().unwrap();
        same(e_, format!("{name}.skill"), skill.rows.len() == exp_rows.len(), format!("件数 rust={} js={}", skill.rows.len(), exp_rows.len()));
        for (r, e) in skill.rows.iter().zip(exp_rows) {
            let label = format!("{name}.skill.{}", m.skills[r.skill].name);
            let act = (m.skills[r.skill].name.as_str(), r.lv as u64, r.checked, r.hit, r.note(), r.checked_pokemon_list.clone());
            let exp = (
                e["skill"].as_str().unwrap(),
                e["lv"].as_u64().unwrap(),
                e["checked"].as_bool().unwrap(),
                e["hit"].as_u64().unwrap() as usize,
                e["note"].as_str().unwrap().to_string(),
                strings(&e["checkedPokemonList"]),
            );
            same(e_, label.clone(), act == exp, format!("rust={act:?} js={exp:?}"));
            for (k, a) in [("bestSkillScore", r.best_skill_score), ("border", r.border), ("score", r.score), ("rate", r.rate)] {
                cmp(e_, format!("{label}.{k}"), a, num(&e[k]));
            }
            let el = e["list"].as_array().unwrap();
            same(e_, format!("{label}.list"), r.list.len() == el.len(), format!("件数 rust={} js={}", r.list.len(), el.len()));
            for (x, y) in r.list.iter().zip(el) {
                same(e_, format!("{label}.list"), Some(x.index) == opt_idx(&y[0]) && Some(x.lv as u64) == y[2].as_u64(), format!("rust={x:?} js={y}"));
                cmp(e_, format!("{label}.list"), x.score, num(&y[1]));
            }
        }
        same(e_, format!("{name}.skill.target"), skill.target_pokemon_list == strings(&page["skill"]["targetPokemonList"]), String::new());

        // 仮定計算
        let fix_config = Config::from_value(&case["fixConfig"]).unwrap();
        let mut sim = BoxSimulator::new(fix_config);
        let result = sim.basic(&box_list, Some(&table)).unwrap();
        let exp = case["fix"].as_array().unwrap();
        same(e_, format!("{name}.fix"), result.len() == exp.len(), format!("件数 rust={} js={}", result.len(), exp.len()));
        for (i, (r, e)) in result.iter().zip(exp).enumerate() {
            let label = format!("{name}.fix[{i}]");
            let hits: Vec<String> = r
                .hit_check_list
                .iter()
                .map(|h| match h {
                    CheckHit::Pokemon { pokemon, .. } => format!("pokemon:{pokemon}"),
                    CheckHit::Food(f) => format!("food:{}", m.foods[*f].name),
                    CheckHit::Skill(s) => format!("skill:{}", m.skills[*s].name),
                })
                .collect();
            let act = (r.pokemon.base().name.as_str(), r.pokemon.box_index().unwrap() as u64, hits);
            let exp_t = (e["name"].as_str().unwrap(), e["boxIndex"].as_u64().unwrap(), strings(&e["hit"]));
            same(e_, label.clone(), act == exp_t, format!("rust={act:?} js={exp_t:?}"));
            cmp(e_, format!("{label}.lv"), r.pokemon.lv, num(&e["lv"]));
            cmp(e_, format!("{label}.energyPerDay"), r.pokemon.energy_per_day, num(&e["energyPerDay"]));
        }
    }
    eprintln!("checked={checked}");
    if !errors.is_empty() {
        panic!("{} 件不一致\n{}", errors.len(), errors.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }
}

#[test]
fn exp_require_info_matches_upstream() {
    use pokesle_core::config::CandyConfig;
    use pokesle_core::exp::calc_require_info;
    let golden = load("exp.json");
    let cases = golden.as_array().unwrap();
    for (i, case) in cases.iter().enumerate() {
        let mut candy_json = case["candy"].clone();
        candy_json["shard"] = Value::Null;
        let candy: CandyConfig = serde_json::from_value(candy_json).unwrap();
        let pokemon: BoxPokemon = serde_json::from_value(case["pokemon"].clone()).unwrap();
        let r = calc_require_info(&pokemon, master().nature(&pokemon.nature), &candy);
        let actual = [
            ("normalCandyNum", r.normal_candy_num),
            ("normalCandyShard", r.normal_candy_shard),
            ("boostCandyNum", r.boost_candy_num),
            ("boostCandyShard", r.boost_candy_shard),
            ("bestBoostCandyNum", r.best_boost_candy_num),
            ("bestBoostCandyShard", r.best_boost_candy_shard),
            ("bestNormalCandyNum", r.best_normal_candy_num),
            ("bestNormalCandyShard", r.best_normal_candy_shard),
        ];
        for (key, a) in actual {
            let e = expected_num(&case["result"][key]);
            let ok = match (a, e) {
                (Some(a), Some(e)) => close(a, e),
                (None, None) => true,
                _ => false,
            };
            assert!(ok, "case {i} {key}: actual {a:?} expected {e:?} ({})", case["pokemon"]);
        }
    }
}

#[test]
fn food_stock_matches_upstream() {
    use pokesle_core::config::{merge_object, default_config_value};
    use pokesle_core::data::CookingType;
    use pokesle_core::food_stock::{enable_cooking_list, max_candidate_num, run_worker, Input, Recipe};

    let m = master();
    let golden = load("food-stock.json");
    let types = ["カレー", "サラダ", "デザート"];
    for (i, case) in golden.as_array().unwrap().iter().enumerate() {
        let label = format!("case {i} {}", case["variant"]);

        // 設定 → 有効な料理・候補数の上限
        let mut value = default_config_value();
        merge_object(&mut value, &case["config"]);
        let config: Config = serde_json::from_value(value).unwrap();
        let list = enable_cooking_list(&config);
        let names: Vec<&str> = list.iter().map(|c| c.cooking().name.as_str()).collect();
        let expected_names: Vec<&str> = case["enableNames"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert_eq!(names, expected_names, "{label}: 有効な料理");
        assert_eq!(max_candidate_num(&config, &list) as f64, case["maxCandidateNum"].as_f64().unwrap(), "{label}: 候補数の上限");

        // ワーカー
        let inp = &case["input"];
        let weights = types.map(|t| inp["weights"][t].as_f64().unwrap());
        let cooking_list: Vec<Recipe> = inp["cookingList"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| Recipe {
                name: c["name"].as_str().unwrap().to_string(),
                cooking_type: CookingType::from_name(c["type"].as_str().unwrap()).unwrap(),
                food_num: c["foodNum"].as_f64().unwrap(),
                food_list: c["foodList"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (m.foods.iter().position(|x| x.name == f["name"].as_str().unwrap()).unwrap(), f["num"].as_f64().unwrap()))
                    .collect(),
                energy: c["energy"].as_f64().unwrap(),
            })
            .collect();
        let input = Input {
            bag_size: inp["bagSize"].as_f64().unwrap(),
            cooking_num: inp["cookingNum"].as_f64().unwrap(),
            candidate_num: inp["candidateNum"].as_f64().unwrap(),
            min_food_num: inp["minFoodNum"].as_f64().unwrap(),
            max_food_num: inp["maxFoodNum"].as_f64().unwrap(),
            weights,
            cooking_list,
            seed: inp["seed"].as_u64().unwrap() as u32,
        };
        let actual = run_worker(&input, &|_| {}).unwrap();
        let expected = &case["result"];
        let Some(actual) = actual else {
            assert!(expected.is_null(), "{label}: 本家は結果あり");
            continue;
        };
        assert!(!expected.is_null(), "{label}: 本家は結果なし");
        assert!(close(actual.score, expected["score"].as_f64().unwrap()), "{label}: score {} vs {}", actual.score, expected["score"]);
        assert_eq!(actual.total_food_num, expected["totalFoodNum"].as_f64().unwrap(), "{label}: totalFoodNum");
        for (fi, food) in m.foods.iter().enumerate() {
            assert_eq!(actual.food_num_list[fi], expected["foodNumMap"][&food.name].as_f64().unwrap(), "{label}: {}", food.name);
        }
        for (t, name) in types.iter().enumerate() {
            let e = &expected["plans"][*name];
            assert!(close(actual.plans[t].energy, e["energy"].as_f64().unwrap()), "{label}: {name} energy");
            let ecl = e["cookingList"].as_array().unwrap();
            assert_eq!(actual.plans[t].cooking_list.len(), ecl.len(), "{label}: {name} 料理数");
            for (a, e) in actual.plans[t].cooking_list.iter().zip(ecl) {
                assert_eq!(a.name, e["name"].as_str().unwrap(), "{label}: {name}");
                assert_eq!(a.num, e["num"].as_f64().unwrap(), "{label}: {name} {}", a.name);
                assert!(close(a.energy, e["energy"].as_f64().unwrap()), "{label}: {name} {} energy", a.name);
            }
        }
    }
}

#[test]
fn box_summary_matches_upstream() {
    use pokesle_core::box_summary::{berry_summary, food_summary, pokemon_summary, skill_summary, EvaluateType, Ranked};
    let fix = load("box-fix.json");
    let box_list: Vec<BoxPokemon> = serde_json::from_value(fix["box"].clone()).unwrap();
    let table = table_from_js(&fix["table"]);
    let golden = load("box-summary.json");
    let m = master();

    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0;
    let mut check = |label: String, actual: &Ranked, expected: &Value| {
        checked += 1;
        let a = actual.value;
        let e = expected_num(&expected["value"]);
        let ok = match (a, e) {
            (Some(a), Some(e)) => close(a, e),
            (None, None) => true,
            (Some(a), None) => a.is_nan(),
            (None, Some(e)) => e.is_nan(),
        };
        if !ok {
            errors.push(format!("{label}: rust={a:?} js={}", expected["value"]));
            return;
        }
        let act = actual.pokemon.as_ref().map(|p| (p.base().name.clone(), p.box_index().map(|i| i as u64)));
        let exp = expected["pokemon"].as_object().map(|o| (o["name"].as_str().unwrap().to_string(), o["index"].as_u64()));
        if act != exp {
            errors.push(format!("{label}: pokemon rust={act:?} js={exp:?}"));
        }
    };

    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let config = Config::from_value(&case["config"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let noop = |_: f64| {};

        // きのみ
        let berry = berry_summary(&config, &box_list, Some(&table), &noop).unwrap();
        let expected = case["berry"].as_array().unwrap();
        assert_eq!(berry.len(), expected.len());
        for (r, e) in berry.iter().zip(expected) {
            assert_eq!(r.name, e["name"].as_str().unwrap());
            for (i, c) in r.cols.iter().enumerate() {
                check(format!("{name} berry {} col{i}", r.name), c, &e["cols"][i]);
            }
        }

        // 食材
        let food = food_summary(&config, &box_list, Some(&table), &noop).unwrap();
        let expected = case["food"].as_array().unwrap();
        for (r, e) in food.iter().zip(expected) {
            assert_eq!(r.name, e["name"].as_str().unwrap());
            assert_eq!(r.require, e["require"].as_f64().unwrap(), "{name} food {} require", r.name);
            assert!(close(r.max, expected_num(&e["max"]).unwrap()), "{name} food {} max {} vs {}", r.name, r.max, e["max"]);
            let mp = r.max_pokemon.map(|i| m.pokemon[i].name.clone());
            assert_eq!(mp.as_deref(), e["maxPokemon"].as_str(), "{name} food {} maxPokemon", r.name);
            let fl: Vec<String> = serde_json::from_value(e["maxFoodList"].clone()).unwrap();
            assert_eq!(r.max_food_list, fl, "{name} food {} maxFoodList", r.name);
            for (i, c) in r.cols.iter().enumerate() {
                check(format!("{name} food {} col{i}", r.name), c, &e["cols"][i]);
            }
        }

        // スキル
        let skill = skill_summary(&config, &box_list, Some(&table), &noop).unwrap();
        let expected = case["skill"].as_array().unwrap();
        for (r, e) in skill.iter().zip(expected) {
            assert_eq!(r.name, e["name"].as_str().unwrap());
            assert!(close(r.max, expected_num(&e["max"]).unwrap()), "{name} skill {} max {} vs {}", r.name, r.max, e["max"]);
            assert_eq!(r.max_pokemon.as_deref(), e["maxPokemon"].as_str(), "{name} skill {} maxPokemon", r.name);
            for (i, c) in r.cols.iter().enumerate() {
                check(format!("{name} skill {} col{i}", r.name), c, &e["cols"][i]);
            }
        }

        // ポケモン
        for (key, each_food, ty) in [("all_energy", false, EvaluateType::Energy), ("food_specialty", true, EvaluateType::Specialty)] {
            let summary = pokemon_summary(&config, &box_list, Some(&table), each_food, ty, &noop).unwrap();
            let expected = case["pokemon"][key].as_array().unwrap();
            assert_eq!(summary.rows.len(), expected.len(), "{name} {key} 行数");
            for (r, e) in summary.rows.iter().zip(expected) {
                let label = format!("{name} {key} {}", m.pokemon[r.base].name);
                assert_eq!(m.pokemon[r.base].name, e["name"].as_str().unwrap(), "{label}");
                check(format!("{label} max"), &r.max, &e["max"]);
                for (lv, c) in &r.by_lv {
                    check(format!("{label} lv{lv}"), c, &e["byLv"][lv.to_string()]);
                }
            }
        }
    }
    eprintln!("checked={checked}");
    assert!(errors.is_empty(), "{} 件不一致\n{}", errors.len(), errors.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn data_view_matches_upstream() {
    use pokesle_core::data_view::{expect_simulate, food_rows, pokemon_derived, ExpectKind, ExpectParam};
    let golden = load("data-view.json");
    let m = master();
    let num = |v: &Value| expected_num(v).unwrap_or(f64::NAN);

    // ポケモンの表
    for e in golden["pokemon"].as_array().unwrap() {
        let p = m.pokemon(e["name"].as_str().unwrap()).unwrap();
        let d = pokemon_derived(p);
        assert!(close(d.ceil, num(&e["ceil"])), "{} ceil", p.name);
        assert!(close(d.skill_rate_with_ceil, num(&e["skillRate2"])), "{} skillRate2", p.name);
        assert!(close(d.skill_num, num(&e["skillNum"])), "{} skillNum", p.name);
    }

    // 食材の表
    let types = ["カレー", "サラダ", "デザート"];
    for case in golden["food"].as_array().unwrap() {
        let config = Config::from_value(&case["config"]).unwrap();
        let rows = food_rows(&config);
        for (r, e) in rows.iter().zip(case["rows"].as_array().unwrap()) {
            let name = &m.foods[r.food].name;
            assert_eq!(name, e["name"].as_str().unwrap());
            for (t, ty) in types.iter().enumerate() {
                assert!(close(r.require[t], num(&e[format!("require_{ty}")])), "{name} require {ty}");
                assert!(close(r.best_type_rate[t], num(&e[format!("bestTypeRate_{ty}")])), "{name} rate {ty}");
                assert!(close(r.max_cooking_energy[t], num(&e[format!("maxCookingEnergy_{ty}")])), "{name} cookingEnergy {ty}");
                assert!(close(r.max_energy[t], num(&e[format!("maxEnergy_{ty}")])), "{name} maxEnergy {ty}");
            }
            assert!(close(r.max_cooking_energy_average, num(&e["average"])), "{name} average");
            assert!(close(r.max_cooking_energy_geometric_mean, num(&e["geometricMean"])), "{name} geometricMean");
            assert!(close(r.max_energy_all, num(&e["maxEnergy"])), "{name} maxEnergy all");
        }
    }

    // 食材期待値・スキル期待値
    let config = Config::default_config();
    for case in golden["expect"].as_array().unwrap() {
        let kind = if case["kind"] == "food" { ExpectKind::Food } else { ExpectKind::Skill };
        let p = &case["param"];
        let param = ExpectParam {
            kind,
            lv: p["lv"].as_f64().unwrap(),
            nature: p["nature"].as_str().unwrap().to_string(),
            sub_skill: p["subSkill"].as_u64().unwrap() as u8,
            speed: p["speed"].as_f64().unwrap(),
            genki_full: p["genkiFull"].as_bool().unwrap(),
        };
        // 並びはワーカー分割で変わるので、(名前, 食材)で突き合わせる
        let key = |name: &str, foods: &[String]| format!("{name}/{}", foods.join(","));
        let mut result = expect_simulate(&config, &param).unwrap();
        result.sort_by_key(|r| key(&r.pokemon.base().name, &r.pokemon.box_pokemon.as_ref().unwrap().food_list));
        let mut rows: Vec<&Value> = case["rows"].as_array().unwrap().iter().collect();
        rows.sort_by_key(|e| {
            let foods: Vec<String> = serde_json::from_value(e["foodList"].clone()).unwrap();
            key(e["name"].as_str().unwrap(), &foods)
        });
        assert_eq!(result.len(), rows.len(), "{} 件数", case["kind"]);
        for (r, e) in result.iter().zip(rows) {
            let label = format!("{} {}", case["kind"], e["name"]);
            assert_eq!(r.pokemon.base().name, e["name"].as_str().unwrap(), "{label}");
            for (fi, f) in e["foods"].as_array().unwrap().iter().enumerate() {
                assert!(close(r.pokemon.food_count[fi], num(f)), "{label} food {fi}: {} vs {f}", r.pokemon.food_count[fi]);
            }
            assert!(close(r.pokemon.skill_rate, num(&e["skillRate"])), "{label} skillRate");
            assert!(close(r.pokemon.ceil_skill_rate, num(&e["ceilSkillRate"])), "{label} ceilSkillRate");
            assert!(close(r.pokemon.skill_ceil, num(&e["skillCeil"])), "{label} skillCeil");
            assert!(close(r.pokemon.skill_per_day, num(&e["skillPerDay"])), "{label} skillPerDay");
        }
    }
}

#[test]
fn tmp_evaluate_matches_upstream() {
    use pokesle_core::evaluate_table::EvalLv;
    use pokesle_core::team_sim::simulate_tmp_evaluate;
    let golden = load("tmp-evaluate.json");
    let num = |v: &Value| expected_num(v);
    for (ci, case) in golden.as_array().unwrap().iter().enumerate() {
        let mut value = pokesle_core::config::default_config_value();
        pokesle_core::config::merge_object(&mut value, &case["config"]);
        let config = Config::from_value(&value).unwrap();
        let pokemon: BoxPokemon = serde_json::from_value(case["pokemon"].clone()).unwrap();
        let r = simulate_tmp_evaluate(&config, &pokemon, &()).unwrap();
        let er = r.evaluate_result.expect("evaluate_result");
        let expected = case["evolutions"].as_object().unwrap();
        assert!(!expected.is_empty(), "case {ci}: 本家の結果が空");
        let mut names = er.names(EvalLv::Lv(30));
        names.sort();
        let mut exp_names: Vec<&String> = expected.keys().collect();
        exp_names.sort();
        assert_eq!(names.iter().collect::<Vec<_>>(), exp_names, "case {ci}: 進化先");
        for (name, by_lv) in expected {
            for lv in [30u32, 50, 60, 70, 80] {
                let e = &by_lv[lv.to_string()];
                let entry = er.get(EvalLv::Lv(lv), name).unwrap_or_else(|| panic!("case {ci} {name} Lv{lv}: 評価なし"));
                for key in ["energy", "berry", "food", "skill"] {
                    let a = entry.get(key).map(|i| i.rate);
                    let x = num(&e["rate"][key]);
                    assert!(
                        match (a, x) {
                            (Some(a), Some(x)) => close(a, x),
                            (None, None) => true,
                            _ => false,
                        },
                        "case {ci} {name} Lv{lv} {key}: rust={a:?} js={x:?}"
                    );
                }
                let d = entry.diagnostic.as_ref().expect("diagnostic");
                assert!(close(d.berry_energy, num(&e["berryEnergy"]).unwrap()), "case {ci} {name} Lv{lv} berryEnergy");
                assert!(close(d.skill_activation_num, num(&e["skillActivationNum"]).unwrap()), "case {ci} {name} Lv{lv} skillActivationNum");
                for (food, v) in e["foodNumMap"].as_object().unwrap() {
                    let a = d.food_num_map.iter().find(|(n, _)| n == food).map(|(_, v)| *v).unwrap_or(f64::NAN);
                    assert!(close(a, num(v).unwrap()), "case {ci} {name} Lv{lv} food {food}: {a} vs {v}");
                }
            }
        }
    }
}

#[test]
fn config_migrate_matches_upstream() {
    use pokesle_core::config_migrate::migrate_saved;
    let golden = load("config-migrate.json");
    for case in golden.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut input = case["input"].clone();
        migrate_saved(&mut input);
        // 変換結果は本家と完全に一致する(キーの並びは問わない)
        assert_eq!(input, case["output"], "{name}");
        // 変換後は設定として読み込める
        Config::from_saved(&case["input"]).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}
