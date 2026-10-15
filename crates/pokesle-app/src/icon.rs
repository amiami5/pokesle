//! ウィンドウのアイコン(自作の icon.png。tools/app-icon/make.py で作る。メインウィンドウと別ウィンドウの小窓で共用する)

use std::sync::{Arc, OnceLock};

use eframe::egui::IconData;

static ICON: OnceLock<Option<Arc<IconData>>> = OnceLock::new();

pub fn window_icon() -> Option<Arc<IconData>> {
    ICON.get_or_init(|| {
        let img = image::load_from_memory_with_format(include_bytes!("../assets/icon.png"), image::ImageFormat::Png).ok()?.into_rgba8();
        let (width, height) = img.dimensions();
        Some(Arc::new(IconData { rgba: img.into_raw(), width, height }))
    })
    .clone()
}
