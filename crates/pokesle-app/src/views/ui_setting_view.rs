//! 見た目の設定(フォント・文字の大きさ)。本家にはない、このアプリ独自の画面

use eframe::egui;

use crate::fonts::{self, FontChoice};
use crate::job::Job;
use crate::storage::{self, UiSetting};

#[derive(Default)]
pub struct UiSettingView {
    /// インストール済みの日本語フォント(初めて開いたときに探す)
    fonts: Option<Vec<FontChoice>>,
    job: Option<Job<Vec<FontChoice>>>,
}

/// 起動時に設定を反映する
pub fn apply(ctx: &egui::Context, setting: &UiSetting) {
    fonts::install(ctx, setting.font.as_ref());
    ctx.set_zoom_factor(setting.zoom);
}

impl UiSettingView {
    pub fn ui(&mut self, ui: &mut egui::Ui, setting: &mut UiSetting, status: &mut String) {
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
        ui.label(egui::RichText::new("文字の大きさは画面全体の表示倍率として反映します。").color(crate::views::style::MUTED));

        if font_changed {
            fonts::install(ui.ctx(), setting.font.as_ref());
        }
        if zoom_changed {
            ui.ctx().set_zoom_factor(setting.zoom);
        }
        if (font_changed || zoom_changed)
            && let Err(e) = storage::save_ui_setting(setting)
        {
            *status = format!("設定を保存できませんでした: {e}");
        }
    }
}
