//! 本家ブラウザ版(localStorage)からの移行データの読み取り
//!
//! ブラウザの F12 コンソールで次を実行するとクリップボードにコピーされる。
//!
//! ```js
//! copy(JSON.stringify({ config: JSON.parse(localStorage.getItem('config')), pokemonBox: JSON.parse(localStorage.getItem('pokemonBox')) }))
//! ```
//!
//! 設定だけ・ボックスだけの JSON(localStorage の値そのまま)でも読み込める。

use serde_json::Value;

use crate::box_io::{import_browser_json, BoxFile};
use crate::config::Config;

/// コンソールに貼り付けるコマンド
pub const CONSOLE_COMMAND: &str =
    "copy(JSON.stringify({ config: JSON.parse(localStorage.getItem('config')), pokemonBox: JSON.parse(localStorage.getItem('pokemonBox')) }))";

#[derive(Debug, Default)]
pub struct BrowserImport {
    pub config: Option<Config>,
    pub box_file: Option<BoxFile>,
}

/// 値が JSON 文字列(localStorage の値をそのまま入れた場合)なら中身を取り出す
fn unwrap_string(v: &Value) -> Value {
    match v {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        other => other.clone(),
    }
}

fn is_box(v: &Value) -> bool {
    v.get("list").is_some_and(Value::is_array)
}

fn is_config(v: &Value) -> bool {
    v.get("simulation").is_some() || v.get("selectEvaluate").is_some() || v.get("teamSimulation").is_some()
}

pub fn parse(text: &str) -> Result<BrowserImport, String> {
    let root: Value = serde_json::from_str(text.trim()).map_err(|e| format!("JSON として読み込めません: {e}"))?;
    let root = unwrap_string(&root);
    let (config_v, box_v) = if root.get("config").is_some() || root.get("pokemonBox").is_some() {
        (unwrap_string(&root["config"]), unwrap_string(&root["pokemonBox"]))
    } else if is_box(&root) {
        (Value::Null, root)
    } else if is_config(&root) {
        (root, Value::Null)
    } else {
        return Err("ポケスリシミュレーターの設定・ボックスのデータが見つかりません".into());
    };

    let mut result = BrowserImport::default();
    if config_v.is_object() {
        result.config = Some(Config::from_saved(&config_v).map_err(|e| format!("設定を読み込めません: {e}"))?);
    }
    if box_v.is_object() {
        if !is_box(&box_v) {
            return Err("ボックスのデータに list がありません".into());
        }
        result.box_file = Some(import_browser_json(&box_v).map_err(|e| format!("ボックスを読み込めません: {e}"))?);
    }
    if result.config.is_none() && result.box_file.is_none() {
        return Err("取り込めるデータがありません(config / pokemonBox が空です)".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_and_single() {
        let pokemon = r#"{"name":"ピカチュウ","lv":30,"foodList":["リラックスカカオ","とくせんリンゴ","あまいミツ"],"subSkillList":["おてつだいスピードM","きのみの数S","食材確率アップS","スキル確率アップS","最大所持数アップS"],"nature":"いじっぱり"}"#;
        let both = format!(r#"{{"config":{{"simulation":{{}}}},"pokemonBox":{{"time":"2026-01-01T00:00:00.000Z","list":[{pokemon}]}}}}"#);
        let r = parse(&both).unwrap();
        assert!(r.config.is_some());
        assert_eq!(r.box_file.unwrap().list.len(), 1);

        let only_box = format!(r#"{{"list":[{pokemon}]}}"#);
        let r = parse(&only_box).unwrap();
        assert!(r.config.is_none() && r.box_file.is_some());

        // localStorage の値(文字列)をそのまま入れた場合
        let quoted = serde_json::json!({ "config": "{\"simulation\":{}}" }).to_string();
        assert!(parse(&quoted).unwrap().config.is_some());

        assert!(parse("[1,2]").is_err());
        assert!(parse("{\"config\":null,\"pokemonBox\":null}").is_err());
    }
}
