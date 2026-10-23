//! 食材アイコンの見本の取り込み(スクショから追加と、設定画面の取り込み直しで使う)。
//!
//! ゲームのポケモンボックスの絞り込み(食材)画面には、全食材のアイコンが名前付きで並ぶ。
//! その画面をスマホから撮り、食材ごとの見本としてこの PC に保存する。

use eframe::egui;
use pokesle_ocr::engine::OcrEngine;
use pokesle_ocr::food::FoodSample;
use pokesle_ocr::google_vision::GoogleVision;
use pokesle_ocr::windows_ocr::WindowsOcr;

use crate::app::Shared;
use crate::job::Job;
use crate::storage::{self, OcrEngineChoice};

#[derive(Default)]
pub struct FoodSampleImport {
    job: Option<Job<Vec<FoodSample>>>,
    message: String,
}

impl FoodSampleImport {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub fn start(&mut self, ctx: &egui::Context, shared: &Shared) {
        let setting = shared.ocr_setting.clone();
        let usage = shared.vision_usage.clone();
        self.message.clear();
        self.job = Some(Job::spawn(ctx, move |progress| {
            progress.set(0.0, "スマホの画面を撮っています");
            let adb = pokesle_ocr::adb::find_adb(Some(&setting.adb_path)).ok_or(
                "adb.exe が見つかりません。設定画面で場所を指定するか、PATH に追加してください。".to_string(),
            )?;
            let png = pokesle_ocr::adb::capture_png(&adb, None)?;
            progress.set(0.5, "食材の名前を読み取っています");
            let engine: Box<dyn OcrEngine> = match setting.engine {
                OcrEngineChoice::Windows => Box::new(WindowsOcr::japanese()),
                OcrEngineChoice::Google => Box::new(GoogleVision::new(&setting.google_api_key).with_usage(usage)),
            };
            let line_list = engine.recognize(&png)?;
            let image = image::load_from_memory(&png).map_err(|e| format!("画像を読み込めませんでした: {e}"))?.to_rgb8();
            Ok(pokesle_ocr::food::samples_from_food_list_screen(&image, &line_list))
        }));
    }

    /// 取り込みが終わっていれば見本を保存する。1種類以上保存できたら true
    pub fn poll(&mut self, shared: &mut Shared) -> bool {
        let Some(job) = &mut self.job else { return false };
        let Some(result) = job.poll() else { return false };
        self.job = None;
        if shared.ocr_setting.engine == OcrEngineChoice::Google {
            let usage = shared.vision_usage.lock().map(|u| u.clone()).unwrap_or_default();
            if let Err(e) = storage::save_vision_usage(&usage) {
                shared.status = format!("利用回数を保存できませんでした: {e}");
            }
        }
        match result {
            Ok(sample_list) if sample_list.is_empty() => {
                self.message = "食材アイコンを見つけられませんでした。食材の一覧が表示されているか確認してください。".into();
                false
            }
            Ok(sample_list) => {
                let mut saved = 0;
                for sample in &sample_list {
                    match storage::save_food_sample(sample) {
                        Ok(()) => saved += 1,
                        Err(e) => self.message = format!("食材の見本を保存できませんでした: {e}"),
                    }
                }
                shared.status = format!("{saved} 種の食材の見本を取り込みました");
                saved > 0
            }
            Err(e) => {
                self.message = e;
                false
            }
        }
    }

    /// 進み具合と、失敗したときのメッセージ
    pub fn show(&self, ui: &mut egui::Ui) {
        if let Some(job) = &self.job {
            job.show_progress(ui);
        }
        if !self.message.is_empty() {
            ui.label(egui::RichText::new(&self.message).color(ui.visuals().error_fg_color));
        }
    }
}
