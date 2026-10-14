//! 見た目の設定(フォント・文字の大きさ)。本家にはない、このアプリ独自の画面

use eframe::egui;

use pokesle_core::browser_import::{self, BrowserImport};

use crate::app::Shared;
use crate::fonts::{self, FontChoice};
use crate::job::Job;
use crate::storage::{self, ThemeSetting, UiSetting};

#[derive(Default)]
pub struct UiSettingView {
    /// インストール済みの日本語フォント(初めて開いたときに探す)
    fonts: Option<Vec<FontChoice>>,
    job: Option<Job<Vec<FontChoice>>>,
    /// ブラウザ版からの移行データ(貼り付けた文章と、その読み取り結果)
    import_text: String,
    import_parsed: Option<Result<BrowserImport, String>>,
    import_config: bool,
    import_box: bool,
}

/// 起動時に設定を反映する
pub fn apply(ctx: &egui::Context, setting: &UiSetting) {
    fonts::install(ctx, setting.font.as_ref());
    ctx.set_zoom_factor(setting.zoom);
}

impl UiSettingView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let setting = &mut shared.ui_setting;
        if self.fonts.is_none() && self.job.is_none() {
            self.job = Some(Job::spawn(ui.ctx(), |_| Ok(fonts::list_japanese())));
        }
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.fonts = Some(r.unwrap_or_default());
            self.job = None;
        }

        let mut font_changed = false;
        let mut zoom_changed = false;
        let mut theme_changed = false;
        ui.heading("設定");
        ui.add_space(6.0);
        egui::Grid::new("ui_setting").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            ui.label("フォント");
            match &self.fonts {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("フォントを探しています…");
                    });
                }
                Some(list) => {
                    let selected = setting.font.as_ref().map_or("既定(游ゴシック)", |f| f.name.as_str());
                    egui::ComboBox::from_id_salt("ui_setting_font").selected_text(selected).width(280.0).height(400.0).show_ui(ui, |ui| {
                        if ui.selectable_label(setting.font.is_none(), "既定(游ゴシック)").clicked() {
                            setting.font = None;
                            font_changed = true;
                        }
                        for f in list {
                            if ui.selectable_label(setting.font.as_ref() == Some(f), &f.name).clicked() {
                                setting.font = Some(f.clone());
                                font_changed = true;
                            }
                        }
                    });
                }
            }
            ui.end_row();

            ui.label("配色");
            ui.horizontal(|ui| {
                for (v, l) in [(ThemeSetting::Light, "ライト"), (ThemeSetting::Dark, "ダーク"), (ThemeSetting::System, "OS に合わせる")] {
                    if ui.radio_value(&mut setting.theme, v, l).changed() {
                        theme_changed = true;
                    }
                }
            });
            ui.end_row();

            ui.label("文字の大きさ");
            ui.horizontal(|ui| {
                let mut percent = (setting.zoom * 100.0).round();
                let r = ui.add(egui::Slider::new(&mut percent, 70.0..=200.0).step_by(5.0).suffix("%"));
                // ドラッグ中に倍率を変えるとスライダーが動いてしまうので、離したときに反映する
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    zoom_changed = true;
                }
                setting.zoom = percent / 100.0;
                if ui.button("100%に戻す").clicked() {
                    setting.zoom = 1.0;
                    zoom_changed = true;
                }
            });
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.label(egui::RichText::new("文字の大きさは画面全体の表示倍率として反映します。").color(crate::views::style::muted()));

        ui.add_space(16.0);
        ui.separator();
        self.import_section(ui, shared);
        let Shared { ui_setting: setting, status, .. } = shared;

        if font_changed {
            fonts::install(ui.ctx(), setting.font.as_ref());
        }
        if theme_changed {
            crate::views::style::apply(ui.ctx(), setting.theme.is_dark(ui.ctx()));
        }
        if zoom_changed {
            ui.ctx().set_zoom_factor(setting.zoom);
        }
        if (font_changed || zoom_changed || theme_changed)
            && let Err(e) = storage::save_ui_setting(setting)
        {
            *status = format!("設定を保存できませんでした: {e}");
        }
    }

    /// 本家ブラウザ版からの移行(設定・ボックス)
    fn import_section(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        use crate::views::style;
        ui.add_space(6.0);
        ui.heading("ブラウザ版からの移行");
        ui.add_space(4.0);
        ui.label("本家(ブラウザ版)を開き、F12 → コンソールに下のコマンドを貼り付けて実行すると、設定とボックスがクリップボードにコピーされます。それを下の欄に貼り付けてください。");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut browser_import::CONSOLE_COMMAND.to_string()).desired_width(520.0));
            if ui.button("コマンドをコピー").clicked() {
                ui.ctx().copy_text(browser_import::CONSOLE_COMMAND.to_string());
            }
        });
        ui.add_space(4.0);
        let r = ui.add(egui::TextEdit::multiline(&mut self.import_text).desired_rows(4).desired_width(f32::INFINITY).hint_text("ここに貼り付け"));
        if r.changed() {
            self.import_parsed = (!self.import_text.trim().is_empty()).then(|| browser_import::parse(&self.import_text));
            if let Some(Ok(p)) = &self.import_parsed {
                self.import_config = p.config.is_some();
                self.import_box = p.box_file.is_some();
            }
        }
        match &self.import_parsed {
            None => {}
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            Some(Ok(p)) => {
                if p.config.is_some() {
                    ui.checkbox(&mut self.import_config, "設定を置き換える");
                }
                if let Some(b) = &p.box_file {
                    ui.checkbox(&mut self.import_box, format!("ボックスを置き換える({}匹 → 現在 {}匹 は消えます)", b.list.len(), shared.box_file.list.len()));
                }
                if style::form_button(ui, "取り込む", style::ButtonKind::Execute).clicked() {
                    let mut done = Vec::new();
                    let mut p = self.import_parsed.take().and_then(Result::ok).unwrap_or_default();
                    if self.import_config
                        && let Some(c) = p.config.take()
                    {
                        shared.config = c;
                        shared.save_config();
                        // 保存済みの厳選テーブルは設定と合わなくなることがあるが、使えるかは画面側で判定される
                        done.push("設定");
                    }
                    if self.import_box
                        && let Some(b) = p.box_file.take()
                    {
                        shared.box_file = b;
                        shared.save_box();
                        done.push("ボックス");
                    }
                    shared.status = format!("ブラウザ版から取り込みました: {}", done.join("・"));
                    self.import_text.clear();
                }
            }
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new("取り込むと現在の内容は置き換わります。厳選テーブル(ブラウザの IndexedDB に保存されています)は移行できないので、取り込み後に基準生成で作り直してください。").color(style::muted()));
    }
}
