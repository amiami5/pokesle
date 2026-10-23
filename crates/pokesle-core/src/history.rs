//! 更新履歴(本家 `pages/history.vue`)
//!
//! 本家のテンプレートを `tools/upstream-bridge/history-parser.mjs` で JSON にしたもの
//! (`data/history.json`)を読む。書き出しは `export-data.mjs` が行うので、本家追従で自動的に更新される。
//! そのうえで、この移植版に関係ない本家の履歴を除き、こちらの更新情報(`data/history-local.json`)を足す。

use std::sync::OnceLock;

use serde::Deserialize;

static HISTORY_JSON: &str = include_str!("../data/history.json");
/// こちら独自の更新情報。本家追従では書き換わらないので、変更したらここに足す
static LOCAL_JSON: &str = include_str!("../data/history-local.json");

/// この日付(Rust 版の初版)より前の本家の履歴は載せない
const PORT_START: (u32, u32, u32) = (2026, 10, 9);
/// 本家の履歴のうち、この文言を含む項目は載せない(この移植版に無い機能)
const EXCLUDED_TEXT: &[&str] = &["手持ちの食材数を読み取る"];

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
    HISTORY.get_or_init(build)
}

/// `2026/10/9` を年月日にする
fn parse_date(date: &str) -> Option<(u32, u32, u32)> {
    let mut it = date.split('/').map(|x| x.parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

fn is_excluded(item: &Item) -> bool {
    item.segments.iter().any(|s| EXCLUDED_TEXT.iter().any(|x| s.text.contains(x)))
}

/// 除外する項目を(入れ子の中も)取り除く。中身が無くなった親の項目も落とす
fn filter_items(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .filter(|i| !is_excluded(i))
        .filter_map(|mut i| {
            let had_children = !i.children.is_empty();
            i.children = filter_items(i.children);
            (!had_children || !i.children.is_empty()).then_some(i)
        })
        .collect()
}

fn build() -> History {
    let mut h: History = serde_json::from_str(HISTORY_JSON).expect("history.json の読み込みに失敗しました");
    let local: Vec<Entry> = serde_json::from_str(LOCAL_JSON).expect("history-local.json の読み込みに失敗しました");
    h.entries.retain(|e| parse_date(&e.date).is_some_and(|d| d >= PORT_START));
    for e in &mut h.entries {
        e.items = filter_items(std::mem::take(&mut e.items));
    }
    h.entries.retain(|e| !e.items.is_empty());
    h.entries.splice(0..0, local);
    // 新しい順。同じ日はこちらの更新情報を先に置く
    h.entries.sort_by_key(|e| std::cmp::Reverse(parse_date(&e.date)));
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load() {
        let h = history();
        assert!(h.entries.len() >= 5);
        assert!(h.entries.iter().all(|e| !e.items.is_empty()));
        assert!(!h.senden.is_empty());
        assert!(h.entries.iter().all(|e| parse_date(&e.date).is_some_and(|d| d >= PORT_START)));
        let dates: Vec<_> = h.entries.iter().map(|e| parse_date(&e.date)).collect();
        assert!(dates.windows(2).all(|w| w[0] >= w[1]));
        let text = format!("{:?}", h.entries);
        assert!(!text.contains("手持ちの食材数を読み取る"));
        assert!(text.contains("ゲンガーの食材確率"));
        assert!(text.contains("Lvを更新"));
    }
}
