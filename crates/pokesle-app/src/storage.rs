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

/// データ画面の料理の作成数(本家 localStorage `dataCookingNum`)
pub fn load_data_cooking_num() -> std::collections::HashMap<String, f64> {
    read_json("data-cooking-num.json").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn save_data_cooking_num(map: &std::collections::HashMap<String, f64>) -> Result<(), String> {
    // 0 は保存しない(本家と同じ)
    let nonzero: std::collections::BTreeMap<&String, &f64> = map.iter().filter(|(_, v)| **v != 0.0).collect();
    write_json("data-cooking-num.json", &nonzero)
}

/// 文字認識の設定(本家にはない、このアプリ独自の設定)
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OcrSetting {
    pub engine: OcrEngineChoice,
    /// Google Cloud Vision の API キー(この PC の設定フォルダに平文で保存する)
    pub google_api_key: String,
    /// adb.exe の場所(空なら自動で探す)
    pub adb_path: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum OcrEngineChoice {
    #[default]
    Windows,
    Google,
}

pub fn load_ocr_setting() -> OcrSetting {
    read_json("ocr-setting.json").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn save_ocr_setting(setting: &OcrSetting) -> Result<(), String> {
    write_json("ocr-setting.json", setting)
}

/// 食材アイコンの見本(利用者が確定した食材)の保存先。ファイル名は「表示状態_時刻_食材名.png」
fn food_sample_dir() -> PathBuf {
    let dir = dir().join("food-samples");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 食材と表示状態(薄い/通常)ごとに残す見本の数。古いものから消す
const FOOD_SAMPLE_LIMIT: usize = 10;

pub fn load_food_samples() -> Vec<pokesle_ocr::food::FoodSample> {
    let Ok(entries) = std::fs::read_dir(food_sample_dir()) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            let mut parts = stem.splitn(3, '_');
            let faded = parts.next()? == "1";
            let _time = parts.next()?;
            let name = parts.next()?.to_string();
            let icon = image::open(&path).ok()?.to_rgba8();
            Some(pokesle_ocr::food::FoodSample { name, faded, icon })
        })
        .collect()
}

pub fn save_food_sample(name: &str, faded: bool, icon: &image::RgbaImage) -> Result<(), String> {
    let dir = food_sample_dir();
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let state = if faded { "1" } else { "0" };
    icon.save(dir.join(format!("{state}_{time}_{name}.png"))).map_err(|e| e.to_string())?;

    // 同じ食材・表示状態の見本が多すぎれば、古いものから消す(時刻はファイル名の順に並ぶ)
    let prefix = format!("{state}_");
    let suffix = format!("_{name}.png");
    let mut same: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix) && n.ends_with(&suffix))
        })
        .collect();
    same.sort();
    for old in same.iter().take(same.len().saturating_sub(FOOD_SAMPLE_LIMIT)) {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

/// Google Cloud Vision の今月の利用回数
pub fn load_vision_usage() -> pokesle_ocr::google_vision::VisionUsage {
    read_json("vision-usage.json").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

#[allow(dead_code)] // 文字認識を実行する画面から使う
pub fn save_vision_usage(usage: &pokesle_ocr::google_vision::VisionUsage) -> Result<(), String> {
    write_json("vision-usage.json", usage)
}

pub fn load_ui_setting() -> UiSetting {
    read_json("ui-setting.json").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn save_ui_setting(setting: &UiSetting) -> Result<(), String> {
    write_json("ui-setting.json", setting)
}
