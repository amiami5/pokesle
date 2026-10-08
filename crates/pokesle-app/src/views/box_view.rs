//! ボックス画面: 一覧・追加/編集・TSV取り込み・概算・厳選テーブル作成

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use pokesle_core::box_io::{self, TsvColumns};
use pokesle_core::box_sim::BoxResult;
use pokesle_core::data::master;
use pokesle_core::evaluate_table::{simulate_evaluate_table, EvalLv, EvaluateTable};
use pokesle_core::team_sim::simulate_box;

use super::pokemon_edit::{EditResult, PokemonEdit};
use super::{food_abc, fmt_num};
use crate::app::Shared;
use crate::job::Job;
use crate::storage;

#[derive(Default)]
pub struct BoxView {
    search: String,
    edit: Option<PokemonEdit>,
    import_open: bool,
    import_text: String,
    import_message: String,
    simulate_job: Option<Job<Vec<BoxResult>>>,
    table_job: Option<Job<EvaluateTable>>,
    /// ボックス位置 → 概算結果(いつ育モードを除く)
    results: Vec<Option<BoxResult>>,
    error: String,
}

impl BoxView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        self.poll_jobs(shared);

        ui.horizontal(|ui| {
            if ui.button("＋ 追加").clicked() {
                self.edit = Some(PokemonEdit::new(None, None));
            }
            if ui.button("TSVから取り込み").clicked() {
                self.import_open = true;
            }
            ui.separator();
            let running = self.simulate_job.is_some() || self.table_job.is_some();
            if ui.add_enabled(!running && !shared.box_file.list.is_empty(), egui::Button::new("概算を計算")).clicked() {
                let config = shared.config.clone();
                let list = shared.box_file.list.clone();
                let table = shared.usable_evaluate_table().cloned();
                self.simulate_job = Some(Job::spawn(ui.ctx(), move |p| {
                    p.set(0.0, "ボックスを概算しています…");
                    simulate_box(&config, &list, table.as_ref(), true)
                }));
            }
            if ui.add_enabled(!running, egui::Button::new("厳選テーブルを作成")).on_hover_text("厳選度の計算に使う基準表を作ります(数分かかります)").clicked() {
                let config = shared.config.clone();
                self.table_job = Some(Job::spawn(ui.ctx(), move |p| simulate_evaluate_table(&config, &p)));
            }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("ポケモン名で絞り込み").desired_width(180.0));
        });
        if let Some(job) = &self.simulate_job {
            job.show_progress(ui);
        }
        if let Some(job) = &self.table_job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        ui.separator();

        self.table(ui, shared);
        self.import_window(ui.ctx(), shared);

        if let Some(edit) = &mut self.edit {
            match edit.ui(ui.ctx()) {
                EditResult::None => {}
                EditResult::Cancel => self.edit = None,
                EditResult::Save(index, pokemon) => {
                    match index {
                        Some(i) if i < shared.box_file.list.len() => shared.box_file.list[i] = pokemon,
                        _ => shared.box_file.list.push(pokemon),
                    }
                    shared.save_box();
                    self.results.clear();
                    self.edit = None;
                }
            }
        }
    }

    fn poll_jobs(&mut self, shared: &mut Shared) {
        if let Some(job) = &mut self.simulate_job
            && let Some(r) = job.poll()
        {
            self.simulate_job = None;
            match r {
                Ok(list) => {
                    self.results = vec![None; shared.box_file.list.len()];
                    for r in list {
                        if r.pokemon.bag_over_operation {
                            continue;
                        }
                        if let Some(i) = r.pokemon.box_index()
                            && let Some(slot) = self.results.get_mut(i)
                            && slot.as_ref().is_none_or(|x| x.pokemon.score < r.pokemon.score)
                        {
                            *slot = Some(r);
                        }
                    }
                    self.error.clear();
                }
                Err(e) => self.error = e,
            }
        }
        if let Some(job) = &mut self.table_job
            && let Some(r) = job.poll()
        {
            self.table_job = None;
            match r {
                Ok(table) => {
                    if let Err(e) = storage::save_evaluate_table(&table) {
                        self.error = format!("厳選テーブルを保存できませんでした: {e}");
                    }
                    shared.evaluate_table = Some(table);
                    shared.status = "厳選テーブルを作成しました".into();
                }
                Err(e) => self.error = e,
            }
        }
    }

    fn table(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let m = master();
        let query = self.search.trim().to_string();
        let rows: Vec<usize> = (0..shared.box_file.list.len())
            .filter(|&i| query.is_empty() || shared.box_file.list[i].name.contains(&query))
            .collect();
        let mut action: Option<(usize, bool)> = None; // (index, delete?)

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(36.0))
            .column(Column::initial(150.0).at_least(80.0))
            .column(Column::exact(40.0))
            .column(Column::exact(40.0))
            .column(Column::exact(44.0))
            .column(Column::initial(260.0).at_least(80.0))
            .column(Column::initial(80.0))
            .column(Column::initial(90.0))
            .column(Column::initial(90.0))
            .column(Column::initial(70.0))
            .column(Column::remainder())
            .header(22.0, |mut header| {
                for title in ["No", "ポケモン", "Lv", "スキ", "食材", "サブスキル", "せいかく", "概算日給", "スコア", "厳選", ""] {
                    header.col(|ui| {
                        ui.strong(title);
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let i = rows[row.index()];
                    let p = &shared.box_file.list[i];
                    let result = self.results.get(i).and_then(|r| r.as_ref());
                    row.col(|ui| {
                        ui.label(format!("{}", i + 1));
                    });
                    row.col(|ui| {
                        let mark = if p.fix == Some(1.0) { "📌" } else { "" };
                        ui.label(format!("{mark}{}", p.name));
                    });
                    row.col(|ui| {
                        ui.label(format!("{}", p.lv));
                    });
                    row.col(|ui| {
                        ui.label(p.skill_lv.map(|x| x.to_string()).unwrap_or_default());
                    });
                    row.col(|ui| {
                        ui.label(food_abc(&p.name, &p.food_list));
                    });
                    row.col(|ui| {
                        let short: Vec<&str> = p.sub_skill_list.iter().map(|s| m.sub_skill(s).map(|x| x.short.as_str()).unwrap_or("?")).collect();
                        ui.label(short.join(" "));
                    });
                    row.col(|ui| {
                        ui.label(&p.nature);
                    });
                    row.col(|ui| {
                        if let Some(r) = result {
                            ui.label(fmt_num(r.pokemon.energy_per_day));
                        }
                    });
                    row.col(|ui| {
                        if let Some(r) = result {
                            ui.label(fmt_num(r.pokemon.score));
                        }
                    });
                    row.col(|ui| {
                        let best = result
                            .and_then(|r| r.evaluate_result.as_ref())
                            .and_then(|e| e.best.get(&EvalLv::Max))
                            .and_then(|b| b.energy.as_ref());
                        if let Some(b) = best {
                            ui.label(format!("{:.0}%", b.score * 100.0)).on_hover_text(b.name.clone().unwrap_or_default());
                        }
                    });
                    row.col(|ui| {
                        if ui.small_button("編集").clicked() {
                            action = Some((i, false));
                        }
                        if ui.small_button("削除").clicked() {
                            action = Some((i, true));
                        }
                    });
                });
            });

        match action {
            Some((i, false)) => self.edit = Some(PokemonEdit::new(Some(i), Some(shared.box_file.list[i].clone()))),
            Some((i, true)) => {
                shared.box_file.list.remove(i);
                shared.save_box();
                self.results.clear();
            }
            None => {}
        }
    }

    fn import_window(&mut self, ctx: &egui::Context, shared: &mut Shared) {
        if !self.import_open {
            return;
        }
        let mut open = true;
        egui::Window::new("TSVから取り込み").open(&mut open).default_width(640.0).show(ctx, |ui| {
            ui.label("スプレッドシート等からコピーした TSV を貼り付けてください。今のボックスは置き換えられます。");
            ui.label("列: 名前 / Lv / スキルLv / 食材1～3 / サブスキル1～5 / せいかく / 色違い / 固定 / 睡眠時間 / … (本家と同じ並び)");
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.import_text).desired_width(f32::INFINITY).desired_rows(12).code_editor());
            });
            ui.horizontal(|ui| {
                if ui.button("取り込む").clicked() {
                    let list = box_io::import_tsv(&self.import_text, &TsvColumns::from_config(&shared.config));
                    if list.is_empty() {
                        self.import_message = "取り込めるポケモンがありませんでした".into();
                    } else {
                        self.import_message = format!("{}匹を取り込みました", list.len());
                        shared.box_file.list = list;
                        shared.save_box();
                        self.results.clear();
                        self.import_text.clear();
                    }
                }
                ui.label(&self.import_message);
            });
        });
        if !open {
            self.import_open = false;
        }
    }
}
