//! チーム編成画面

use eframe::egui;
use pokesle_core::data::master;
use pokesle_core::team_sim::{run_team_simulation, TeamMember, TeamSimulationRequest, TeamSimulationResult};

use super::{fmt_num, food_icon};
use crate::app::Shared;
use crate::job::Job;

const DAYS: [&str; 7] = ["月曜", "火曜", "水曜", "木曜", "金曜", "土曜", "日曜"];

pub struct TeamView {
    /// -1 は1週間
    target_day: i32,
    target_hour: f64,
    initial_cooking_change: f64,
    initial_cooking_power_up: f64,
    before_energy: f64,
    job: Option<Job<TeamSimulationResult>>,
    result: Option<TeamSimulationResult>,
    error: String,
}

impl Default for TeamView {
    fn default() -> Self {
        TeamView {
            target_day: -1,
            target_hour: 24.0,
            initial_cooking_change: 0.0,
            initial_cooking_power_up: 0.0,
            before_energy: 0.0,
            job: None,
            result: None,
            error: String::new(),
        }
    }
}

impl TeamView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            match r {
                Ok(r) => {
                    self.result = Some(r);
                    self.error.clear();
                }
                Err(e) => self.error = e,
            }
        }

        egui::Grid::new("team_req").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("対象");
            egui::ComboBox::from_id_salt("day")
                .selected_text(if self.target_day < 0 { "1週間".to_string() } else { DAYS[self.target_day as usize].to_string() })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.target_day, -1, "1週間");
                    for (i, d) in DAYS.iter().enumerate() {
                        ui.selectable_value(&mut self.target_day, i as i32, *d);
                    }
                });
            ui.end_row();
            if self.target_day >= 0 {
                ui.label("計算する時間");
                ui.add(egui::DragValue::new(&mut self.target_hour).range(1.0..=24.0).suffix(" 時間"));
                ui.end_row();
                ui.label("今日のエナジー(計算開始前)");
                ui.add(egui::DragValue::new(&mut self.before_energy).range(0.0..=1e9).speed(100.0));
                ui.end_row();
            }
            ui.label("料理チャンス初期値");
            ui.add(egui::DragValue::new(&mut self.initial_cooking_change).range(0.0..=70.0).suffix(" %"));
            ui.end_row();
            ui.label("料理パワーアップ初期値");
            ui.add(egui::DragValue::new(&mut self.initial_cooking_power_up).range(0.0..=999.0));
            ui.end_row();
        });

        ui.horizontal(|ui| {
            let can_run = self.job.is_none() && !shared.box_file.list.is_empty();
            if ui.add_enabled(can_run, egui::Button::new("シミュレーション実行")).clicked() {
                let config = shared.config.clone();
                let list = shared.box_file.list.clone();
                let table = shared.usable_evaluate_table().cloned();
                let request = TeamSimulationRequest {
                    target_day: (self.target_day >= 0).then_some(self.target_day as f64),
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
            if shared.box_file.list.is_empty() {
                ui.label("ボックスにポケモンを登録してください");
            }
        });
        if let Some(job) = &self.job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        ui.separator();

        let Some(result) = &self.result else { return };
        if result.team_list.is_empty() {
            ui.label("条件を満たすチームが見つかりませんでした");
            return;
        }
        let m = master();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (rank, team) in result.team_list.iter().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.heading(format!("{}位", rank + 1));
                        ui.label(format!("スコア {}", fmt_num(team.score)));
                        ui.label(format!("エナジー {}", fmt_num(team.energy)));
                        ui.label(format!("ゆめのかけら(スキル) {}", fmt_num(team.skill_shard)));
                    });
                    egui::Grid::new(("team", rank)).striped(true).num_columns(6).show(ui, |ui| {
                        for title in ["ポケモン", "Lv", "食材", "エナジー/日", "スキル/日", "スキルエナジー/日"] {
                            ui.strong(title);
                        }
                        ui.end_row();
                        for member in &team.pokemon_list {
                            match member {
                                TeamMember::Pokemon { pokemon, .. } => {
                                    let bx = pokemon.box_pokemon.as_ref();
                                    let name = format!("{}{}", pokemon.base().name, if pokemon.bag_over_operation { "(いつ育)" } else { "" });
                                    ui.label(name);
                                    ui.label(format!("{}", pokemon.lv));
                                    ui.horizontal(|ui| {
                                        if let Some(b) = bx {
                                            ui.set_height(24.0);
                                            food_icon::food_list(ui, b);
                                        }
                                    });
                                    ui.label(fmt_num(pokemon.energy_per_day));
                                    ui.label(format!("{:.2}", pokemon.skill_per_day));
                                    ui.label(fmt_num(pokemon.skill_energy_per_day));
                                }
                                TeamMember::NightCapPikachu(p) => {
                                    ui.label("ナイトキャップピカチュウ");
                                    ui.label(format!("{}", p.lv));
                                    ui.label("");
                                    ui.label(fmt_num(p.b_ep_d));
                                    ui.label("");
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        }
                    });
                    ui.collapsing("料理", |ui| {
                        for c in &team.cooking_list {
                            ui.label(format!("{}  {} (鍋 {})", m.cookings[c.cooking].name, fmt_num(c.energy), c.pot_size));
                        }
                    });
                });
            }
        });
    }
}
