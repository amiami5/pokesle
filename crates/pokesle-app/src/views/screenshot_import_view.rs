//! スクショから追加(本家 screenshot-import.vue 相当)。
//!
//! ポケモンの詳細画面を、スマホから撮るか PNG をドロップして集め、1匹ぶんとして読み取る。
//! 詳細画面は縦に長いので、1匹につき複数枚(Lv・名前・食材が映る画面と、サブスキル・せいかくが映る画面)を使う。

use eframe::egui;
use pokesle_ocr::engine::OcrEngine;
use pokesle_ocr::google_vision::GoogleVision;
use pokesle_ocr::import::to_box_pokemon;
use pokesle_ocr::status::StatusResult;
use pokesle_ocr::windows_ocr::WindowsOcr;

use crate::app::Shared;
use crate::job::Job;
use crate::storage::{self, OcrEngineChoice, OcrSetting};
use crate::views::{food_icon, style};

struct Row {
    selected: bool,
    shiny: bool,
    result: StatusResult,
    /// 種族を読み取れなかったときに、候補から選んだもの
    chosen_name: Option<String>,
}

impl Row {
    fn error(&self) -> Option<String> {
        to_box_pokemon(&self.result, self.chosen_name.as_deref(), self.shiny).err()
    }
}

#[derive(Default)]
pub struct ScreenshotImportView {
    /// 読み取り待ちの画面(PNG)。1匹ぶん
    pending: Vec<Vec<u8>>,
    row_list: Vec<Row>,
    capture_job: Option<Job<Vec<u8>>>,
    read_job: Option<Job<StatusResult>>,
    message: String,
}

impl ScreenshotImportView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        self.poll_jobs(shared);
        self.take_dropped_files(ui.ctx());

        ui.heading("スクショから追加");
        ui.add_space(4.0);
        style::alert(
            ui,
            "β版です。誤読をする可能性があるので、結果を確認しながら使ってください。\n\
             ポケモンの詳細画面を、上から順に撮ります。1枚目は Lv・名前・食材・おてつだい時間が映る画面、\n\
             2枚目は下にスクロールして、サブスキルとせいかくが映る画面です。PNG ファイルをこの画面にドロップしても追加できます。",
        );
        ui.add_space(6.0);
        let engine_name = match shared.ocr_setting.engine {
            OcrEngineChoice::Windows => "Windows 標準 OCR",
            OcrEngineChoice::Google => "Google Cloud Vision",
        };
        ui.label(egui::RichText::new(format!("文字認識: {engine_name}(設定画面で変更できます)")).color(style::muted()));
        ui.add_space(6.0);

        let busy = self.capture_job.is_some() || self.read_job.is_some();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                if style::form_button(ui, "スマホから撮る", style::ButtonKind::Normal).clicked() {
                    self.start_capture(ui.ctx(), &shared.ocr_setting);
                }
                ui.label(format!("撮った画面: {} 枚", self.pending.len()));
                ui.add_enabled_ui(!self.pending.is_empty(), |ui| {
                    if style::form_button(ui, "この画面で読み取る", style::ButtonKind::Execute).clicked() {
                        self.start_read(ui.ctx(), shared);
                    }
                    if style::form_button(ui, "撮り直す", style::ButtonKind::Normal).clicked() {
                        self.pending.clear();
                        self.message.clear();
                    }
                });
            });
        });
        if let Some(job) = &self.capture_job {
            job.show_progress(ui);
        }
        if let Some(job) = &self.read_job {
            job.show_progress(ui);
        }
        if !self.message.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&self.message).color(ui.visuals().error_fg_color));
        }

        if self.row_list.is_empty() {
            return;
        }
        ui.add_space(10.0);
        self.table(ui);
        ui.add_space(8.0);
        self.add_button(ui, shared);
    }

    /// 終わった処理の結果を取り込む
    fn poll_jobs(&mut self, shared: &mut Shared) {
        if let Some(job) = &mut self.capture_job
            && let Some(result) = job.poll()
        {
            self.capture_job = None;
            match result {
                Ok(png) => {
                    self.pending.push(png);
                    self.message.clear();
                }
                Err(e) => self.message = e,
            }
        }
        if let Some(job) = &mut self.read_job
            && let Some(result) = job.poll()
        {
            self.read_job = None;
            // Google Cloud Vision を使った分の利用回数を保存する
            if shared.ocr_setting.engine == OcrEngineChoice::Google {
                let usage = shared.vision_usage.lock().map(|u| u.clone()).unwrap_or_default();
                if let Err(e) = storage::save_vision_usage(&usage) {
                    shared.status = format!("利用回数を保存できませんでした: {e}");
                }
            }
            match result {
                Ok(result) => {
                    let mut row = Row { selected: true, shiny: false, result, chosen_name: None };
                    row.selected = row.error().is_none();
                    self.row_list.push(row);
                    self.pending.clear();
                    self.message.clear();
                }
                Err(e) => self.message = e,
            }
        }
    }

    /// ウインドウにドロップされた PNG を、読み取り待ちに加える
    fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            match file.bytes() {
                Ok(bytes) if bytes.starts_with(&[0x89, b'P', b'N', b'G']) => self.pending.push(bytes),
                Ok(_) => self.message = "PNG 画像だけ追加できます。".into(),
                Err(e) => self.message = format!("ファイルを読み込めませんでした: {e}"),
            }
        }
    }

    fn start_capture(&mut self, ctx: &egui::Context, setting: &OcrSetting) {
        let adb_path = setting.adb_path.clone();
        self.capture_job = Some(Job::spawn(ctx, move |progress| {
            progress.set(0.0, "スマホの画面を撮っています");
            let adb = pokesle_ocr::adb::find_adb(Some(&adb_path)).ok_or(
                "adb.exe が見つかりません。設定画面で場所を指定するか、PATH に追加してください。".to_string(),
            )?;
            pokesle_ocr::adb::capture_png(&adb, None)
        }));
    }

    fn start_read(&mut self, ctx: &egui::Context, shared: &Shared) {
        let png_list = self.pending.clone();
        let setting = shared.ocr_setting.clone();
        let usage = shared.vision_usage.clone();
        self.read_job = Some(Job::spawn(ctx, move |progress| {
            progress.set(0.0, "画面の文字を読み取っています");
            let engine: Box<dyn OcrEngine> = match setting.engine {
                OcrEngineChoice::Windows => Box::new(WindowsOcr::japanese()),
                OcrEngineChoice::Google => Box::new(GoogleVision::new(&setting.google_api_key).with_usage(usage)),
            };
            pokesle_ocr::reader::read_pngs(engine.as_ref(), &png_list)
        }));
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let mut remove: Option<usize> = None;
        egui::Grid::new("screenshot_import_table").striped(true).num_columns(8).spacing([14.0, 8.0]).show(ui, |ui| {
            for header in ["", "ポケモン", "Lv", "食材構成", "スキル\nLv", "サブスキル", "せいかく", ""] {
                style::th(ui, header);
            }
            ui.end_row();

            for (index, row) in self.row_list.iter_mut().enumerate() {
                let error = row.error();
                ui.add_enabled(error.is_none(), egui::Checkbox::without_text(&mut row.selected));

                ui.vertical(|ui| {
                    let name = row.chosen_name.as_deref().or(row.result.pokemon_name.as_deref());
                    match name {
                        Some(name) => {
                            ui.strong(name);
                        }
                        None => {
                            ui.strong("判定できませんでした");
                        }
                    }
                    // 種族を1つに決められなかったときは、候補から選んでもらう
                    if row.result.pokemon_name.is_none() && !row.result.pokemon_candidate_list.is_empty() {
                        let mut selected = row.chosen_name.clone().unwrap_or_default();
                        let candidate_list = row.result.pokemon_candidate_list.clone();
                        egui::ComboBox::from_id_salt(("import_candidate", index))
                            .selected_text(if selected.is_empty() { "候補から選ぶ" } else { selected.as_str() })
                            .show_ui(ui, |ui| {
                                for candidate in candidate_list {
                                    ui.selectable_value(&mut selected, candidate.clone(), candidate);
                                }
                            });
                        if !selected.is_empty() && row.chosen_name.as_deref() != Some(selected.as_str()) {
                            row.chosen_name = Some(selected);
                        }
                    }
                    ui.checkbox(&mut row.shiny, "色違い");
                    if let Some(error) = &error {
                        ui.label(egui::RichText::new(error).small().color(ui.visuals().error_fg_color));
                    }
                });

                ui.label(row.result.lv.map_or("―".to_string(), |lv| lv.to_string()));

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for food in &row.result.food_list {
                        match food {
                            Some(food) => {
                                food_icon::food_icon(ui, food, 24.0);
                            }
                            None => {
                                ui.label("―");
                            }
                        }
                    }
                });

                ui.label(row.result.main_skill_lv.map_or("―".to_string(), |lv| lv.to_string()));

                ui.vertical(|ui| {
                    for sub_skill in &row.result.sub_skill_list {
                        ui.label(egui::RichText::new(sub_skill).small());
                    }
                });

                match &row.result.nature {
                    Some(nature) => crate::views::nature_label(ui, nature),
                    None => {
                        ui.label("―");
                    }
                }

                if ui.small_button("削除").clicked() {
                    remove = Some(index);
                }
                ui.end_row();
            }
        });
        if let Some(index) = remove {
            self.row_list.remove(index);
        }
    }

    fn add_button(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let count = self.row_list.iter().filter(|r| r.selected && r.error().is_none()).count();
        ui.add_enabled_ui(count > 0, |ui| {
            if style::form_button(ui, &format!("選択した {count} 匹をボックスに追加"), style::ButtonKind::Execute).clicked() {
                let mut added = 0;
                self.row_list.retain(|row| {
                    if !row.selected {
                        return true;
                    }
                    match to_box_pokemon(&row.result, row.chosen_name.as_deref(), row.shiny) {
                        Ok(pokemon) => {
                            shared.box_file.list.push(pokemon);
                            added += 1;
                            false
                        }
                        Err(_) => true,
                    }
                });
                if added > 0 {
                    shared.save_box();
                    shared.status = format!("{added} 匹をボックスに追加しました");
                }
            }
        });
    }
}
