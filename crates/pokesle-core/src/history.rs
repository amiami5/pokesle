//! 更新履歴(本家 `pages/history.vue`)
//!
//! 本家のテンプレートを `tools/upstream-bridge/history-parser.mjs` で JSON にしたもの
//! (`data/history.json`)を読む。書き出しは `export-data.mjs` が行うので、本家追従で自動的に更新される。

use std::sync::OnceLock;

use serde::Deserialize;

static HISTORY_JSON: &str = include_str!("../data/history.json");

/// 文章の断片。`href` は外部リンク、`route` は本家アプリ内の画面(`/credit` など)
#[derive(Debug, Clone, Deserialize)]
pub struct Segment {
    pub text: String,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub route: Option<String>,
}

/// 箇条書きの1項目(入れ子あり)
#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub segments: Vec<Segment>,
    pub children: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub date: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct History {
    /// 先頭の案内文
    pub senden: Vec<Segment>,
    /// 新しい順
    pub entries: Vec<Entry>,
}

pub fn history() -> &'static History {
    static HISTORY: OnceLock<History> = OnceLock::new();
    HISTORY.get_or_init(|| serde_json::from_str(HISTORY_JSON).expect("history.json の読み込みに失敗しました"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load() {
        let h = history();
        assert!(h.entries.len() > 100);
        assert!(h.entries.iter().all(|e| !e.items.is_empty()));
        assert!(!h.senden.is_empty());
    }
}
