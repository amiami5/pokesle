//! 時間のかかる計算をバックグラウンドで実行する。

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use eframe::egui;

#[derive(Clone, Default)]
pub struct Progress {
    inner: Arc<Mutex<(f64, String)>>,
}

impl Progress {
    pub fn set(&self, value: f64, message: &str) {
        if let Ok(mut p) = self.inner.lock() {
            p.0 = value;
            if !message.is_empty() {
                p.1 = message.to_string();
            }
        }
    }

    pub fn get(&self) -> (f64, String) {
        self.inner.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

impl pokesle_core::evaluate_table::Progress for Progress {
    fn set(&self, value: f64, message: &str) {
        Progress::set(self, value, message);
    }
}

pub struct Job<T> {
    pub progress: Progress,
    handle: Option<JoinHandle<Result<T, String>>>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(ctx: &egui::Context, f: impl FnOnce(Progress) -> Result<T, String> + Send + 'static) -> Job<T> {
        let progress = Progress::default();
        let p = progress.clone();
        let ctx = ctx.clone();
        let handle = std::thread::spawn(move || {
            let r = f(p);
            ctx.request_repaint();
            r
        });
        Job { progress, handle: Some(handle) }
    }

    /// 終わっていれば結果を取り出す
    pub fn poll(&mut self) -> Option<Result<T, String>> {
        if self.handle.as_ref().is_some_and(|h| h.is_finished()) {
            let h = self.handle.take()?;
            return Some(h.join().unwrap_or_else(|_| Err("計算中にエラーが発生しました".into())));
        }
        None
    }

    pub fn show_progress(&self, ui: &mut egui::Ui) {
        let (value, message) = self.progress.get();
        ui.horizontal(|ui| {
            ui.spinner();
            ui.add(egui::ProgressBar::new(value as f32).show_percentage().desired_width(240.0));
            ui.label(message);
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
}
