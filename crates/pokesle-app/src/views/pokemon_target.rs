//! 対象ポケモンの指定(本家 components/pokemon-target-setting.vue)

use eframe::egui;
use pokesle_core::config::PokemonTarget;
use pokesle_core::data::master;
use pokesle_core::evaluate_setting::{matches_pokemon_target, target_base_pokemon, TARGET_SPECIALTY_LIST};

/// 全員 / 条件で指定 / ポケモンで指定
fn target_type(t: &PokemonTarget) -> &str {
    if let Some(ty) = &t.target_type {
        return ty;
    }
    if t.all {
        "all"
    } else if !t.pokemon_name_list.is_empty() {
        "pokemon"
    } else {
        "condition"
    }
}

fn set_target_type(t: &mut PokemonTarget, ty: &str) {
    t.target_type = Some(ty.to_string());
    t.all = ty == "all";
    if ty != "condition" {
        for s in TARGET_SPECIALTY_LIST {
            t.specialties.insert(s.to_string(), false);
        }
        t.skill_name_list.clear();
        t.friend_point = None;
    }
    if ty != "pokemon" {
        t.pokemon_name_list.clear();
    }
}

/// ボタンに出す説明
pub fn label(t: &PokemonTarget) -> String {
    match target_type(t) {
        "all" => "全員".into(),
        "pokemon" => format!("ポケモン：{}", t.pokemon_name_list.join("・")),
        _ => {
            let mut list: Vec<String> = TARGET_SPECIALTY_LIST
                .iter()
                .filter(|s| t.specialties.get(**s).copied().unwrap_or(false))
                .map(|s| format!("{s}とくい"))
                .collect();
            list.extend(t.skill_name_list.iter().cloned());
            if let Some(fp) = t.friend_point {
                list.push(format!("フレンドポイント{fp}以上"));
            }
            if list.is_empty() { "条件で指定（未選択）".into() } else { list.join("・") }
        }
    }
}

/// 最終進化のポケモン(名前順)
fn last_pokemon_list() -> Vec<usize> {
    let m = master();
    let mut list: Vec<usize> = m.pokemon.iter().filter(|p| p.is_last).map(|p| p.index).collect();
    list.sort_by(|&a, &b| m.pokemon[a].name.encode_utf16().cmp(m.pokemon[b].name.encode_utf16()));
    list
}

fn toggle(list: &mut Vec<String>, name: &str, on: bool) {
    let has = list.iter().any(|x| x == name);
    if on && !has {
        list.push(name.to_string());
    } else if !on && has {
        list.retain(|x| x != name);
    }
}

/// 対象の編集欄。変更があれば true
pub fn editor(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, t: &mut PokemonTarget) -> bool {
    let m = master();
    let mut changed = false;
    let id = egui::Id::new(id);
    ui.horizontal(|ui| {
        let current = target_type(t).to_string();
        for (ty, name) in [("all", "全員"), ("condition", "条件で指定"), ("pokemon", "ポケモンで指定")] {
            if ui.radio(current == ty, name).clicked() && current != ty {
                set_target_type(t, ty);
                changed = true;
            }
        }
    });
    ui.separator();

    match target_type(t) {
        "condition" => {
            ui.label(egui::RichText::new("とくい").strong());
            ui.horizontal_wrapped(|ui| {
                for s in TARGET_SPECIALTY_LIST {
                    let v = t.specialties.entry(s.to_string()).or_insert(false);
                    changed |= ui.checkbox(v, format!("{s}とくい")).changed();
                }
            });
            ui.label(egui::RichText::new("かつ スキル").strong());
            for cat in &m.skill_categories {
                let skills: Vec<_> = m.skills.iter().filter(|s| s.category == cat.id).collect();
                if skills.is_empty() {
                    continue;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("{}:", cat.name)).weak());
                    for s in skills {
                        let mut on = t.skill_name_list.contains(&s.name);
                        if ui.checkbox(&mut on, &s.name).changed() {
                            toggle(&mut t.skill_name_list, &s.name, on);
                            changed = true;
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("かつ 種ポケフレンドポイント").strong());
                let mut fps: Vec<f64> = m.pokemon.iter().filter(|p| p.is_last).filter_map(|p| target_base_pokemon(p).fp).collect();
                fps.sort_by(|a, b| a.total_cmp(b));
                fps.dedup();
                let text = t.friend_point.map(|x| format!("{x}以上")).unwrap_or_else(|| "指定なし".into());
                egui::ComboBox::from_id_salt(id.with("fp")).selected_text(text).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut t.friend_point, None, "指定なし").changed();
                    for fp in fps {
                        changed |= ui.selectable_value(&mut t.friend_point, Some(fp), format!("{fp}以上")).changed();
                    }
                });
            });
            ui.separator();
            let matched: Vec<&str> =
                last_pokemon_list().into_iter().filter(|&p| matches_pokemon_target(Some(t), &m.pokemon[p])).map(|p| m.pokemon[p].name.as_str()).collect();
            ui.label(egui::RichText::new(format!("対象ポケモン({}匹)", matched.len())).strong());
            egui::ScrollArea::vertical().id_salt(id.with("matched")).max_height(120.0).show(ui, |ui| {
                ui.label(if matched.is_empty() { "該当なし".to_string() } else { matched.join("、") });
            });
        }
        "pokemon" => {
            egui::ScrollArea::vertical().id_salt(id.with("pokemon")).max_height(360.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for p in last_pokemon_list() {
                        let name = &m.pokemon[p].name;
                        let mut on = t.pokemon_name_list.contains(name);
                        if ui.checkbox(&mut on, name).changed() {
                            toggle(&mut t.pokemon_name_list, name, on);
                            changed = true;
                        }
                    }
                });
            });
        }
        _ => {}
    }
    changed
}
