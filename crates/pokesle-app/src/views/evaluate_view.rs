//! 基準生成画面(本家 pages/setting.vue): 厳選テーブルの作成

use eframe::egui;
use pokesle_core::data::master;
use pokesle_core::evaluate_table::{simulate_evaluate_table, EvaluateTable};

use super::style::{self, ButtonKind};
use crate::app::Shared;
use crate::job::Job;
use crate::storage;

#[derive(Default)]
pub struct EvaluateView {
    job: Option<Job<EvaluateTable>>,
    done: bool,
    error: String,
}

impl EvaluateView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            match r {
                Ok(table) => {
                    if let Err(e) = storage::save_evaluate_table(&table) {
                        self.error = format!("厳選テーブルを保存できませんでした: {e}");
                    }
                    shared.evaluate_table = Some(table);
                    self.done = true;
                }
                Err(e) => self.error = e,
            }
        }

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(1200.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            match &shared.evaluate_table {
                Some(t) if t.version != master().version.evaluate => {
                    style::danger(ui, "初めての利用、もしくはシステムがアップデートされたため厳選テーブルがクリアされました。")
                }
                None => style::danger(ui, "初めての利用、もしくはシステムがアップデートされたため厳選テーブルがクリアされました。"),
                Some(_) if shared.usable_evaluate_table().is_none() => {
                    style::danger(ui, "睡眠時間、チェック回数が変更されたため厳選テーブルがクリアされました。")
                }
                _ => {}
            }
            style::alert(
                ui,
                "一部を除く全ポケモンの厳選テーブルを計算するページです。\n計算しておくと、ボックス画面などで厳選度が表示されるようになります。\n※ダークライやミュウは食材の組合せが512通りあり計算量が多いため対象外です。これらの特別なポケモンは簡易診断で見てください。",
            );
            ui.label(egui::RichText::new("厳選基準の詳細設定(本家の EvaluateSetting)は未対応です。現在は初期設定(または取り込んだ設定)で計算します。").color(style::MUTED));

            if self.done {
                style::alert(ui, "厳選情報の計算が完了しました。ボックス画面から厳選情報を確認できます。");
            }
            if !self.error.is_empty() {
                style::danger(ui, &self.error);
            }
            match &self.job {
                Some(job) => job.show_progress(ui),
                None => {
                    if style::form_button(ui, "設定を保存して厳選情報を計算する", ButtonKind::Execute).clicked() {
                        shared.save_config();
                        self.done = false;
                        self.error.clear();
                        let config = shared.config.clone();
                        self.job = Some(Job::spawn(ui.ctx(), move |p| simulate_evaluate_table(&config, &p)));
                    }
                }
            }
        });
    }
}
