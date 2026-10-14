//! 設定・ボックス・厳選テーブルの保存先(%APPDATA%\pokesle-rs)

use std::path::PathBuf;

use pokesle_core::box_io::BoxFile;
use pokesle_core::config::Config;
use pokesle_core::evaluate_table::EvaluateTable;
use serde_json::Value;

pub fn dir() -> PathBuf {
    // 見た目の確認用(デバッグ版のみ): 実データを触らないよう保存先を差し替える
    #[cfg(debug_assertions)]
    if let Ok(d) = std::env::var("POKESLE_DEBUG_DATA_DIR") {
        return PathBuf::from(d);
    }
    let dir = directories::ProjectDirs::from("", "", "pokesle-rs")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}


fn read_json(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(dir().join(name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 一時ファイルへ書いてから置き換える(書き込み中の終了で壊れないように)
fn write_json(name: &str, value: &impl serde::Serialize) -> Result<(), String> {
    let path = dir().join(name);
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn load_config() -> Config {
    read_json("config.json")
        .and_then(|v| Config::from_saved(&v).ok())
        .unwrap_or_else(Config::default_config)
}

pub fn save_config(config: &Config) -> Result<(), String> {
    write_json("config.json", &config.to_value())
}

pub fn load_box() -> BoxFile {
    let mut file: BoxFile = read_json("box.json").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    file.reindex();
    file
}

pub fn save_box(file: &BoxFile) -> Result<(), String> {
    write_json("box.json", file)
}

pub fn load_evaluate_table() -> Option<EvaluateTable> {
    read_json("evaluate-table.json").and_then(|v| serde_json::from_value(v).ok())
}

pub fn save_evaluate_table(table: &EvaluateTable) -> Result<(), String> {
    write_json("evaluate-table.json", table)
}

/// 見た目の設定(本家にはない、このアプリ独自の設定)
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiSetting {
    /// None なら既定(游ゴシック等)
    pub font: Option<crate::fonts::FontChoice>,
    /// 表示倍率(文字の大きさ)
    pub zoom: f32,
    /// 配色(ライト・ダーク・OS に合わせる)
    pub theme: ThemeSetting,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ThemeSetting {
    #[default]
    Light,
    Dark,
    System,
}

impl ThemeSetting {
    pub fn is_dark(self, ctx: &eframe::egui::Context) -> bool {
        match self {
            ThemeSetting::Light => false,
            ThemeSetting::Dark => true,
            ThemeSetting::System => ctx.system_theme() == Some(eframe::egui::Theme::Dark),
        }
    }
}

impl Default for UiSetting {
    fn default() -> Self {
        UiSetting { font: None, zoom: 1.0, theme: ThemeSetting::Light }
    }
}

pub fn load_ui_setting() -> UiSetting {
    read_json("ui-setting.json").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn save_ui_setting(setting: &UiSetting) -> Result<(), String> {
    write_json("ui-setting.json", setting)
}
