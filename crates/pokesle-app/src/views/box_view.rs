//! ボックス画面: 一覧・追加/編集・TSV取り込み/書き出し・概算・厳選テーブル作成

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use pokesle_core::box_io::{self, TsvColumns};
use pokesle_core::box_sim::BoxResult;
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::SimPokemon;
use pokesle_core::evaluate_table::{simulate_evaluate_table, EvalLv, EvaluateTable};
use pokesle_core::team_sim::simulate_box;

use super::pokemon_edit::{EditResult, PokemonEdit};
use super::{fmt_fixed, fmt_num, food_icon, nature_label};
use crate::app::Shared;
use crate::job::Job;
use crate::storage;

/// サブスキルの解放レベル(本家 pokemon-simulator.ts)
const SUB_SKILL_LV: [u32; 5] = [10, 25, 50, 70, 80];

/// 数値の表示形式(本家 sortable-table.vue)
#[derive(Clone, Copy)]
enum Fmt {
    /// 小数点以下の桁数
    Num(usize),
    /// ×100 して % 表示(小数点以下の桁数)
    Percent(usize),
}

/// 「シミュ詳細」の列(本家 index.vue の `config.pokemonList.simulatedInfo`)
const DETAIL_COLUMNS: [(&str, Fmt, fn(&SimPokemon) -> f64); 25] = [
    ("おてつだい
時間", Fmt::Num(0), |p| p.speed),
    ("日中おてつ
だい回数", Fmt::Num(2), |p| p.day_help_num),
    ("夜間おてつ
だい回数", Fmt::Num(2), |p| p.night_help_num),
    ("きのみ
個数", Fmt::Num(0), |p| p.berry_num),
    ("きのみ
/手伝い", Fmt::Num(1), |p| p.b_ep_h),
    ("きのみ
/日", Fmt::Num(1), |p| p.b_ep_d),
    ("食材
率", Fmt::Percent(1), |p| p.food_rate),
    ("食材
個数", Fmt::Num(1), |p| p.food_num),
    ("食材
/手伝い", Fmt::Num(1), |p| p.f_ep_h),
    ("食材
/日", Fmt::Num(1), |p| p.f_ep_d),
    ("きのみ+食材
/手伝い", Fmt::Num(1), |p| p.pickup_energy_per_help),
    ("きのみ+食材
/日", Fmt::Num(1), |p| p.pickup_energy_per_day),
    ("所持
数", Fmt::Num(0), |p| p.bag),
    ("いつ育
到達回数", Fmt::Num(1), |p| p.bag_full_help_num),
    ("通常おてつ
だい回数", Fmt::Num(2), |p| p.normal_help_num),
    ("いつ育おてつ
だい回数", Fmt::Num(2), |p| p.berry_help_num),
    ("補正後
スキルLv", Fmt::Num(0), |p| p.fixed_skill_lv),
    ("スキル確率", Fmt::Percent(2), |p| p.skill_rate),
    ("天井補正
スキル確率", Fmt::Percent(2), |p| p.ceil_skill_rate),
    ("スキル
回数/日", Fmt::Num(2), |p| p.skill_per_day),
    ("スキル
エナジー/日", Fmt::Num(1), |p| p.skill_energy_per_day),
    ("スキル
ゆめかけ/日", Fmt::Num(1), |p| p.shard),
    ("エナジー
/日", Fmt::Num(1), |p| p.energy_per_day),
    ("サポート
エナジー/日", Fmt::Num(1), |p| p.support_energy_per_day),
    ("サポート
ゆめかけ/日", Fmt::Num(1), |p| p.support_shard_per_day),
];

/// 列見出しの説明(本家 index.vue の help-button)
fn detail_help(title: &str) -> Option<&'static str> {
    match title {
        "エナジー
/日" => Some("おてつだいボーナスや他ポケモンに影響を与えるスキルの効果を抜きにして、自分自身だけで稼げるエナジーです。"),
        "サポート
エナジー/日" => Some("おてつだいボーナスやげんき回復系スキル、おてつだいサポートなどによる他ポケモンの効率上昇によって、追加で得られるであろうエナジーです。"),
        "サポート
ゆめかけ/日" => Some("おてつだいボーナスやげんき回復系スキルによる他ポケモンの効率上昇、もしくはゆめのかけらボーナスやリサーチEXPボーナスによって、得られるであろうゆめのかけらの量です。"),
        _ => None,
    }
}

fn fmt_value(v: f64, fmt: Fmt) -> String {
    match fmt {
        Fmt::Num(d) => fmt_fixed(v, d),
        Fmt::Percent(d) => format!("{:.*}%", d, v * 100.0),
    }
}

/// 設定 `pokemonList.simulatedInfo`(シミュ詳細)
fn simulated_info(shared: &Shared) -> bool {
    shared.config.extra.get("pokemonList").and_then(|v| v.get("simulatedInfo")).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn set_simulated_info(shared: &mut Shared, on: bool) {
    let list = shared.config.extra.entry("pokemonList").or_insert_with(|| serde_json::json!({}));
    if let Some(map) = list.as_object_mut() {
        map.insert("simulatedInfo".into(), on.into());
    }
    shared.save_config();
}

#[derive(Default)]
pub struct BoxView {
    search: String,
    edit: Option<PokemonEdit>,
    import_open: bool,
    import_text: String,
    import_message: String,
    export_text: String,
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
            if ui.button("TSVの取り込み・書き出し").clicked() {
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
            ui.separator();
            let mut detail = simulated_info(shared);
            if ui.checkbox(&mut detail, "シミュ詳細").on_hover_text("概算の内訳(おてつだい回数・きのみ・食材・スキルなど)の列を表示します").changed() {
                set_simulated_info(shared, detail);
            }
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
        let detail = simulated_info(shared);
        let detail_num = if detail { DETAIL_COLUMNS.len() } else { 0 };

        // 列が画面に収まらないときは横にスクロールする
        egui::ScrollArea::horizontal().show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(36.0).at_least(24.0))
                .column(Column::initial(150.0).at_least(80.0))
                .column(Column::initial(40.0).at_least(24.0))
                .column(Column::initial(72.0).at_least(24.0))
                .column(Column::initial(84.0).at_least(40.0))
                .columns(Column::initial(72.0).at_least(32.0), SUB_SKILL_LV.len())
                .column(Column::initial(80.0))
                .column(Column::initial(90.0))
                .column(Column::initial(70.0))
                .columns(Column::initial(84.0).at_least(32.0), detail_num)
                .column(Column::remainder())
                // シミュ詳細の見出しは2行
                .header(if detail { 40.0 } else { 22.0 }, |mut header| {
                    let sub_titles = SUB_SKILL_LV.map(|lv| format!("サブ{lv}"));
                    let titles = ["No", "ポケモン", "Lv", "スキルLv", "食材"].into_iter().chain(sub_titles.iter().map(|s| s.as_str())).chain(["せいかく", "スコア", "厳選"])
                        .chain(DETAIL_COLUMNS.iter().take(detail_num).map(|(t, _, _)| *t))
                        .chain([""]);
                    for title in titles {
                        header.col(|ui| {
                            let r = ui.strong(title);
                            if let Some(help) = detail_help(title) {
                                r.on_hover_text(help);
                            }
                        });
                    }
                })
                .body(|body| {
                    body.rows(26.0, rows.len(), |mut row| {
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
                            food_icon::food_list(ui, p);
                        });
                        for (slot, unlock) in SUB_SKILL_LV.iter().enumerate() {
                            row.col(|ui| {
                                let Some(name) = p.sub_skill_list.get(slot) else { return };
                                let short = m.sub_skill(name).map(|x| x.short.as_str()).unwrap_or("?");
                                // まだ解放されていないサブスキルは薄く表示する
                                let text = if p.lv >= *unlock as f64 { egui::RichText::new(short) } else { egui::RichText::new(short).weak() };
                                ui.label(text).on_hover_text(name);
                            });
                        }
                        row.col(|ui| {
                            nature_label(ui, &p.nature);
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
                        for (_, fmt, get) in DETAIL_COLUMNS.iter().take(detail_num) {
                            row.col(|ui| {
                                if let Some(r) = result {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(fmt_value(get(&r.pokemon), *fmt));
                                    });
                                }
                            });
                        }
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
        egui::Window::new("TSVの取り込み・書き出し").id(egui::Id::new("tsv_window")).open(&mut open).default_width(900.0).show(ctx, |ui| {
            let mut cols = TsvColumns::from_config(&shared.config);
            egui::CollapsingHeader::new("列番号設定").default_open(true).show(ui, |ui| {
                ui.label("列番号を空にすると取り込み・書き出しの対象外になります。取り込み時はその情報が欠けるので注意してください。");
                if tsv_columns_ui(ui, &mut cols) {
                    cols.save_to_config(&mut shared.config);
                    shared.save_config();
                }
            });
            ui.separator();

            ui.columns(2, |c| {
                let ui = &mut c[0];
                ui.horizontal(|ui| {
                    if ui.button("書き出す").clicked() {
                        self.export_text = box_io::export_tsv(&shared.box_file.list, &cols);
                    }
                });
                egui::ScrollArea::vertical().id_salt("export").max_height(360.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.export_text).desired_width(f32::INFINITY).desired_rows(16).code_editor());
                });

                let ui = &mut c[1];
                // 貼り付けた内容を今の列設定で読んだときの件数
                let lines = self.import_text.lines().filter(|l| !l.trim().is_empty()).count();
                let readable = if lines == 0 { 0 } else { box_io::import_tsv(&self.import_text, &cols).len() };
                ui.horizontal(|ui| {
                    if ui.add_enabled(readable > 0, egui::Button::new("取り込む")).on_hover_text("今のボックスは置き換えられます").clicked() {
                        let list = box_io::import_tsv(&self.import_text, &cols);
                        self.import_message = format!("{}匹を取り込みました", list.len());
                        shared.box_file.list = list;
                        shared.save_box();
                        self.results.clear();
                        self.import_text.clear();
                    } else if lines > 0 {
                        let text = format!("{lines}行中 {readable}匹を読み込めます");
                        if readable < lines {
                            ui.colored_label(ui.visuals().warn_fg_color, text).on_hover_text("読めない行は名前・Lv・食材・サブスキル・せいかくを確認するか、列番号設定を見直してください");
                        } else {
                            ui.label(text);
                        }
                    } else {
                        ui.label(&self.import_message);
                    }
                });
                egui::ScrollArea::vertical().id_salt("import").max_height(360.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.import_text).hint_text("スプレッドシート等からコピーした TSV を貼り付け").desired_width(f32::INFINITY).desired_rows(16).code_editor());
                });
            });
        });
        if !open {
            self.import_open = false;
        }
    }
}

/// 列番号の入力欄(本家と同じ項目)。変更があれば true
fn tsv_columns_ui(ui: &mut egui::Ui, cols: &mut TsvColumns) -> bool {
    cols.food_list.resize(3, None);
    cols.sub_skill_list.resize(5, None);
    let [f1, f2, f3] = &mut cols.food_list[..] else { unreachable!() };
    let [s1, s2, s3, s4, s5] = &mut cols.sub_skill_list[..] else { unreachable!() };
    let fields: [(&str, &mut Option<usize>); 20] = [
        ("名前", &mut cols.name),
        ("Lv", &mut cols.lv),
        ("スキルLv", &mut cols.skill_lv),
        ("食材ABC", &mut cols.food_abc),
        ("食材1", f1),
        ("食材2", f2),
        ("食材3", f3),
        ("サブスキル1", s1),
        ("サブスキル2", s2),
        ("サブスキル3", s3),
        ("サブスキル4", s4),
        ("サブスキル5", s5),
        ("せいかく", &mut cols.nature),
        ("色違い", &mut cols.shiny),
        ("固定", &mut cols.fix),
        ("睡眠時間", &mut cols.sleep_time),
        ("目標Lv", &mut cols.training),
        ("次Lv迄のExp", &mut cols.next_exp),
        ("メモ", &mut cols.memo),
        ("お気に入り", &mut cols.favorite),
    ];
    let mut changed = false;
    egui::Grid::new("tsv_columns").num_columns(10).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, (label, value)) in fields.into_iter().enumerate() {
            ui.label(label);
            let mut text = value.map(|v| v.to_string()).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut text).hint_text("なし").desired_width(36.0)).changed() {
                let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
                *value = digits.parse().ok().filter(|&v| v > 0);
                changed = true;
            }
            if i % 5 == 4 {
                ui.end_row();
            }
        }
    });
    changed
}
