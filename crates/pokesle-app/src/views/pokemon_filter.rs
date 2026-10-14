//! 除外フィルタの編集(本家 components/filter/pokemon-filter-editor.vue)

use eframe::egui::{self, Color32};
use pokesle_core::config::{FilterCondition, FilterConfig};
use pokesle_core::data::master;
use pokesle_core::filter::{filter_pokemon, FILTER_TYPE_LIST};
use pokesle_core::sim_pokemon::BoxPokemon;
use serde_json::Value;

use super::style::{self, form_button, ButtonKind};

/// 条件の入力欄の種類(本家 PokemonFilter.TYPE_LIST の input)
fn input_kind(condition_type: &str) -> &'static str {
    match condition_type {
        "pokemon" => "pokemon",
        "pokemon_tree" => "last_pokemon",
        "lv_lte" | "lv_gte" => "lv",
        "skill" => "skill",
        _ => "",
    }
}

/// 追加しようとしている条件(本家 newCondition)
#[derive(Clone, Default)]
struct NewCondition {
    kind: usize,
    /// ポケモン名・スキル名
    name: Option<String>,
    /// Lv の入力欄の文字
    lv: String,
}

impl NewCondition {
    fn value(&self) -> Value {
        if input_kind(FILTER_TYPE_LIST[self.kind].0) == "lv" {
            match self.lv.trim().parse::<f64>() {
                Ok(n) if n.fract() == 0.0 && n.abs() < 1e15 => Value::from(n as i64),
                Ok(n) => serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        } else {
            self.name.clone().map(Value::String).unwrap_or(Value::Null)
        }
    }
}

/// ポケモン名の並び(本家 Pokemon.nameSortList)
fn name_sort_list() -> Vec<&'static str> {
    let mut list: Vec<&str> = master().pokemon.iter().map(|p| p.name.as_str()).collect();
    list.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    list
}

/// 除外したポケモンの数(設定ボタンのラベル用)
pub fn exclude_count(box_list: &[BoxPokemon], filter: &FilterConfig) -> usize {
    if !filter.enable {
        return 0;
    }
    filter_pokemon(box_list, filter).exclude_list.len()
}

/// 条件の有効・無効を切り替えるスイッチ(本家の svg)
fn switch(ui: &mut egui::Ui, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 10.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let gray = Color32::from_gray(0x88);
    let p = ui.painter();
    p.rect_filled(rect, 5.0, gray);
    if on {
        p.circle_filled(egui::pos2(rect.left() + rect.width() * 0.7, rect.center().y), 3.0, Color32::WHITE);
    } else {
        p.rect_filled(rect.shrink(1.0), 4.0, Color32::WHITE);
        p.circle_filled(egui::pos2(rect.left() + rect.width() * 0.3, rect.center().y), 3.0, gray);
    }
    response
}

/// ×・▲・▼ の小さなボタン
fn icon_button(ui: &mut egui::Ui, kind: char) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let gray = Color32::from_gray(if response.hovered() { 0x55 } else { 0x88 });
    let p = ui.painter();
    let r = rect.shrink(2.0);
    match kind {
        '×' => {
            let s = egui::Stroke::new(1.5, gray);
            p.line_segment([r.left_top(), r.right_bottom()], s);
            p.line_segment([r.left_bottom(), r.right_top()], s);
        }
        '▲' => {
            p.add(egui::Shape::convex_polygon(vec![egui::pos2(r.left(), r.bottom() - 2.0), egui::pos2(r.center().x, r.top() + 2.0), egui::pos2(r.right(), r.bottom() - 2.0)], gray, egui::Stroke::NONE));
        }
        _ => {
            p.add(egui::Shape::convex_polygon(vec![egui::pos2(r.left(), r.top() + 2.0), egui::pos2(r.right(), r.top() + 2.0), egui::pos2(r.center().x, r.bottom() - 2.0)], gray, egui::Stroke::NONE));
        }
    }
    response.clicked()
}

/// 本家 PokemonFilterEditor。変更があれば true
pub fn editor(ui: &mut egui::Ui, id: &str, filter: &mut FilterConfig, box_list: &[BoxPokemon]) -> bool {
    // 本家と同じく幅 500px に収める
    ui.scope(|ui| {
        ui.set_max_width(500.0);
        editor_body(ui, id, filter, box_list)
    })
    .inner
}

fn editor_body(ui: &mut egui::Ui, id: &str, filter: &mut FilterConfig, box_list: &[BoxPokemon]) -> bool {
    let m = master();
    let mut changed = false;
    changed |= ui.checkbox(&mut filter.enable, "除外フィルタを有効にする").changed();
    ui.add_space(10.0);

    let result = filter_pokemon(box_list, filter);
    let mut action: Option<(usize, char)> = None;
    let mut toggle: Option<usize> = None;

    egui::Frame::new().stroke(egui::Stroke::new(1.0, super::style::line_color())).show(ui, |ui| {
        ui.set_width(498.0);
        ui.set_height(300.0);
        egui::ScrollArea::vertical().id_salt(("filter_list", id)).auto_shrink([false, false]).show(ui, |ui| {
            let row = |ui: &mut egui::Ui, fill: Color32, content: &mut dyn FnMut(&mut egui::Ui), count: Option<usize>, tools: &mut dyn FnMut(&mut egui::Ui)| {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 5)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // 隙間は add_space で入れるので、幅の合計がちょうど一杯になる
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let tools_w = 80.0;
                        let count_w = 40.0;
                        let cond_w = (ui.available_width() - tools_w - count_w - 20.0).max(100.0);
                        egui::Frame::new().fill(fill).corner_radius(10).inner_margin(egui::Margin::symmetric(10, 2)).show(ui, |ui| {
                            ui.set_width(cond_w - 20.0);
                            ui.set_min_height(20.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                content(ui);
                            });
                        });
                        ui.add_space(10.0);
                        ui.allocate_ui_with_layout(egui::vec2(count_w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_width(count_w);
                            if let Some(n) = count {
                                ui.label(egui::RichText::new(format!("{n}件")).strong());
                            }
                        });
                        ui.add_space(10.0);
                        ui.allocate_ui_with_layout(egui::vec2(tools_w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_width(tools_w);
                            ui.spacing_mut().item_spacing.x = 5.0;
                            tools(ui);
                        });
                    });
                });
            };

            row(ui, super::style::pick(0xeeeeee, 0x2c3342), &mut |ui| {
                ui.label("ボックスすべて");
            }, Some(box_list.len()), &mut |_| {});

            for (i, step) in result.step_list.iter().enumerate() {
                let disabled = filter.condition_list[i].disabled;
                let fill = if step.mode { super::style::pick(0xeeffdd, 0x253a2b) } else { super::style::pick(0xffeeee, 0x3e2a2d) };
                row(ui, fill, &mut |ui| {
                    if switch(ui, !disabled).clicked() {
                        toggle = Some(i);
                    }
                    let text = egui::RichText::new(&step.name);
                    ui.label(if disabled { text.strikethrough() } else { text });
                }, step.count, &mut |ui| {
                    for kind in ['×', '▲', '▼'] {
                        if icon_button(ui, kind) {
                            action = Some((i, kind));
                        }
                    }
                });
            }
        });
    });

    let list = &mut filter.condition_list;
    if let Some(i) = toggle {
        list[i].disabled = !list[i].disabled;
        changed = true;
    }
    match action {
        Some((i, '×')) => {
            list.remove(i);
            changed = true;
        }
        // 本家 Array.swap: 範囲外なら何もしない
        Some((i, '▲')) if i >= 1 => {
            list.swap(i, i - 1);
            changed = true;
        }
        Some((i, '▼')) if i + 1 < list.len() => {
            list.swap(i, i + 1);
            changed = true;
        }
        _ => {}
    }

    // 条件の追加
    ui.add_space(10.0);
    let state_id = egui::Id::new(("filter_new_condition", id));
    let mut new = ui.data(|d| d.get_temp::<NewCondition>(state_id)).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let before = new.kind;
        egui::ComboBox::from_id_salt(("filter_type", id)).selected_text(FILTER_TYPE_LIST[new.kind].1).show_ui(ui, |ui| {
            for (k, (_, name)) in FILTER_TYPE_LIST.iter().enumerate() {
                ui.selectable_value(&mut new.kind, k, *name);
            }
        });
        // 種類を変えたら値を消す(本家の watch)
        if new.kind != before {
            new.name = None;
            new.lv.clear();
        }

        let mut name_select = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui, &mut Option<String>)| {
            egui::ComboBox::from_id_salt(("filter_value", id))
                .width(200.0)
                .height(400.0)
                .selected_text(new.name.clone().unwrap_or_default())
                .show_ui(ui, |ui| add(ui, &mut new.name));
        };
        match input_kind(FILTER_TYPE_LIST[new.kind].0) {
            "pokemon" => name_select(ui, &mut |ui, v| {
                for name in name_sort_list() {
                    ui.selectable_value(v, Some(name.to_string()), name);
                }
            }),
            "last_pokemon" => name_select(ui, &mut |ui, v| {
                for name in name_sort_list().into_iter().filter(|n| m.pokemon(n).is_some_and(|p| p.is_last)) {
                    ui.selectable_value(v, Some(name.to_string()), name);
                }
            }),
            "skill" => name_select(ui, &mut |ui, v| {
                for cat in &m.skill_categories {
                    let skills: Vec<_> = m.skills.iter().filter(|s| s.category == cat.id).collect();
                    if skills.is_empty() {
                        continue;
                    }
                    ui.label(egui::RichText::new(&cat.name).strong().color(style::th_color()));
                    for s in skills {
                        ui.selectable_value(v, Some(s.name.clone()), format!("  {}", s.name));
                    }
                }
            }),
            "lv" => {
                ui.add(egui::TextEdit::singleline(&mut new.lv).desired_width(50.0));
                ui.label("Lv");
            }
            _ => {}
        }

        let value = new.value();
        let enable_add = !value.is_null() && filter.enable;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(enable_add, |ui| {
                for (mode, text) in [(false, "除外"), (true, "追加")] {
                    if form_button(ui, text, ButtonKind::Normal).clicked() && enable_add {
                        filter.condition_list.push(FilterCondition {
                            condition_type: FILTER_TYPE_LIST[new.kind].0.to_string(),
                            value: value.clone(),
                            mode,
                            disabled: false,
                            extra: Default::default(),
                        });
                        changed = true;
                    }
                }
            });
        });
    });
    ui.data_mut(|d| d.insert_temp(state_id, new));
    changed
}
