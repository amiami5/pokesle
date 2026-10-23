//! 本家の食材画像から色の分布(数値だけ)を作り、data/food-signature.json に書き出す。
//! 画像そのものは書き出さない。使い方(リポジトリのルートから):
//!   cargo run -p pokesle-ocr --example make_food_signature

use std::collections::BTreeMap;
use std::path::Path;

use regex::Regex;

fn main() {
    let upstream = Path::new("upstream/src");
    let source = std::fs::read_to_string(upstream.join("data/food_and_cooking.ts")).expect("本家のデータを読めません");

    // `import xxxImg from '../img/food/xxx.png'` と `{ name: '…', energy: …, img: xxxImg }` の対応
    let import_pattern = Regex::new(r"import (\w+) from '\.\./(img/food/[^']+)'").unwrap();
    let path_by_var: BTreeMap<String, String> = import_pattern
        .captures_iter(&source)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let food_pattern = Regex::new(r"name: '([^']+)',\s*energy: \d+,\s*img: (\w+)").unwrap();

    let mut table: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for caps in food_pattern.captures_iter(&source) {
        let (name, var) = (&caps[1], &caps[2]);
        let Some(path) = path_by_var.get(var) else { continue };
        let image = image::open(upstream.join(path)).unwrap_or_else(|e| panic!("{path}: {e}")).to_rgba8();
        let signature = pokesle_ocr::food::template_signature(&image);
        // 小数点以下を丸めて、画像を復元できないほど粗い数値にする
        table.insert(name.to_string(), signature.iter().map(|v| (v * 1e4).round() / 1e4).collect());
    }
    assert!(table.len() >= 19, "食材が {} 種しか見つかりません", table.len());

    let out = Path::new("crates/pokesle-ocr/data/food-signature.json");
    std::fs::write(out, serde_json::to_string_pretty(&table).unwrap() + "\n").unwrap();
    println!("{} 種の色の分布を {} に書き出しました", table.len(), out.display());
}
