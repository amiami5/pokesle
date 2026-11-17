//! スクショから追加(本家 screenshot-import.vue 相当)。
//!
//! ポケモンの詳細画面を、スマホから撮るか PNG をドロップして集め、1匹ぶんとして読み取る。
//! 詳細画面は縦に長いので、1匹につき複数枚(Lv・名前・食材が映る画面と、サブスキル・せいかくが映る画面)を使う。

use eframe::egui;
use pokesle_ocr::engine::OcrEngine;
use pokesle_ocr::google_vision::GoogleVision;
use pokesle_ocr::import::to_box_pokemon;
use pokesle_ocr::reader::ReadResult;
use pokesle_ocr::status::StatusResult;
use pokesle_ocr::windows_ocr::WindowsOcr;

use crate::app::Shared;
use crate::job::Job;
use crate::storage::{self, OcrEngineChoice, OcrSetting};
use crate::views::pokemon_edit::{EditEnv, EditResult, PokemonEdit};
use crate::views::food_sample_import::FoodSampleImport;
use crate::views::{food_icon, style};
use pokesle_core::sim_pokemon::BoxPokemon;

struct Row {
    selected: bool,
    shiny: bool,
    result: StatusResult,
    /// 種族を読み取れなかったときに、候補から選んだもの
    chosen_name: Option<String>,
    /// 編集画面で直した内容。あれば読み取り結果より優先する
    edited: Option<BoxPokemon>,
    /// 食材欄の各枠のアイコン。ボックスに追加したとき、確定した食材の見本として保存する
    food_icon_list: Vec<image::RgbaImage>,
}

impl Row {
    /// ボックスに追加する内容。読み取り結果が足りず、直してもいなければエラー
    fn pokemon(&self) -> Result<BoxPokemon, String> {
        match &self.edited {
            Some(pokemon) => Ok(pokemon.clone()),
            None => to_box_pokemon(&self.result, self.chosen_name.as_deref(), self.shiny),
        }
    }

    fn error(&self) -> Option<String> {
        self.pokemon().err()
    }

    /// 編集画面の初期値。読み取れなかった項目は、仮の値(後で直してもらう)で埋める
    fn draft(&self) -> BoxPokemon {
        if let Ok(pokemon) = self.pokemon() {
            return pokemon;
        }
        let m = pokesle_core::data::master();
        let name = self.chosen_name.clone().or(self.result.pokemon_name.clone());
        let base = name.as_deref().and_then(|n| m.pokemon(n)).unwrap_or(&m.pokemon[0]);
        let mut food_list: Vec<String> = self.result.food_list.iter().flatten().cloned().collect();
        food_list.resize(3, base.food_name_list[0].clone());
        let mut sub_skill_list = self.result.sub_skill_list.clone();
        for sub_skill in &m.sub_skills {
            if sub_skill_list.len() >= 5 {
                break;
            }
            if !sub_skill_list.contains(&sub_skill.name) {
                sub_skill_list.push(sub_skill.name.clone());
            }
        }
        BoxPokemon {
            name: base.name.clone(),
            lv: f64::from(self.result.lv.unwrap_or(10)),
            skill_lv: Some(f64::from(self.result.main_skill_lv.unwrap_or(1))),
            food_list,
            sub_skill_list,
            nature: self.result.nature.clone().unwrap_or_else(|| "まじめ".into()),
            shiny: self.shiny,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Duplicate {
    /// 同じ個体の可能性がある(種族・せいかくが同じで、サブスキルが一方がもう一方に含まれる)
    Possible,
    /// Lv・食材・サブスキルまで全く同じ
    Exact,
}

fn compare(a: &BoxPokemon, b: &BoxPokemon) -> Option<Duplicate> {
    if a.name != b.name || a.nature != b.nature {
        return None;
    }
    let sorted = |p: &BoxPokemon| {
        let mut list = p.sub_skill_list.clone();
        list.sort();
        list
    };
    let (sa, sb) = (sorted(a), sorted(b));
    if sa == sb && a.lv == b.lv && a.food_list == b.food_list {
        return Some(Duplicate::Exact);
    }
    let subset = |x: &[String], y: &[String]| x.iter().all(|s| y.contains(s));
    (subset(&sa, &sb) || subset(&sb, &sa)).then_some(Duplicate::Possible)
}

/// ボックス内で、他のポケモンと同じ可能性があるもの(ボックス位置ごと)
pub(super) fn box_duplicate_list(list: &[BoxPokemon]) -> Vec<bool> {
    let mut group: std::collections::HashMap<(&str, &str), Vec<usize>> = std::collections::HashMap::new();
    for (i, p) in list.iter().enumerate() {
        group.entry((p.name.as_str(), p.nature.as_str())).or_default().push(i);
    }
    let mut result = vec![false; list.len()];
    for index_list in group.values().filter(|l| l.len() > 1) {
        for (n, &a) in index_list.iter().enumerate() {
            for &b in &index_list[n + 1..] {
                if compare(&list[a], &list[b]).is_some() {
                    result[a] = true;
                    result[b] = true;
                }
            }
        }
    }
    result
}

/// 行ごとの重複の警告。ボックス内と、一覧の他の行を調べる
fn duplicate_warning(index: usize, pokemon_list: &[Option<BoxPokemon>], box_list: &[BoxPokemon]) -> Option<(Duplicate, String)> {
    let target = pokemon_list[index].as_ref()?;
    let mut found: Option<(Duplicate, String)> = None;
    let mut note = |kind: Duplicate, place: String| {
        if found.as_ref().is_none_or(|(k, _)| kind > *k) {
            found = Some((kind, place));
        }
    };
    for (i, other) in box_list.iter().enumerate() {
        if let Some(kind) = compare(target, other) {
            note(kind, format!("ボックスの {} 番目", i + 1));
        }
    }
    for (i, other) in pokemon_list.iter().enumerate() {
        if i == index {
            continue;
        }
        if let Some(kind) = other.as_ref().and_then(|o| compare(target, o)) {
            note(kind, "この一覧の別の行".into());
        }
    }
    found
}

#[derive(Default)]
pub struct ScreenshotImportView {
    /// 読み取り待ちの画面(PNG)。1匹ぶん
    pending: Vec<Vec<u8>>,
    row_list: Vec<Row>,
    capture_job: Option<Job<Vec<u8>>>,
    read_job: Option<Job<ReadResult>>,
    /// 食材一覧の画面から見本を取り込む処理
    sample_import: FoodSampleImport,
    /// 全食材の見本が揃っているか(揃っていれば取り込みボタンを出さない)。None は未確認
    samples_complete: Option<bool>,
    /// 2枚そろったので、自動で読み取りを始める
    auto_read: bool,
    message: String,
    /// 編集中の行(行の位置, 編集画面)
    edit: Option<(usize, PokemonEdit)>,
}

impl ScreenshotImportView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        self.poll_jobs(shared);
        self.take_dropped_files(ui.ctx());
        if std::mem::take(&mut self.auto_read) && self.read_job.is_none() {
            self.start_read(ui.ctx(), shared);
        }

        ui.heading("スクショから追加");
        ui.add_space(4.0);
        style::alert(
            ui,
            "β版です。誤読をする可能性があるので、結果を確認しながら使ってください。\n\
             ポケモンの詳細画面を、上から順に撮ります。1枚目は Lv・名前・食材・おてつだい時間が映る画面、\n\
             2枚目は下にスクロールして、サブスキルとせいかくが映る画面です。2枚そろうと自動で読み取ります。
             PNG ファイルをこの画面にドロップしても追加できます。",
        );
        ui.add_space(6.0);
        let engine_name = match shared.ocr_setting.engine {
            OcrEngineChoice::Windows => "Windows 標準 OCR",
            OcrEngineChoice::Google => "Google Cloud Vision",
        };
        ui.label(egui::RichText::new(format!("文字認識: {engine_name}(設定画面で変更できます)")).color(style::muted()));
        ui.add_space(6.0);

        let busy = self.capture_job.is_some() || self.read_job.is_some() || self.sample_import.busy();
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
        // 食材の見本が揃うまでは、取り込みボタンを出す(取り込み直しは設定画面から)
        let samples_complete = *self.samples_complete.get_or_insert_with(storage::has_all_food_samples);
        if !samples_complete || self.sample_import.busy() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if style::form_button(ui, "食材一覧から見本を取り込む", style::ButtonKind::Normal).clicked() {
                        self.sample_import.start(ui.ctx(), shared);
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "最初に1度、ゲームのポケモンボックスで絞り込み(食材)の画面を出して押してください。食材アイコンの見本を取り込み、食材の判定に使います。",
                    )
                    .color(style::muted()),
                );
            });
        }
        self.sample_import.show(ui);
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

        self.edit_popup(ui.ctx());
        if self.row_list.is_empty() {
            return;
        }
        ui.add_space(10.0);
        self.table(ui, shared);
        ui.add_space(8.0);
        self.add_button(ui, shared);
        ui.add_space(4.0);
        if ui.button("取り込み結果をすべて削除").clicked() {
            self.row_list.clear();
            self.edit = None;
        }
    }

    /// 終わった処理の結果を取り込む
    fn poll_jobs(&mut self, shared: &mut Shared) {
        if self.sample_import.poll(shared) {
            self.samples_complete = None;
        }
        if let Some(job) = &mut self.capture_job
            && let Some(result) = job.poll()
        {
            self.capture_job = None;
            match result {
                Ok(png) => {
                    self.pending.push(png);
                    self.message.clear();
                    // 1枚目(Lv・名前・食材)と2枚目(サブスキル・せいかく)がそろったら読み取る
                    self.auto_read = self.pending.len() >= 2;
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
                Ok(read) => {
                    // 種族だけで食材が決まる枠(1枠目など)のアイコンは、確実な見本として保存する
                    for sample in pokesle_ocr::reader::certain_food_samples(&read) {
                        if let Err(e) = storage::save_food_sample(&sample) {
                            shared.status = format!("食材の見本を保存できませんでした: {e}");
                        }
                    }
                    let mut row = Row {
                        selected: true,
                        shiny: false,
                        result: read.status,
                        chosen_name: None,
                        edited: None,
                        food_icon_list: read.food_icon_list,
                    };
                    row.selected = row.error().is_none();
                    // 最新の読み取りを一番上に出す。編集中の行は、1つ下にずれた位置を指し直す
                    self.row_list.insert(0, row);
                    // 重複の警告が出る行は、チェックを外しておく
                    let pokemon_list: Vec<Option<BoxPokemon>> = self.row_list.iter().map(|r| r.pokemon().ok()).collect();
                    if duplicate_warning(0, &pokemon_list, &shared.box_file.list).is_some() {
                        self.row_list[0].selected = false;
                    }
                    if let Some((index, _)) = &mut self.edit {
                        *index += 1;
                    }
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
        if !dropped.is_empty() && self.pending.len() + dropped.len() >= 2 {
            self.auto_read = true;
        }
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
            // 利用者が確定した食材の見本があれば、食材の判定に使う
            let sample_list = storage::load_food_samples();
            pokesle_ocr::reader::read_pngs(engine.as_ref(), &png_list, &sample_list)
        }));
    }

    /// 編集画面を出し、保存されたら行に反映する
    fn edit_popup(&mut self, ctx: &egui::Context) {
        let Some((index, edit)) = &mut self.edit else { return };
        let index = *index;
        match edit.ui(ctx) {
            EditResult::None => {}
            EditResult::Cancel => self.edit = None,
            EditResult::Save(_, pokemon) => {
                if let Some(row) = self.row_list.get_mut(index) {
                    row.shiny = pokemon.shiny;
                    row.edited = Some(*pokemon);
                    row.selected = true;
                }
                self.edit = None;
            }
        }
    }

    fn start_edit(&mut self, index: usize, shared: &Shared) {
        let Some(row) = self.row_list.get(index) else { return };
        let env = EditEnv {
            config: shared.sim_config(),
            box_list: shared.box_file.list.clone(),
            table: shared.usable_evaluate_table().cloned(),
        };
        self.edit = Some((index, PokemonEdit::new(None, Some(row.draft())).with_env(env)));
    }

    fn table(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let mut remove: Option<usize> = None;
        let mut edit: Option<usize> = None;
        egui::Grid::new("screenshot_import_table").striped(true).num_columns(9).spacing([14.0, 8.0]).show(ui, |ui| {
            for header in ["", "ポケモン", "Lv", "食材構成", "スキル\nLv", "サブスキル", "せいかく", "", ""] {
                style::th(ui, header);
            }
            ui.end_row();

            let pokemon_list: Vec<Option<BoxPokemon>> = self.row_list.iter().map(|r| r.pokemon().ok()).collect();
            for (index, row) in self.row_list.iter_mut().enumerate() {
                let error = row.error();
                let duplicate = duplicate_warning(index, &pokemon_list, &shared.box_file.list);
                ui.add_enabled(error.is_none(), egui::Checkbox::without_text(&mut row.selected));

                // 編集画面で直していれば、その内容を表示する
                let edited = row.edited.clone();
                ui.vertical(|ui| {
                    let name = match &edited {
                        Some(pokemon) => Some(pokemon.name.as_str()),
                        None => row.chosen_name.as_deref().or(row.result.pokemon_name.as_deref()),
                    };
                    match name {
                        Some(name) => {
                            ui.strong(name);
                        }
                        None => {
                            ui.strong("判定できませんでした");
                        }
                    }
                    // 種族を1つに決められなかったときは、候補から選んでもらう
                    if edited.is_none() && row.result.pokemon_name.is_none() && !row.result.pokemon_candidate_list.is_empty() {
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
                    if ui.checkbox(&mut row.shiny, "色違い").changed()
                        && let Some(pokemon) = &mut row.edited
                    {
                        pokemon.shiny = row.shiny;
                    }
                    if edited.is_some() {
                        ui.label(egui::RichText::new("編集済み").small().color(style::muted()));
                    }
                    if let Some(error) = &error {
                        ui.label(egui::RichText::new(error).small().color(ui.visuals().error_fg_color));
                    }
                    if let Some((kind, place)) = &duplicate {
                        let text = match kind {
                            Duplicate::Exact => format!("⚠ {place}と全く同じです"),
                            Duplicate::Possible => format!("⚠ {place}と同じ個体かもしれません"),
                        };
                        ui.label(egui::RichText::new(text).small().color(ui.visuals().warn_fg_color));
                    }
                });

                let lv = edited.as_ref().map(|p| p.lv).or(row.result.lv.map(f64::from));
                ui.label(lv.map_or("―".to_string(), style::js_num));

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let food_list: Vec<Option<String>> = match &edited {
                        Some(pokemon) => pokemon.food_list.iter().cloned().map(Some).collect(),
                        None => row.result.food_list.clone(),
                    };
                    for food in &food_list {
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

                let skill_lv = match &edited {
                    Some(pokemon) => pokemon.skill_lv,
                    None => row.result.main_skill_lv.map(f64::from),
                };
                ui.label(skill_lv.map_or("―".to_string(), style::js_num));

                ui.vertical(|ui| {
                    let sub_skill_list = edited.as_ref().map_or(&row.result.sub_skill_list, |p| &p.sub_skill_list);
                    for sub_skill in sub_skill_list {
                        ui.label(egui::RichText::new(sub_skill).small());
                    }
                });

                let nature = edited.as_ref().map(|p| p.nature.clone()).or(row.result.nature.clone());
                match &nature {
                    Some(nature) => crate::views::nature_label(ui, nature),
                    None => {
                        ui.label("―");
                    }
                }

                if ui.small_button("編集").clicked() {
                    edit = Some(index);
                }
                if ui.small_button("削除").clicked() {
                    remove = Some(index);
                }
                ui.end_row();
            }
        });
        if let Some(index) = edit {
            self.start_edit(index, shared);
        }
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
                    match row.pokemon() {
                        Ok(pokemon) => {
                            // 確定した食材を、次の読み取りの見本にする
                            for (slot, (icon, food)) in row.food_icon_list.iter().zip(&pokemon.food_list).enumerate() {
                                let sample = pokesle_ocr::food::FoodSample {
                                    name: food.clone(),
                                    faded: pokesle_ocr::food::food_slot_locked(pokemon.lv as u32, slot),
                                    icon: icon.clone(),
                                };
                                if let Err(e) = storage::save_food_sample(&sample) {
                                    shared.status = format!("食材の見本を保存できませんでした: {e}");
                                }
                            }
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
