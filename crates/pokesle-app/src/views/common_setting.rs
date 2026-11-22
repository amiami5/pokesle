//! 共通シミュレーション設定のボタン列(本家 components/common-setting.vue)
//!
//! ボックス画面・チーム編成画面の上部に並ぶ「今週：…」「イベントボーナス：…」などのボタンと、
//! 押したときに開く設定ポップアップ。項目と並びは本家に合わせている。

use eframe::egui;
use pokesle_core::config::{Config, EventBonus};
use pokesle_core::cooking::evaluate_lv_list;
use pokesle_core::data::{master, Berry, CookingType};
use pokesle_core::sim_pokemon::BoxPokemon;

use super::pokemon_filter;
use super::style::{self, form_button, js_num, number, percent_number, small, th, ButtonKind, Label};
use super::{fmt_fixed, food_icon};

/// 本家 Berry.typeList の並び
const TYPE_LIST: [&str; 18] = [
    "ノーマル", "ほのお", "みず", "でんき", "くさ", "こおり", "かくとう", "どく", "じめん", "ひこう", "エスパー", "むし", "いわ", "ゴースト",
    "ドラゴン", "あく", "はがね", "フェアリー",
];

const SPECIALTIES: [&str; 3] = ["きのみ", "食材", "スキル"];

fn berry_list_by_color() -> Vec<&'static Berry> {
    let mut list: Vec<&Berry> = master().berries.iter().collect();
    list.sort_by_key(|b| b.color_order);
    list
}

fn type_berry(ty: &str) -> Option<&'static Berry> {
    master().berries.iter().find(|b| b.type_name == ty)
}

fn grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([16.0, 10.0]).min_col_width(80.0).show(ui, add);
}

/// 本家 CommonSetting。呼び出し側の横並び(horizontal_wrapped)の中で呼ぶ。変更があれば true。
/// `box_list` は育成仮定の除外フィルタの件数表示に使う
pub fn ui(ui: &mut egui::Ui, config: &mut Config, box_list: &[BoxPokemon]) -> bool {
    let mut changed = false;
    changed |= field_setting(ui, config);
    changed |= event_bonus_setting(ui, config);
    changed |= berry_setting(ui, config);
    changed |= skill_setting(ui, config);
    changed |= bag_over_setting(ui, config);
    changed |= fix_setting(ui, config, box_list);
    changed |= select_setting(ui, config);
    changed |= cooking_setting(ui, config);
    changed |= expect_setting(ui, config);
    changed |= other_setting(ui, config);
    changed
}

// ---- フィールド ----

fn field_label(c: &Config) -> Label {
    let m = master();
    let s = &c.simulation;
    let mut l: Label = vec![(format!("今週：{}", s.field), false)];
    if s.field == "ワカクサ本島" {
        let b: Vec<String> = (0..3).map(|i| s.berry_list.get(i).filter(|x| !x.is_empty()).cloned().unwrap_or_else(|| "?".into())).collect();
        l.push((format!(" ({})", b.join(" ")), false));
    }
    l.push((format!(" FB:{}", js_num(s.field_bonus)), false));
    if m.field(&s.field).is_some_and(|f| f.ex) {
        match s.field_ex as i32 {
            1 => l.push((" EXきのみ".into(), true)),
            2 => l.push((" EX食材".into(), true)),
            3 => l.push((" EXスキル".into(), true)),
            _ => {}
        }
    }
    l.push((format!(" {}", s.cooking_type.clone().filter(|x| !x.is_empty()).unwrap_or_else(|| "すべて".into())), false));
    if s.cooking_weight != 1.0 {
        l.push((format!("(x{})", js_num(s.cooking_weight)), true));
    }
    if s.camp_ticket {
        l.push((" キャンチケ".into(), true));
    }
    if s.genki_full {
        l.push((" げんき100%".into(), true));
    }
    l
}

fn field_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let m = master();
    let mut changed = false;
    let label = field_label(c);
    style::setting(ui, "field", "フィールド", &label, false, |ui| {
        let s = &mut c.simulation;
        grid(ui, "field_grid", |ui| {
            th(ui, "フィールド");
            ui.vertical(|ui| {
                for ex in [false, true] {
                    egui::Grid::new(("field_list", ex)).num_columns(3).spacing([16.0, 5.0]).show(ui, |ui| {
                        for (i, f) in m.fields.iter().filter(|f| f.ex == ex).enumerate() {
                            if ui.radio(s.field == f.name, &f.name).clicked() && s.field != f.name {
                                s.field = f.name.clone();
                                changed = true;
                            }
                            if i % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
                    ui.add_space(5.0);
                }
            });
            ui.end_row();

            // フィールドを変えたら、選べないきのみを先頭の候補に直す(本家の watch)
            let field = m.field(&s.field);
            if changed && let Some(opts) = field.and_then(|f| f.berry_option_list.as_ref()) {
                s.berry_list.resize(3, String::new());
                for i in 0..3 {
                    if let Some(o) = opts.get(i)
                        && !o.contains(&s.berry_list[i])
                    {
                        s.berry_list[i] = o.first().cloned().unwrap_or_default();
                    }
                }
            }

            th(ui, "きのみ");
            ui.horizontal_top(|ui| {
                s.berry_list.resize(3, String::new());
                for i in 0..3 {
                    ui.vertical(|ui| {
                        let main = if field.is_some_and(|f| f.ex) && i == 0 { "(メイン)" } else { "" };
                        ui.strong(format!("きのみ{}{main}", i + 1));
                        for berry in berry_list_by_color() {
                            let fixed = field.and_then(|f| f.berry_list.as_ref()).map(|l| l.get(i) == Some(&berry.name));
                            let disabled = field.and_then(|f| f.berry_option_list.as_ref()).is_some_and(|o| !o.get(i).is_some_and(|l| l.contains(&berry.name)))
                                || fixed == Some(false);
                            let checked = fixed.unwrap_or(s.berry_list[i] == berry.name);
                            let clicked = ui
                                .horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.add_enabled_ui(!disabled, |ui| {
                                        food_icon::berry_icon(ui, berry, 20.0);
                                        ui.add(egui::RadioButton::new(checked, &berry.name)).clicked()
                                    })
                                    .inner
                                })
                                .inner;
                            if clicked && !checked {
                                s.berry_list[i] = berry.name.clone();
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(12.0);
                }
            });
            ui.end_row();

            th(ui, "EX設定");
            ui.vertical(|ui| {
                let ex = field.is_some_and(|f| f.ex);
                for (v, l) in [(1.0, "きのみエナジー x2.4"), (2.0, "食材+1 / +2"), (3.0, "スキル x1.25")] {
                    if ui.add_enabled(ex, egui::RadioButton::new(s.field_ex == v, l)).clicked() {
                        s.field_ex = v;
                        changed = true;
                    }
                }
            });
            ui.end_row();

            th(ui, "フィールドボーナス");
            ui.horizontal(|ui| {
                changed |= number(ui, &mut s.field_bonus, 1.0, 40.0);
                ui.label("%");
            });
            ui.end_row();

            th(ui, "料理の種類");
            let current = s.cooking_type.clone().unwrap_or_default();
            let names = [("カレー", "カレー・シチュー"), ("サラダ", "サラダ"), ("デザート", "デザート・ドリンク"), ("", "なんでも！")];
            let text = names.iter().find(|x| x.0 == current).map(|x| x.1).unwrap_or("");
            egui::ComboBox::from_id_salt("cooking_type").selected_text(text).show_ui(ui, |ui| {
                for (v, l) in names {
                    if ui.selectable_label(current == v, l).clicked() {
                        s.cooking_type = Some(v.to_string());
                        changed = true;
                    }
                }
            });
            ui.end_row();

            th(ui, "料理評価倍率");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    changed |= number(ui, &mut s.cooking_weight, 0.1, 80.0);
                    ui.label("倍");
                });
                small(
                    ui,
                    "イベント等で料理エナジーにボーナスがかかっている場合に設定してください。\nまた、イベント週以外でもきのみエナジー度外視でレシピレベルを育てる場合などはこの値を大きくすると、料理を重視したシミュレーションが出来ます。",
                    400.0,
                );
            });
            ui.end_row();

            th(ui, "キャンプチケット");
            changed |= ui.checkbox(&mut s.camp_ticket, "使う").changed();
            ui.end_row();

            th(ui, "げんき");
            ui.vertical(|ui| {
                changed |= ui.checkbox(&mut s.genki_full, "常に100%として計算").changed();
                small(ui, "げんきマクラを使用する週や、笛使用時の編成を検討する場合に使用してください。", 400.0);
            });
            ui.end_row();
        });
    });
    changed
}

// ---- イベントボーナス ----

fn all_checked(ev: &EventBonus) -> bool {
    !ev.target.types.values().any(|x| !x) && !ev.target.specialties.values().any(|x| !x)
}

fn event_bonus_label(c: &Config) -> Label {
    let s = &c.simulation;
    let mut parts: Vec<String> = s
        .event_bonus_list
        .iter()
        .filter_map(|ev| {
            let target = if all_checked(ev) {
                "全員".to_string()
            } else {
                SPECIALTIES
                    .iter()
                    .filter(|x| ev.target.specialties.get(**x).copied().unwrap_or(false))
                    .map(|x| x.to_string())
                    .chain(TYPE_LIST.iter().filter(|t| ev.target.types.get(**t).copied().unwrap_or(false)).map(|t| t.to_string()))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let effect = [
                (ev.berry != 0.0).then(|| format!("きのみ+{}", js_num(ev.berry))),
                (ev.food != 0.0).then(|| format!("食材+{}", js_num(ev.food))),
                (ev.skill_rate != 1.0).then(|| format!("スキル確率x{}", js_num(ev.skill_rate))),
                (ev.skill_lv != 0.0).then(|| format!("スキルレベル+{}", js_num(ev.skill_lv))),
                (ev.bag != 0.0).then(|| format!("所持数+{}", js_num(ev.bag))),
                (ev.bag_rate != 1.0).then(|| format!("所持数x{}", js_num(ev.bag_rate))),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            (!effect.is_empty()).then(|| format!("{target}({effect})"))
        })
        .collect();
    if s.event_bonus.skill.berry_burst != 1.0 {
        parts.push(format!("きのみバーストx{}", js_num(s.event_bonus.skill.berry_burst)));
    }
    if s.event_bonus.skill.food_get != 1.0 {
        parts.push(format!("食材系スキルx{}", js_num(s.event_bonus.skill.food_get)));
    }
    if let Some(food) = s.event_bonus.food.as_deref().filter(|f| !f.is_empty()) {
        parts.push(format!("{food}ボーナス"));
    }
    if s.event_bonus.pot.add != 0.0 {
        parts.push(format!("鍋+{}", js_num(s.event_bonus.pot.add)));
    }
    if s.event_bonus.pot.rate != 1.0 {
        parts.push(format!("鍋x{}", js_num(s.event_bonus.pot.rate)));
    }
    let mut l: Label = vec![("イベントボーナス：".into(), false)];
    if parts.is_empty() {
        l.push(("なし".into(), false));
    } else {
        l.push((parts.join(" "), true));
    }
    l
}

fn event_bonus_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let mut changed = false;
    let label = event_bonus_label(c);
    style::setting(ui, "event_bonus", "イベントボーナス", &label, false, |ui| {
        let s = &mut c.simulation;
        let mut delete = None;
        for (index, ev) in s.event_bonus_list.iter_mut().enumerate() {
            egui::Frame::new().fill(style::surface()).stroke(egui::Stroke::new(1.0, style::line_color())).corner_radius(6).inner_margin(8).show(ui, |ui| {
                egui::Grid::new(("event_bonus", index)).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    th(ui, "対象");
                    ui.vertical(|ui| {
                        let mut all = all_checked(ev);
                        if ui.checkbox(&mut all, "全て選択").changed() {
                            for v in ev.target.specialties.values_mut().chain(ev.target.types.values_mut()) {
                                *v = all;
                            }
                            changed = true;
                        }
                        ui.add_space(6.0);
                        egui::Grid::new(("event_bonus_target", index)).num_columns(3).min_col_width(150.0).spacing([8.0, 6.0]).show(ui, |ui| {
                            let mut n = 0;
                            for sp in SPECIALTIES {
                                let v = ev.target.specialties.entry(sp.to_string()).or_insert(false);
                                changed |= ui.checkbox(v, format!("{sp}とくい")).changed();
                                n += 1;
                            }
                            ui.end_row();
                            for ty in TYPE_LIST {
                                let v = ev.target.types.entry(ty.to_string()).or_insert(false);
                                let name = type_berry(ty).map(|b| b.name.as_str()).unwrap_or("");
                                changed |= ui.checkbox(v, format!("{ty}({name})")).changed();
                                n += 1;
                                if n % 3 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                    ui.end_row();
                    let mut row = |ui: &mut egui::Ui, title: &str, v: &mut f64, speed: f64, unit: &str| {
                        th(ui, title);
                        ui.horizontal(|ui| {
                            changed |= number(ui, v, speed, 60.0);
                            ui.label(unit);
                        });
                        ui.end_row();
                    };
                    row(ui, "きのみ", &mut ev.berry, 1.0, "個追加");
                    row(ui, "食材", &mut ev.food, 1.0, "個追加");
                    row(ui, "スキル倍率", &mut ev.skill_rate, 0.1, "倍");
                    row(ui, "スキルレベル", &mut ev.skill_lv, 1.0, "Lv追加");
                    th(ui, "所持数");
                    ui.horizontal(|ui| {
                        changed |= number(ui, &mut ev.bag, 1.0, 60.0);
                        ui.label("個追加");
                        ui.add_space(12.0);
                        changed |= number(ui, &mut ev.bag_rate, 0.1, 60.0);
                        ui.label("倍");
                    });
                    ui.end_row();
                });
                if form_button(ui, "このボーナスを削除", ButtonKind::Important).clicked() {
                    delete = Some(index);
                }
            });
            ui.add_space(10.0);
        }
        if let Some(i) = delete {
            s.event_bonus_list.remove(i);
            changed = true;
        }
        if form_button(ui, "イベントボーナスを追加", ButtonKind::Normal).clicked() {
            s.event_bonus_list.push(EventBonus::default());
            changed = true;
        }
        ui.add_space(10.0);
        grid(ui, "event_bonus_skill", |ui| {
            th(ui, "スキル");
            ui.vertical(|ui| {
                style::alert(ui, "スキルの設定は適用タイプによらず適用されます");
                ui.add_space(5.0);
                egui::Grid::new("event_bonus_skill_list").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                    ui.label("きのみバースト(みかづきのいのり含む)");
                    ui.horizontal(|ui| {
                        changed |= number(ui, &mut s.event_bonus.skill.berry_burst, 0.1, 60.0);
                        ui.label("倍");
                    });
                    ui.end_row();
                    ui.label("食材系スキル");
                    ui.horizontal(|ui| {
                        changed |= number(ui, &mut s.event_bonus.skill.food_get, 0.1, 60.0);
                        ui.label("倍");
                    });
                    ui.end_row();
                });
            });
            ui.end_row();

            th(ui, "食材ボーナス");
            ui.vertical(|ui| {
                let current = s.event_bonus.food.as_deref().filter(|f| !f.is_empty()).unwrap_or("未選択");
                ui.label(format!("対象食材：{current}"));
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(500.0);
                    ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                    for food in &master().foods {
                        let selected = s.event_bonus.food.as_deref() == Some(food.name.as_str());
                        if food_icon::food_choice(ui, &food.name, selected).clicked() {
                            s.event_bonus.food = if selected { None } else { Some(food.name.clone()) };
                            changed = true;
                        }
                    }
                });
                small(
                    ui,
                    "食材とくい・オールとくいは食材を拾うたびに対象食材を1個追加で獲得します。\n対象食材を含むレシピと、レシピ外の追加食材として投入した対象食材のエナジーが1.5倍になります。",
                    500.0,
                );
            });
            ui.end_row();

            th(ui, "鍋拡張");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    changed |= number(ui, &mut s.event_bonus.pot.add, 1.0, 60.0);
                    ui.label("個追加");
                    ui.add_space(12.0);
                    changed |= number(ui, &mut s.event_bonus.pot.rate, 0.1, 60.0);
                    ui.label("倍");
                });
                small(ui, "適用タイプによらずなべの大きさに適用されます。", 500.0);
            });
            ui.end_row();
        });
        if form_button(ui, "リセット", ButtonKind::Normal).clicked() {
            s.event_bonus_list.clear();
            s.event_bonus.skill.berry_burst = 1.0;
            s.event_bonus.skill.food_get = 1.0;
            s.event_bonus.food = None;
            s.event_bonus.pot = Default::default();
            changed = true;
        }
    });
    changed
}

// ---- きのみ設定 ----

fn berry_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let mut changed = false;
    let rate = |c: &Config, name: &str| c.simulation.berry_energy_rate.get(name).copied().unwrap_or(1.0);
    let customized: Vec<String> =
        berry_list_by_color().iter().filter(|b| rate(c, &b.name) != 1.0).map(|b| format!("{} {}%", b.name, fmt_fixed(rate(c, &b.name) * 100.0, 1))).collect();
    let mut label: Label = vec![("きのみ設定".into(), false)];
    if !customized.is_empty() {
        label.push(("：".into(), false));
        label.push((customized.join("、"), true));
    }
    style::setting(ui, "berry", "きのみ設定", &label, false, |ui| {
        egui::Grid::new("berry_rate").num_columns(6).spacing([12.0, 8.0]).show(ui, |ui| {
            for (i, berry) in berry_list_by_color().into_iter().enumerate() {
                ui.horizontal(|ui| {
                    food_icon::berry_icon(ui, berry, 20.0);
                    ui.label(&berry.name);
                });
                ui.horizontal(|ui| {
                    let v = c.simulation.berry_energy_rate.entry(berry.name.clone()).or_insert(1.0);
                    changed |= percent_number(ui, v, 60.0);
                    ui.label("%");
                });
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(10.0);
        if form_button(ui, "リセット", ButtonKind::Normal).clicked() {
            for b in &master().berries {
                c.simulation.berry_energy_rate.insert(b.name.clone(), 1.0);
            }
            changed = true;
        }
    });
    changed
}

// ---- スキル設定 ----

const FOOD_GET_HELP: &str = "■食材のまま評価\n単純に獲得する食材量の期待値で計算します。\n実際は期待値通りになることは稀であり、この設定でシミュレーションして出てきた料理は作れないことが多いので注意です。\n\n\
■今日のエナジーとして評価\n食材を最も良い料理に活用した場合のエナジー期待値で計算します。\n\n\
■翌日のエナジーとして評価\n食材を最も良い料理に活用した場合のエナジー期待値で計算しますが、明日のエナジーとして計算します。\n今日の睡眠にはエナジーが乗らず、明日以降の睡眠に乗るエナジーとして計算するので、1日分減らした評価で計算します。\n\
月曜の場合：6/7倍\n火曜の場合：5/6倍\n水曜の場合：4/5倍\n木曜の場合：3/4倍\n金曜の場合：2/3倍\n土曜の場合：1/2倍\n日曜の場合：7倍 (日曜の稼ぎは日曜の睡眠の1日分にしか乗りませんが、月曜に食材を回すことで7日分のエナジーになるため)";

fn skill_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let m = master();
    let mut changed = false;
    let rate = |c: &Config, name: &str| c.simulation.skill_rate.get(name).copied().unwrap_or(1.0);
    let mut label: Label = vec![("スキル設定".into(), false)];
    let custom: Vec<String> = m.skills.iter().filter(|s| rate(c, &s.name) != 1.0).map(|s| format!("{}({}%)", s.name, js_num(rate(c, &s.name) * 100.0))).collect();
    if !custom.is_empty() {
        label.push(("：".into(), false));
        label.push((custom.join(" "), true));
    }
    style::setting(ui, "skill", "スキル設定", &label, false, |ui| {
        egui::CollapsingHeader::new("個別スキル設定").default_open(true).show(ui, |ui| {
            if form_button(ui, "全て100%にリセット", ButtonKind::Normal).clicked() {
                for s in &m.skills {
                    c.simulation.skill_rate.insert(s.name.clone(), 1.0);
                }
                changed = true;
            }
            ui.add_space(10.0);
            for cat in &m.skill_categories {
                let skills: Vec<_> = m.skills.iter().filter(|s| s.category == cat.id).collect();
                if skills.is_empty() {
                    continue;
                }
                style::section_title(ui, &cat.name);
                egui::Grid::new(("skill_rate", &cat.id)).num_columns(6).spacing([12.0, 6.0]).show(ui, |ui| {
                    for (i, s) in skills.into_iter().enumerate() {
                        ui.label(&s.name);
                        ui.horizontal(|ui| {
                            let v = c.simulation.skill_rate.entry(s.name.clone()).or_insert(1.0);
                            changed |= percent_number(ui, v, 60.0);
                            ui.label("%");
                        });
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(6.0);
            }
        });
        ui.add_space(10.0);
        egui::CollapsingHeader::new("共通設定").default_open(true).show(ui, |ui| {
            let t = &mut c.team_simulation;
            egui::Grid::new("skill_common").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("食材ゲット・食材アシスト評価方法");
                    ui.label(egui::RichText::new("?").strong().color(style::primary())).on_hover_text(FOOD_GET_HELP);
                });
                ui.vertical(|ui| {
                    for (v, l) in [(1.0, "食材のまま評価"), (2.0, "今日のエナジーとして評価"), (3.0, "翌日のエナジーとして評価")] {
                        changed |= ui.radio_value(&mut t.food_get_evaluate_type, v, l).changed();
                    }
                });
                ui.end_row();
                ui.label("食材ゲット評価率");
                ui.horizontal(|ui| {
                    changed |= percent_number(ui, &mut t.food_get_evaluate_rate, 60.0);
                    ui.label("%");
                });
                ui.end_row();
            });
        });
    });
    changed
}

// ---- いつ育運用 ----

fn bag_over_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let mut changed = false;
    let label = style::plain(format!("いつ育運用: {}", if c.simulation.bag_over_operation { "する" } else { "しない" }));
    style::setting(ui, "bag_over", "いつ育運用", &label, false, |ui| {
        grid(ui, "bag_over_grid", |ui| {
            th(ui, "いつ育育成");
            ui.vertical(|ui| {
                changed |= ui.checkbox(&mut c.simulation.bag_over_operation, "する").changed();
                small(ui, "きのみタイプ/きのみの数S持ちのみ対象です", 200.0);
            });
            ui.end_row();
        });
    });
    changed
}

// ---- 育成仮定 ----

fn fix_label(c: &Config) -> Label {
    let s = &c.simulation;
    let mut text = format!("育成仮定: {}", if s.fix { "する" } else { "しない" });
    if s.fix {
        let mut parts: Vec<String> = Vec::new();
        if s.fix_check_list {
            parts.push("チェックリスト該当".into());
        }
        if s.fix_border.is_some() || s.fix_border_specialty.is_some() {
            let f = |x: Option<f64>| x.map(js_num).unwrap_or_else(|| "-".into());
            parts.push(format!("厳選{}%/{}%以上", f(s.fix_border), f(s.fix_border_specialty)));
        }
        let lv = s.fix_lv.map(js_num).unwrap_or_default();
        let evolve = if s.fix_evolve { " 進化" } else { "" };
        match s.fix_resource_mode as i32 {
            0 => parts.push(format!("{lv}Lv{evolve}")),
            1 => parts.push("リソース使用(制限なし)".into()),
            _ => parts.push(format!("リソース使用({lv}Lv{evolve})")),
        }
        if s.fix_sub_skill_seed {
            parts.push("銀種".into());
        }
        if s.fix_skill_seed == 1.0 {
            parts.push("金種".into());
        }
        if s.fix_skill_seed == 2.0 {
            parts.push("金種最大".into());
        }
        if s.fix_sleep_ribbon {
            parts.push("おやすみリボン".into());
        }
        text.push_str(&format!("( {} )", parts.join(" ")));
    }
    style::plain(text)
}

fn fix_setting(ui: &mut egui::Ui, c: &mut Config, box_list: &[BoxPokemon]) -> bool {
    let mut changed = false;
    let label = fix_label(c);
    style::setting(ui, "fix", "育成仮定", &label, false, |ui| {
        changed |= resource_popup(ui.ctx(), c);
        let s = &mut c.simulation;
        grid(ui, "fix_grid", |ui| {
            th(ui, "育成仮定");
            changed |= ui.checkbox(&mut s.fix, "する").changed();
            ui.end_row();

            let fix = s.fix;
            th(ui, "仮定条件");
            ui.vertical(|ui| {
                ui.add_enabled_ui(fix, |ui| {
                    changed |= ui.checkbox(&mut s.fix_check_list, "厳選チェックリストに該当").changed();
                    ui.horizontal(|ui| {
                        ui.label("エナジー厳選度");
                        changed |= opt_number(ui, "fix_border", &mut s.fix_border, true);
                        ui.label("%以上のみ");
                    });
                    ui.horizontal(|ui| {
                        ui.label("とくい厳選度");
                        changed |= opt_number(ui, "fix_border_specialty", &mut s.fix_border_specialty, true);
                        ui.label("%以上のみ");
                    });
                });
                // 本家では育成仮定が「しない」でも押せる
                ui.add_space(3.0);
                let n = pokemon_filter::exclude_count(box_list, &s.fix_filter);
                let label = style::plain(if n == 0 { "除外なし".to_string() } else { format!("除外：{n}匹") });
                style::setting(ui, "fix_filter", "除外フィルタ", &label, false, |ui| {
                    changed |= pokemon_filter::editor(ui, "fix_filter", &mut s.fix_filter, box_list);
                });
            });
            ui.end_row();

            th(ui, "育成制限");
            ui.vertical(|ui| {
                for (v, l) in [
                    (0.0, "指定Lv・進化まで(アメ・ゆめかけを考慮しない)"),
                    (1.0, "アメ・ゆめかけを使えるだけ使う"),
                    (2.0, "指定Lv・進化までアメ・ゆめかけを使えるだけ使う"),
                ] {
                    changed |= ui.radio_value(&mut s.fix_resource_mode, v, l).changed();
                }
                if form_button(ui, "アメ・ゆめのかけら管理", ButtonKind::Normal).clicked() {
                    style::open_popup(ui.ctx(), "resource");
                }
            });
            ui.end_row();

            th(ui, "Lv");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(fix && s.fix_resource_mode != 1.0, |ui| {
                        changed |= opt_number(ui, "fix_lv", &mut s.fix_lv, false);
                    });
                    ui.label("Lvまで育てたと仮定");
                });
                small(ui, "指定レベル以上のポケモンはそのままでシミュレーションします。", 400.0);
            });
            ui.end_row();

            th(ui, "進化");
            ui.vertical(|ui| {
                ui.add_enabled_ui(fix && s.fix_resource_mode != 1.0, |ui| {
                    changed |= ui.checkbox(&mut s.fix_evolve, "最終進化にしたと仮定").changed();
                });
                small(ui, "イーブイ等、複数の進化先がある場合は全ての進化先をシミュレーションします。", 400.0);
                ui.add_enabled_ui(fix, |ui| {
                    changed |= ui.checkbox(&mut s.fix_evolve_exclude_sleep, "睡眠時間が必要な進化を除く").changed();
                });
            });
            ui.end_row();

            th(ui, "サブスキルの種");
            ui.vertical(|ui| {
                ui.add_enabled_ui(fix, |ui| {
                    changed |= ui.checkbox(&mut s.fix_sub_skill_seed, "最大まで与えたものとして仮定").changed();
                });
                small(ui, "サブスキルの種を与えたと仮定", 400.0);
            });
            ui.end_row();

            th(ui, "メインスキルの種");
            ui.vertical(|ui| {
                ui.add_enabled_ui(fix, |ui| {
                    ui.horizontal(|ui| {
                        for (v, l) in [(0.0, "なし"), (1.0, "厳選設定のスキルレベルまで"), (2.0, "最大")] {
                            changed |= ui.radio_value(&mut s.fix_skill_seed, v, l).changed();
                        }
                    });
                });
                small(ui, "メインスキルの種を与えたと仮定", 400.0);
            });
            ui.end_row();

            th(ui, "おやすみリボン");
            ui.vertical(|ui| {
                ui.add_enabled_ui(fix, |ui| {
                    changed |= ui.checkbox(&mut s.fix_sleep_ribbon, "2000時間まで寝たと仮定").changed();
                });
                small(ui, "2000時間未満のポケモンは睡眠時間2000時間として計算します（2000時間以上の場合はそのまま）。", 400.0);
            });
            ui.end_row();
        });
    });
    changed
}

/// アメ・ゆめのかけら管理(本家 resource-edit-popup.vue)
fn resource_popup(ctx: &egui::Context, c: &mut Config) -> bool {
    let mut changed = false;
    style::popup(ctx, "resource", "リソース管理", |ui| {
        let candy = &mut c.candy;
        ui.horizontal(|ui| {
            ui.label("ゆめのかけら");
            changed |= opt_number_hint(ui, "candy_shard", &mut candy.shard, false, 160.0, "未入力の場合は無制限");
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label("ばんのうアメ");
            for key in ["s", "m", "l"] {
                ui.label(format!("{}:", key.to_uppercase()));
                changed |= bag_number(ui, &mut candy.bag, key, 50.0);
            }
        });
        ui.add_space(5.0);
        small(
            ui,
            "ばんのうアメは各ポケモンに使用できる量を指定してください。
Sを10個指定した場合、チームシミュレーションではSを10個使用した個体を複数編成したものがレコメンドされる可能性があります。",
            900.0,
        );
        ui.add_space(10.0);

        // アメの種類ごと(本家 seedPokemonList: アメ名が初めて出てくるポケモン)
        let mut seed: Vec<&pokesle_core::data::Pokemon> = Vec::new();
        for p in &master().pokemon {
            if seed.iter().all(|x| x.candy_name != p.candy_name) {
                seed.push(p);
            }
        }
        egui::Grid::new("resource_candy").striped(true).num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
            th(ui, "タイプ");
            th(ui, "アメ名");
            th(ui, "アメ数");
            ui.end_row();
            for p in seed {
                ui.label(&p.type_name);
                ui.label(&p.candy_name);
                changed |= bag_number(ui, &mut candy.bag, &p.candy_name, 80.0);
                ui.end_row();
            }
        });
    });
    changed
}

/// アメの数の入力。未設定なら空欄で出し、空欄にしたら 0(本家の null と同じ扱い)
fn bag_number(ui: &mut egui::Ui, bag: &mut std::collections::HashMap<String, f64>, key: &str, width: f32) -> bool {
    let mut v = bag.get(key).copied();
    if opt_number_hint(ui, &format!("candy_bag_{key}"), &mut v, false, width, "") {
        bag.insert(key.to_string(), v.unwrap_or(0.0));
        return true;
    }
    false
}

/// 空欄で null になる数値入力(本家 `$event || null`: 0 も null になる)
fn opt_number(ui: &mut egui::Ui, id: &str, value: &mut Option<f64>, zero_is_null: bool) -> bool {
    opt_number_hint(ui, id, value, zero_is_null, 50.0, "-")
}

fn opt_number_hint(ui: &mut egui::Ui, id: &str, value: &mut Option<f64>, zero_is_null: bool, width: f32, hint: &str) -> bool {
    style::opt_number(ui, id, value, zero_is_null, width, hint)
}

// ---- 厳選設定 ----

fn select_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let mut changed = false;
    let label = style::plain(format!("厳選設定:{}", if c.simulation.select_type == 1.0 { "2段階評価" } else { "厳選度" }));
    style::setting(ui, "select", "厳選設定", &label, false, |ui| {
        let s = &mut c.simulation;
        style::alert(ui, "この設定はチェックリストでの基準と共有しているため、変更した場合はチェックリストの基準値も合わせて見直ししてください。");
        ui.add_space(8.0);
        grid(ui, "select_grid", |ui| {
            th(ui, "厳選設定");
            egui::ComboBox::from_id_salt("select_type").selected_text(if s.select_type == 1.0 { "2段階評価" } else { "厳選度" }).show_ui(ui, |ui| {
                changed |= ui.selectable_value(&mut s.select_type, 0.0, "厳選度").changed();
                changed |= ui.selectable_value(&mut s.select_type, 1.0, "2段階評価").changed();
            });
            ui.end_row();
            if s.select_type == 1.0 {
                th(ui, "目標スコア");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        changed |= number(ui, &mut s.select_border, 1.0, 80.0);
                        ui.label("%");
                    });
                    small(
                        ui,
                        "例えば90%にすると、厳選度90%の個体に対しこの個体が稼ぐエナジーが何%あるか計算します。\nパーセンタイルの場合上位80%の時点で理論値と大差なかったり逆にものすごく差がある可能性がありますが、目標スコア比で見るとこの問題が回避できます。",
                        300.0,
                    );
                });
                ui.end_row();
            }
        });
    });
    changed
}

// ---- 料理設定 ----

pub fn cooking_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let m = master();
    let mut changed = false;
    let disabled = evaluate_lv_list(Some(&c.simulation)).iter().filter(|x| !x.enable).count();
    let label = style::plain(if disabled > 0 { format!("料理設定(無効:{disabled}種)") } else { "料理設定".into() });
    style::setting(ui, "cooking", "料理設定", &label, disabled > 0, |ui| {
        style::alert(
            ui,
            "各種シミュレーションに使用する料理を選択してください。\n任意の料理のレシピレベルを上げたい場合はその料理のみチェックを入れてください。\nエナジー効率が良い料理は、エナジーが高いものではなく追加エナジーが高いものになります。",
        );
        ui.add_space(8.0);
        let s = &mut c.simulation;
        ui.horizontal(|ui| {
            th(ui, "シミュレーション設定");
            ui.add_space(12.0);
            changed |= ui.checkbox(&mut s.cooking_recipe_lv_type1, "実際のLv").changed();
            ui.add_space(12.0);
            changed |= ui.checkbox(&mut s.cooking_recipe_lv_type2, "全て").changed();
            changed |= number(ui, &mut s.cooking_recipe_fix_lv, 1.0, 50.0);
            ui.label("Lv");
            ui.add_space(12.0);
            changed |= ui.checkbox(&mut s.cooking_recipe_lv_type3, "").changed();
            changed |= number(ui, &mut s.cooking_recipe_repeat_lv, 1.0, 50.0);
            ui.label("回料理した際のLv");
            ui.add_space(12.0);
            changed |= ui.checkbox(&mut s.cooking_recipe_lv_type4, "基礎エナジーの").changed();
            changed |= number(ui, &mut s.cooking_recipe_repeat_lv2, 1.0, 50.0);
            ui.label("%回料理した際のLv");
        });
        small(
            ui,
            "上記で選択したもののうち、最もレベルの高いものが各種シミュレーションに採用されます。\n「個々に設定されたレシピレベルを使用する」のみを使用するのが最も正しい値になりますが、最上位料理であっても育っていない場合は評価されなくなります。\n料理を育てたい通常週と、スコアアタック週で使い分けてください。",
            900.0,
        );
        ui.add_space(8.0);

        let mut list = evaluate_lv_list(Some(s));
        let type_order = |t: CookingType| CookingType::ALL.iter().position(|x| *x == t).unwrap_or(0);
        list.sort_by(|a, b| {
            type_order(a.cooking().cooking_type).cmp(&type_order(b.cooking().cooking_type)).then(pokesle_core::js::cmp_num(b.cooking().energy, a.cooking().energy))
        });
        let max_lv = m.cooking_stats.max_recipe_lv as f64;
        let mut headers: Vec<String> = ["", "タイプ", "名前", "レシピ", "食材数", "ボーナス", "基礎エナジー"].map(String::from).to_vec();
        if s.cooking_recipe_lv_type1 {
            headers.push("実際のLv".into());
        }
        if s.cooking_recipe_lv_type3 {
            headers.push(format!("{}回\n料理時Lv", js_num(s.cooking_recipe_repeat_lv)));
        }
        if s.cooking_recipe_lv_type4 {
            headers.push(format!("基礎エナジー\n×{}%回\n料理時Lv", js_num(s.cooking_recipe_repeat_lv2)));
        }
        headers.push("計算Lv".into());
        headers.push("最終エナジー".into());

        egui::Grid::new("cooking_table").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            let mut all = m.cookings.iter().all(|x| s.enable_cooking.get(&x.name).copied().unwrap_or(false));
            for (i, h) in headers.iter().enumerate() {
                if i == 0 {
                    if ui.checkbox(&mut all, "").changed() {
                        for x in &m.cookings {
                            s.enable_cooking.insert(x.name.clone(), all);
                        }
                        changed = true;
                    }
                } else {
                    th(ui, h);
                }
            }
            ui.end_row();
            for e in &list {
                let cook = e.cooking();
                let enabled = s.enable_cooking.get(&cook.name).copied().unwrap_or(false);
                let mut v = enabled;
                if ui.checkbox(&mut v, "").changed() {
                    s.enable_cooking.insert(cook.name.clone(), v);
                    changed = true;
                }
                let weak = |t: String| if enabled { egui::RichText::new(t) } else { egui::RichText::new(t).color(style::th_color()) };
                ui.label(weak(cook.cooking_type.name().into()));
                ui.label(weak(cook.name.clone()));
                ui.label(weak(cook.food_list.iter().map(|f| format!("{}×{}", f.name, js_num(f.num))).collect::<Vec<_>>().join("\n")));
                ui.label(weak(js_num(cook.food_num)));
                ui.label(weak(format!("{}%", fmt_fixed(cook.rate * 100.0, 0))));
                ui.label(weak(fmt_fixed(cook.energy, 0)));
                if s.cooking_recipe_lv_type1 {
                    if cook.rate > 1.0 {
                        let setting = s.cooking_settings.entry(cook.name.clone()).or_default();
                        if ui.add(egui::DragValue::new(&mut setting.lv).range(1.0..=max_lv)).changed() {
                            changed = true;
                        }
                    } else {
                        ui.label("-");
                    }
                }
                if s.cooking_recipe_lv_type3 {
                    ui.label(weak(e.lv3.map(js_num).unwrap_or_default()));
                }
                if s.cooking_recipe_lv_type4 {
                    ui.label(weak(e.lv4.map(js_num).unwrap_or_default()));
                }
                ui.label(weak(js_num(e.lv)));
                ui.label(weak(fmt_fixed(e.fix_energy, 0)));
                ui.end_row();
            }
        });
    });
    changed
}

// ---- 下振れ補正 ----

fn expect_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let m = master();
    let mut changed = false;
    style::setting(ui, "expect", "下振れ補正", &style::plain("下振れ補正"), false, |ui| {
        ui.set_max_width(640.0);
        style::alert(
            ui,
            "1%で当たるものを100回試行した時の期待値は1%×100で1回ですが、1回以上当たる確率は63.4%程度しかありません。\nつまり、下振れして1回も当たらない確率が36.6%あるということです。\n期待値での計算でシミュレーションすると下振れした際に困るものも多いので、食材の獲得、料理パワーアップなどの料理に関する項目、げんきオールなどのヒーラー系の項目について下振れ補正をONにすると安定感のあるシミュレーションが出来ます（ただし当該ポケモンたちの想定エナジーが低く見積もられる点はご注意ください）。",
        );
        ui.add_space(8.0);
        let e = &mut c.simulation.expect_type;
        let radio = |ui: &mut egui::Ui, e: &mut std::collections::HashMap<String, f64>, key: &str| -> bool {
            let v = e.entry(key.to_string()).or_insert(0.0);
            ui.horizontal(|ui| ui.radio_value(v, 0.0, "通常期待値").changed() | ui.radio_value(v, 1.0, "下振れ補正").changed()).inner
        };
        egui::Grid::new("expect_grid").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
            th(ui, "下振れ補正ボーダー");
            ui.horizontal(|ui| {
                let v = e.entry("border".into()).or_insert(0.0);
                changed |= number(ui, v, 1.0, 80.0);
                ui.label("%");
            });
            ui.end_row();
            th(ui, "食材");
            changed |= radio(ui, e, "food");
            ui.end_row();
            for cat in &m.skill_categories {
                let skills: Vec<_> = m.skills.iter().filter(|s| s.category == cat.id).collect();
                if skills.is_empty() {
                    continue;
                }
                style::section_title(ui, &cat.name);
                ui.end_row();
                for s in skills {
                    th(ui, &s.name);
                    changed |= radio(ui, e, &s.name);
                    ui.end_row();
                }
            }
        });
    });
    changed
}

// ---- その他設定 ----

fn other_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let m = master();
    let mut changed = false;
    style::setting(ui, "other", "その他設定", &style::plain("その他設定"), false, |ui| {
        grid(ui, "other_grid", |ui| {
            th(ui, "なべの大きさ");
            ui.horizontal(|ui| {
                changed |= number(ui, &mut c.simulation.pot_size, 1.0, 60.0);
                ui.label("個");
            });
            ui.end_row();

            th(ui, "ナイトキャップピカチュウ");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let max = m.night_cap_pikachu.len() as f64;
                    changed |= ui.add_sized([80.0, 22.0], egui::DragValue::new(&mut c.team_simulation.night_cap_pikachu).range(0.0..=max)).changed();
                    ui.label("Lv");
                });
                small(ui, "0の場合はナイトキャップピカチュウなし", 300.0);
            });
            ui.end_row();

            th(ui, "ゆめのかけら評価");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    changed |= number(ui, &mut c.simulation.shard_weight, 1.0, 80.0);
                    ui.label("%");
                });
                small(ui, "0%:エナジーだけで評価\n100%:ゆめのかけらで評価\n50%:どっちもほどほど", 300.0);
            });
            ui.end_row();

            th(ui, "リサーチランク");
            ui.vertical(|ui| {
                changed |= ui.checkbox(&mut c.simulation.research_rank_max, "カンスト").changed();
                small(ui, "リサボをゆめのかけらとして評価するか", 100.0);
            });
            ui.end_row();

            th(ui, "睡眠時間");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    changed |= number(ui, &mut c.sleep_time, 0.1, 50.0);
                    ui.label("時間");
                });
                style::danger(ui, "睡眠時間を変更すると厳選情報の再計算が必要です");
            });
            ui.end_row();

            th(ui, "チェック頻度");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    changed |= number(ui, &mut c.check_freq, 1.0, 50.0);
                    ui.label("回");
                });
                style::danger(ui, "チェック頻度を変更すると厳選情報の再計算が必要です");
            });
            ui.end_row();

            th(ui, "スレッド数");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    changed |= ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut c.worker_num).range(1.0..=64.0)).changed();
                    if ui.button("自動").on_hover_text("このPCの論理CPU数を入れます").clicked() {
                        c.worker_num = std::thread::available_parallelism().map_or(4.0, |n| n.get().min(64) as f64);
                        changed = true;
                    }
                });
                style::danger(ui, "スレッド数を増やすと処理が速くなりますが、メモリやCPUの負荷が増加します");
            });
            ui.end_row();
        });
        ui.add_space(12.0);
        if form_button(ui, "初期設定に戻す", ButtonKind::Important).on_hover_text("本家にはない項目です。全ての設定を初期値に戻します").clicked() {
            *c = Config::default_config();
            changed = true;
        }
    });
    changed
}
