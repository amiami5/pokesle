//! チーム編成画面

use std::collections::HashMap;

use eframe::egui;
use pokesle_core::data::master;
use pokesle_core::team_sim::{run_team_simulation, TeamSimulationRequest, TeamSimulationResult};

use pokesle_core::config::Config;

use super::pokemon_edit::{EditResult, PokemonEdit};
use super::team_result::{self, LvState, ResultSetting};
use super::{common_setting, fmt_num, pokemon_filter, style};
use crate::app::Shared;
use crate::job::Job;

const DAYS: [&str; 7] = ["月曜", "火曜", "水曜", "木曜", "金曜", "土曜", "日曜"];

pub struct TeamView {
    /// -1 は1週間
    target_day: i32,
    /// true のときは曜日選択を無効化し、1日(最大24時間・初期3食)のみを計算する
    one_day_only: bool,
    target_hour: f64,
    cooking_num: f64,
    initial_cooking_change: f64,
    initial_cooking_power_up: f64,
    before_energy: f64,
    job: Option<Job<TeamSimulationResult>>,
    /// 結果と、計算に使った設定
    result: Option<(TeamSimulationResult, Config)>,
    /// 実行中の計算に使っている設定
    job_config: Option<Config>,
    error: String,
    /// 結果の名前横の編集アイコンから開いた編集
    edit: Option<PokemonEdit>,
    /// ◀▶で変えたが、まだ更新していないLv(ボックス位置 → Lv)
    lv_pending: HashMap<usize, f64>,
    /// 前回この画面を描いた egui のパス番号(他の画面から戻ったかの判定)
    last_pass: u64,
}

impl Default for TeamView {
    fn default() -> Self {
        TeamView {
            target_day: -1,
            one_day_only: false,
            target_hour: 24.0,
            cooking_num: 3.0,
            initial_cooking_change: 0.0,
            initial_cooking_power_up: 0.0,
            before_energy: 0.0,
            job: None,
            result: None,
            job_config: None,
            error: String::new(),
            edit: None,
            lv_pending: HashMap::new(),
            last_pass: 0,
        }
    }
}

impl TeamView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        // 他の画面を挟んで戻ってきたら、更新していないLvは捨てる
        let pass = ui.ctx().cumulative_pass_nr();
        if pass > self.last_pass + 1 {
            self.lv_pending.clear();
        }
        self.last_pass = pass;
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            match r {
                Ok(r) => {
                    self.result = Some((r, self.job_config.take().unwrap_or_else(|| shared.config.clone())));
                    self.error.clear();
                }
                Err(e) => self.error = e,
            }
        }

        // 1段目: 共通設定(本家 simulation.vue)
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            changed |= common_setting::ui(ui, &mut shared.config, &shared.box_file.list);
        });
        // 2段目: シミュレーション設定・除外フィルタ・チーム条件
        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            changed |= simulation_setting(ui, &mut shared.config);
            let n = pokemon_filter::exclude_count(&shared.box_file.list, &shared.config.simulation.filter);
            let label = style::plain(if n == 0 { "除外なし".to_string() } else { format!("除外条件：{n}匹") });
            style::setting(ui, "team_filter", "除外フィルタ", &label, false, |ui| {
                changed |= pokemon_filter::editor(ui, "team_filter", &mut shared.config.simulation.filter, &shared.box_file.list);
            });
            changed |= require_setting(ui, &mut shared.config);
            changed |= display_setting(ui, &mut shared.config);
        });
        ui.add_space(5.0);

        // 実行ボタンと結果を先に出す(条件は何度も書き換える場所ではないので下に置く)
        self.run_and_results(ui, shared);
        ui.add_space(8.0);

        // 条件(本家の SettingList)
        egui::Frame::new().fill(style::surface()).stroke(egui::Stroke::new(1.0, style::line_color())).corner_radius(6).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("team_req").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                style::th(ui, "現在のエナジー");
                style::number(ui, &mut self.before_energy, 100.0, 100.0);
                ui.end_row();
                style::th(ui, "曜日");
                ui.horizontal(|ui| {
                    let prev_day = self.target_day;
                    ui.add_enabled_ui(!self.one_day_only, |ui| {
                        egui::ComboBox::from_id_salt("day")
                            .selected_text(if self.target_day < 0 { "1週間".to_string() } else { DAYS[self.target_day as usize].to_string() })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.target_day, -1, "1週間");
                                for (i, d) in DAYS.iter().enumerate() {
                                    ui.selectable_value(&mut self.target_day, i as i32, *d);
                                }
                            });
                    });
                    if self.target_day != prev_day && self.target_day >= 0 {
                        let rem_days = (7 - self.target_day) as f64;
                        self.target_hour = rem_days * 24.0;
                        self.cooking_num = rem_days * 3.0;
                    }
                    ui.add_space(8.0);
                    if ui.checkbox(&mut self.one_day_only, "1日のみモード").changed() {
                        if self.one_day_only {
                            self.target_hour = 24.0;
                            self.cooking_num = 3.0;
                        } else if self.target_day >= 0 {
                            let rem_days = (7 - self.target_day) as f64;
                            self.target_hour = rem_days * 24.0;
                            self.cooking_num = rem_days * 3.0;
                        }
                    }
                });
                ui.end_row();
                style::th(ui, "時間");
                ui.horizontal(|ui| {
                    if self.one_day_only {
                        ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut self.target_hour).range(1.0..=24.0));
                    } else if self.target_day >= 0 {
                        let max_hour = (7 - self.target_day) as f64 * 24.0;
                        ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut self.target_hour).range(1.0..=max_hour));
                    } else {
                        let mut v = 168.0;
                        ui.add_enabled(false, egui::DragValue::new(&mut v));
                    }
                    ui.label("時間");
                });
                ui.end_row();
                style::th(ui, "料理");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if self.one_day_only || self.target_day >= 0 {
                            ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut self.cooking_num).range(0.0..=21.0));
                        } else {
                            let mut v = 21.0;
                            ui.add_enabled(false, egui::DragValue::new(&mut v));
                        }
                        ui.label("食分");
                    });
                    if self.one_day_only {
                        style::small(ui, "その1日で作る食数（通常は3、翌朝分も集めたい場合は+1）", 260.0);
                    } else if self.target_day >= 0 {
                        let msg = format!("{}〜日曜の食数（昼・夜スタート時は減らして調整可）", DAYS[self.target_day as usize]);
                        style::small(ui, &msg, 300.0);
                    }
                });
                ui.end_row();
                let first_meal_label = if self.one_day_only {
                    "1食目".to_string()
                } else if self.target_day < 0 {
                    "月曜1食目".to_string()
                } else {
                    format!("{}1食目", DAYS[self.target_day as usize])
                };
                style::th(ui, "料理チャンス");
                ui.horizontal(|ui| {
                    style::number(ui, &mut self.initial_cooking_change, 1.0, 50.0);
                    self.initial_cooking_change = self.initial_cooking_change.clamp(0.0, 70.0);
                    ui.label("%");
                    style::small(ui, &format!("{first_meal_label}から適用（大成功まで持ち越し・最大70%）"), 320.0);
                });
                ui.end_row();
                style::th(ui, "料理パワーアップ");
                ui.horizontal(|ui| {
                    style::number(ui, &mut self.initial_cooking_power_up, 1.0, 50.0);
                    self.initial_cooking_power_up = self.initial_cooking_power_up.max(0.0);
                    let sim = &shared.config.simulation;
                    let base_pot = if !self.one_day_only && self.target_day == 6 { sim.pot_size * 2.0 } else { sim.pot_size };
                    let normal_pot = sim.event_bonus.pot.apply(base_pot, 0.0, sim.camp_ticket);
                    let first_pot = normal_pot + self.initial_cooking_power_up;
                    let note = if self.initial_cooking_power_up > 0.0 {
                        format!("{first_meal_label}の鍋容量: {}（通常: {}）", js_num(first_pot), js_num(normal_pot))
                    } else {
                        format!("{first_meal_label}の鍋容量: {}", js_num(normal_pot))
                    };
                    style::small(ui, &note, 320.0);
                });
                ui.end_row();
                let sim = &mut shared.config.simulation;
                style::th(ui, "余った食材の評価");
                ui.horizontal_top(|ui| {
                    changed |= ui.radio_value(&mut sim.remain_food_mode, 0.0, "隙間に詰める").changed();
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        changed |= ui.radio_value(&mut sim.remain_food_mode, 1.0, "以下割合で換算").changed();
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            changed |= style::percent_number(ui, &mut sim.remain_food_rate, 50.0);
                            ui.label("%");
                        });
                    });
                });
                ui.end_row();
                style::th(ui, "モード");
                ui.horizontal(|ui| {
                    for (v, l) in [(0.0, "通常"), (1.0, "料理育成(カンスト除外のみ)"), (2.0, "料理育成(料理以外無視)")] {
                        changed |= ui.radio_value(&mut sim.mode, v, l).changed();
                    }
                });
                ui.end_row();
            });
        });
        ui.add_space(5.0);
        egui::Frame::new().fill(style::surface()).stroke(egui::Stroke::new(1.0, style::line_color())).corner_radius(6).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                style::th(ui, "所持食材");
                ui.add_space(12.0);
                let m = master();
                let unlimited = shared.config.food_unlimited;
                egui::Grid::new("food_default").num_columns(6).spacing([6.0, 4.0]).show(ui, |ui| {
                    for (i, food) in m.foods.iter().enumerate() {
                        ui.label(&food.name);
                        if unlimited {
                            let mut v = 9999.0;
                            ui.add_enabled(false, egui::DragValue::new(&mut v));
                        } else {
                            let v = shared.config.food_default_num.entry(food.name.clone()).or_insert(0.0);
                            changed |= ui.add_sized([40.0, 20.0], egui::DragValue::new(v).range(0.0..=9999.0)).changed();
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                changed |= ui.checkbox(&mut shared.config.food_unlimited, "食材無制限").changed();
                ui.add_space(8.0);
                ui.add_enabled_ui(!shared.config.food_unlimited, |ui| {
                    if style::form_button(ui, "全部0にする", style::ButtonKind::Normal).clicked() {
                        for v in shared.config.food_default_num.values_mut() {
                            if *v != 0.0 {
                                *v = 0.0;
                                changed = true;
                            }
                        }
                    }
                });
            });
        });
        if changed {
            shared.save_config();
        }
        ui.add_space(5.0);
    }

    /// 実行ボタン・進捗・結果
    fn run_and_results(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let can_run = self.job.is_none() && !shared.box_file.list.is_empty();
        let has_results = self.result.as_ref().is_some_and(|(r, _)| !r.team_list.is_empty());
        let mut set_open: Option<bool> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            ui.add_enabled_ui(can_run, |ui| {
                if style::form_button(ui, "シミュレーション実行", style::ButtonKind::Execute).clicked() {
                    let mut config = shared.sim_config();
                    config.team_simulation.cooking_num = self.cooking_num;
                    self.job_config = Some(config.clone());
                    let list = shared.box_file.list.clone();
                    let table = shared.usable_evaluate_table().cloned();
                    let request = TeamSimulationRequest {
                        target_day: if self.one_day_only { Some(0.0) } else { (self.target_day >= 0).then_some(self.target_day as f64) },
                        until_sunday: !self.one_day_only && self.target_day >= 0,
                        target_hour: self.target_hour,
                        initial_cooking_change: self.initial_cooking_change,
                        initial_cooking_power_up: self.initial_cooking_power_up,
                        before_energy: self.before_energy,
                    };
                    self.job = Some(Job::spawn(ui.ctx(), move |p| {
                        p.set(0.0, "チームを探索しています…");
                        run_team_simulation(&config, &list, table.as_ref(), &request, &|v| p.set(v, ""))
                    }));
                }
            });
            self.lv_pending.retain(|&i, lv| shared.box_file.list.get(i).is_some_and(|p| p.lv != *lv));
            let n = self.lv_pending.len();
            let r = ui.add_enabled_ui(n > 0, |ui| style::form_button(ui, &format!("Lvを更新({n}匹)"), style::ButtonKind::Execute)).inner;
            if r.clicked() {
                for (i, lv) in self.lv_pending.drain() {
                    shared.box_file.list[i].lv = lv;
                }
                shared.save_box();
            }
            let r = ui.add_enabled_ui(n > 0, |ui| style::form_button(ui, "Lvの変更を破棄", style::ButtonKind::Normal)).inner;
            if r.clicked() {
                self.lv_pending.clear();
            }
            if ui.add_enabled_ui(has_results, |ui| style::form_button(ui, "すべて展開", style::ButtonKind::Normal)).inner.clicked() {
                set_open = Some(true);
            }
            if ui.add_enabled_ui(has_results, |ui| style::form_button(ui, "すべて折りたたむ", style::ButtonKind::Normal)).inner.clicked() {
                set_open = Some(false);
            }
        });
        if shared.box_file.list.is_empty() {
            ui.label("ボックスにポケモンを登録してください");
        }
        if let Some(job) = &self.job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        ui.add_space(5.0);

        self.edit_window(ui.ctx(), shared);

        let Some((result, result_config)) = &self.result else {
            ui.label("シミュレーション実行ボタンを押してください");
            return;
        };
        if result.team_list.is_empty() {
            ui.label("条件を満たすチームが見つかりませんでした");
            return;
        }
        let setting = ResultSetting::load(&shared.config);
        let short = shared.config.extra.get("pokemonList").and_then(|v| v.get("subSkillShort")).and_then(|v| v.as_bool()).unwrap_or(true);
        let mut edit = None;
        let mut lv_change = None;
        let lv_state = LvState { box_list: &shared.box_file.list, pending: &self.lv_pending };
        ui.vertical(|ui| {
            for (rank, team) in result.team_list.iter().enumerate() {
                let title = format!("{}: {} (最終エナジー:{})", rank + 1, fmt_num(team.score), fmt_num(team.energy));
                let header = egui::CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(("team_result", rank)).default_open(rank == 0).open(set_open);
                let r = header.show(ui, |ui| {
                    egui::ScrollArea::horizontal().id_salt(("team_result_scroll", rank)).show(ui, |ui| {
                        let r = team_result::team_table(ui, team, &shared.config, result_config, setting, short, lv_state);
                        edit = edit.or(r.edit);
                        lv_change = lv_change.or(r.lv);
                    });
                });
                r.header_response.on_hover_text("スコアは「エナジー✕(100% + 追加ゆめのかけら ÷ エナジーによるゆめのかけら ✕ ゆめのかけら評価倍率)」です。");
                ui.add_space(10.0);
            }
        });
        if let Some((i, lv)) = lv_change {
            if shared.box_file.list.get(i).is_some_and(|p| p.lv == lv) {
                self.lv_pending.remove(&i);
            } else {
                self.lv_pending.insert(i, lv);
            }
        }
        // 結果は計算し直さない(本家も同じ)
        if let Some(i) = edit
            && let Some(p) = shared.box_file.list.get(i)
        {
            self.edit = Some(PokemonEdit::new(Some(i), Some(p.clone())));
        }
    
    }

    fn edit_window(&mut self, ctx: &egui::Context, shared: &mut Shared) {
        let Some(edit) = &mut self.edit else { return };
        match edit.ui(ctx) {
            EditResult::None => {}
            EditResult::Cancel => self.edit = None,
            EditResult::Save(index, pokemon) => {
                if let Some(slot) = index.and_then(|i| shared.box_file.list.get_mut(i)) {
                    *slot = *pokemon;
                    shared.save_box();
                }
                self.edit = None;
            }
        }
    }
}

/// 本家 simulation.vue の「表示設定」
fn display_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let mut s = ResultSetting::load(c);
    let mut changed = false;
    style::setting(ui, "team_display", "表示設定", &style::plain("表示設定"), false, |ui| {
        egui::Grid::new("team_display_grid").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            style::th(ui, "エナジー内訳");
            changed |= ui.checkbox(&mut s.detail, "エナジー内訳").changed();
            ui.end_row();
            style::th(ui, "食材情報");
            changed |= ui.checkbox(&mut s.food, "食材情報").changed();
            ui.end_row();
        });
    });
    if changed {
        s.save(c);
    }
    changed
}

/// 本家 simulation.vue の「シミュレーション設定」
fn simulation_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let mut changed = false;
    style::setting(ui, "team_simulation", "シミュレーション設定", &style::plain("シミュレーション設定"), false, |ui| {
        let t = &mut c.team_simulation;
        egui::Grid::new("team_simulation_grid").num_columns(3).spacing([16.0, 8.0]).show(ui, |ui| {
            let mut row = |ui: &mut egui::Ui, first: &str, title: &str, v: &mut f64| {
                style::th(ui, first);
                style::th(ui, title);
                ui.horizontal(|ui| {
                    ui.label("上位");
                    changed |= style::number(ui, v, 0.1, 50.0);
                    ui.label("匹");
                });
                ui.end_row();
            };
            row(ui, "対象ポケモン", "きのみスコア", &mut t.max_rank_berry);
            row(ui, "", "食材スコア", &mut t.max_rank_food);
            row(ui, "", "総合スコア", &mut t.max_rank_all);
            row(ui, "", "サポート抜きスコア", &mut t.max_rank_not_support);
            ui.label("");
            ui.label("");
            style::small(
                ui,
                "上記設定は重複しては選ばれず、例えばきのみスコアで選ばれたポケモンは後の3つにはヒットしません。\n必ず上記設定の合計の数が対象になります。\nヒーラーなどは総合スコアが高く出ますが、ヒーラーばかりを対象にしても良い結果にならないので総合スコアの件数は少なめにした方が良い結果になりやすいです。",
                250.0,
            );
            ui.end_row();
            style::th(ui, "結果表示");
            ui.label("");
            ui.horizontal(|ui| {
                ui.label("上位");
                changed |= style::number(ui, &mut t.result_num, 0.1, 50.0);
                ui.label("件");
            });
            ui.end_row();
        });
    });
    changed
}

/// 本家 Berry.typeList の並び
const TYPE_ORDER: [&str; 18] = [
    "ノーマル", "ほのお", "みず", "でんき", "くさ", "こおり", "かくとう", "どく", "じめん", "ひこう", "エスパー", "むし", "いわ", "ゴースト",
    "ドラゴン", "あく", "はがね", "フェアリー",
];

fn js_num(x: f64) -> String {
    if x.fract() == 0.0 { format!("{}", x as i64) } else { x.to_string() }
}

/// 本家 simulation.vue の「チーム条件」
fn require_setting(ui: &mut egui::Ui, c: &mut Config) -> bool {
    let m = master();
    let r = &c.team_simulation.require;
    let sp = |k: &str| r.specialty_num.get(k).copied().unwrap_or(0.0);
    let mut parts: Vec<String> = Vec::new();
    if r.day_help_rate > 0.0 {
        parts.push(format!("日中手伝い効率{}%以上", js_num(r.day_help_rate)));
    }
    if r.night_help_rate > 0.0 {
        parts.push(format!("夜間手伝い効率{}%以上", js_num(r.night_help_rate)));
    }
    if r.suimin_exp > 0.0 {
        parts.push(format!("睡眠EXPボーナス{}匹以上", js_num(r.suimin_exp)));
    }
    for k in ["きのみ", "食材", "スキル"] {
        if sp(k) > 0.0 {
            parts.push(format!("{k}とくい{}匹以上", js_num(sp(k))));
        }
    }
    for ty in TYPE_ORDER {
        if r.type_num.get(ty).copied().unwrap_or(0.0) > 0.0 {
            parts.push(format!("{ty}1匹以上"));
        }
    }
    let label: style::Label =
        if parts.is_empty() { style::plain("チーム条件：なし") } else { vec![("チーム条件：".into(), false), (parts.join(" "), true)] };
    let mut changed = false;
    style::setting(ui, "team_require", "チーム条件", &label, false, |ui| {
        let r = &mut c.team_simulation.require;
        egui::Grid::new("team_require_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            let mut row = |ui: &mut egui::Ui, title: &str, v: &mut f64, unit: &str| {
                style::th(ui, title);
                ui.horizontal(|ui| {
                    changed |= style::number(ui, v, 1.0, 80.0);
                    ui.label(unit);
                });
                ui.end_row();
            };
            row(ui, "日中手伝い効率", &mut r.day_help_rate, "%以上(最大222%)");
            row(ui, "夜間手伝い効率", &mut r.night_help_rate, "%以上(最大222%)");
            row(ui, "睡眠EXPボーナス", &mut r.suimin_exp, "匹以上");
            for k in ["きのみ", "食材", "スキル"] {
                row(ui, &format!("{k}とくい"), r.specialty_num.entry(k.to_string()).or_insert(0.0), "匹以上");
            }
            for ty in TYPE_ORDER {
                let color = m.berries.iter().find(|b| b.type_name == ty).and_then(|b| parse_color(&b.type_color)).unwrap_or(style::th_color());
                ui.label(egui::RichText::new(ty).strong().color(color));
                let v = r.type_num.entry(ty.to_string()).or_insert(0.0);
                let mut on = *v == 1.0;
                if ui.checkbox(&mut on, "1匹以上編成する").changed() {
                    *v = if on { 1.0 } else { 0.0 };
                    changed = true;
                }
                ui.end_row();
            }
        });
    });
    changed
}

/// "#rrggbb" を色に
fn parse_color(s: &str) -> Option<egui::Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}
