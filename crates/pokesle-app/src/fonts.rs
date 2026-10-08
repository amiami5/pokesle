//! 日本語フォント。再配布を避けるため Windows 標準のフォントを実行時に読み込む。

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// 優先順に試すフォント(ファイル名, TTC 内の番号)
const CANDIDATES: [(&str, u32); 4] = [
    ("YuGothM.ttc", 0),     // 游ゴシック Medium
    ("BIZ-UDGothicR.ttc", 0), // BIZ UDゴシック
    ("meiryo.ttc", 0),      // メイリオ
    ("msgothic.ttc", 0),    // MS ゴシック
];

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::env::var("WINDIR").map(|w| format!("{w}\\Fonts")).unwrap_or_else(|_| "C:\\Windows\\Fonts".into());
    for (file, index) in CANDIDATES {
        if let Ok(bytes) = std::fs::read(format!("{dir}\\{file}")) {
            let mut data = FontData::from_owned(bytes);
            data.index = index;
            // 游ゴシック等は行の高さに対して字面が上寄りに描かれるので、下げて枠の上下中央に合わせる(実測で 0.2)
            data.tweak.y_offset_factor = 0.2;
            fonts.font_data.insert("jp".into(), Arc::new(data));
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(family).or_default().insert(0, "jp".into());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}
