//! 日本語フォント。再配布を避けるため Windows にインストール済みのフォントを実行時に読み込む。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use serde::{Deserialize, Serialize};
use skrifa::{MetadataProvider, raw::FileRef, string::StringId};

/// 既定で優先順に試すフォント(ファイル名, TTC 内の番号)
const CANDIDATES: [(&str, u32); 4] = [
    ("YuGothM.ttc", 0),     // 游ゴシック Medium
    ("BIZ-UDGothicR.ttc", 0), // BIZ UDゴシック
    ("meiryo.ttc", 0),      // メイリオ
    ("msgothic.ttc", 0),    // MS ゴシック
];

/// 設定で選んだフォント
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FontChoice {
    pub name: String,
    pub path: PathBuf,
    pub index: u32,
}

fn system_dir() -> PathBuf {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    Path::new(&windir).join("Fonts")
}

/// フォントのある場所(全ユーザー用と、ユーザーごとのインストール先)
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![system_dir()];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(Path::new(&local).join("Microsoft\\Windows\\Fonts"));
    }
    dirs
}

/// 選んだフォントを先頭に、既定のフォントを文字が欠けたときの代わりに入れる
pub fn install(ctx: &egui::Context, choice: Option<&FontChoice>) {
    let mut fonts = FontDefinitions::default();
    let chosen = choice.and_then(|c| std::fs::read(&c.path).ok().map(|b| (b, c.index)));
    let fallback = CANDIDATES.iter().find_map(|&(file, index)| std::fs::read(system_dir().join(file)).ok().map(|b| (b, index)));
    // 後から先頭に入れるので、優先度の低い順に並べる
    for (key, font) in [("jp_fallback", fallback), ("jp", chosen)] {
        let Some((bytes, index)) = font else { continue };
        let y_offset = y_offset_factor(&bytes, index);
        let mut data = FontData::from_owned(bytes);
        data.index = index;
        data.tweak.y_offset_factor = y_offset;
        fonts.font_data.insert(key.into(), Arc::new(data));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().insert(0, key.into());
        }
    }
    ctx.set_fonts(fonts);
}

/// 字面を行の上下中央に合わせるための下げ幅(文字の大きさに対する割合)。
/// egui は行の高さに行間(line gap)を含めつつ字面を上端から描くため、游ゴシックのように行間が大きいフォントは上寄りになる。
/// 全角の字面(ベースラインの 0.12 下から 0.88 上)の中央と行の中央の差を、
/// 游ゴシックで実測した見た目のちょうど良い値(0.2)に合うよう 2/3 倍して使う
fn y_offset_factor(bytes: &[u8], index: u32) -> f32 {
    let Ok(font) = skrifa::FontRef::from_index(bytes, index) else { return 0.0 };
    let m = font.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
    let em = m.units_per_em as f32;
    if em <= 0.0 {
        return 0.0;
    }
    let (ascent, descent, gap) = (m.ascent / em, m.descent / em, m.leading / em);
    let diff = (ascent - descent + gap) / 2.0 - (ascent - 0.38);
    (diff * 2.0 / 3.0).clamp(-0.3, 0.4)
}

/// インストール済みのフォントのうち、日本語(かな・漢字)を含むもの。名前順
pub fn list_japanese() -> Vec<FontChoice> {
    let mut list = Vec::new();
    for dir in font_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "ttf" | "ttc" | "otf" | "otc") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let count = match FileRef::new(&bytes) {
                Ok(FileRef::Font(_)) => 1,
                Ok(FileRef::Collection(c)) => c.len(),
                Err(_) => 0,
            };
            for index in 0..count {
                let Ok(font) = skrifa::FontRef::from_index(&bytes, index) else { continue };
                let cmap = font.charmap();
                if cmap.map('あ').is_none() || cmap.map('漢').is_none() {
                    continue;
                }
                if let Some(name) = font_name(&font) {
                    list.push(FontChoice { name, path: path.clone(), index });
                }
            }
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list.dedup_by(|a, b| a.name == b.name);
    list
}

/// 表示名(日本語の名前があればそれ)
fn font_name(font: &skrifa::FontRef) -> Option<String> {
    let names = || font.localized_strings(StringId::FULL_NAME);
    let s = names().find(|s| s.language().is_some_and(|l| l.starts_with("ja"))).or_else(|| names().english_or_first())?;
    let name: String = s.chars().collect();
    (!name.trim().is_empty()).then_some(name)
}
