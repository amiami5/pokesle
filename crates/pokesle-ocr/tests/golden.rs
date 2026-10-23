//! 本家(upstream/)の OCR 解釈ロジックとの突き合わせ。
//! 正解データは `tools/upstream-bridge/golden-ocr.mjs` で本家を Node で実行して作る。

use pokesle_ocr::status::{FoodRecognition, StatusLine, StatusResult, normalize_text, parse_data, text_similarity};
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    #[serde(rename = "textPairs")]
    text_pairs: Vec<TextPair>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct TextPair {
    expected: String,
    actual: String,
    similarity: f64,
    normalized: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    text: String,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    sub_skill_rarity: Option<u32>,
}

#[derive(Deserialize)]
struct Region {
    occluded: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Food {
    name: Option<String>,
    num: Option<f64>,
    region: Region,
    match_list: Vec<Match>,
    empty: bool,
}

#[derive(Deserialize)]
struct Match {
    name: String,
    score: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    raw_text: String,
    line_list: Vec<Line>,
    food_recognition_list: Vec<Food>,
    primary_line_list: Vec<Line>,
    source_coordinate_line_list: Vec<Line>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    name: Option<String>,
    pokemon_name: Option<String>,
    lv: Option<u32>,
    help_time: Option<String>,
    bag: Option<u32>,
    food_list: Vec<Option<String>>,
    main_skill_name: Option<String>,
    main_skill_lv: Option<u32>,
    sub_skill_list: Vec<String>,
    nature: Option<String>,
}

#[derive(Deserialize)]
struct Case {
    input: Input,
    result: Expected,
}

fn lines(list: &[Line]) -> Vec<StatusLine> {
    list.iter()
        .map(|l| StatusLine {
            text: l.text.clone(),
            x0: l.x0,
            y0: l.y0,
            x1: l.x1,
            y1: l.y1,
            sub_skill_rarity: l.sub_skill_rarity,
        })
        .collect()
}

fn golden() -> Golden {
    let json = include_str!("golden/ocr-status.json");
    serde_json::from_str(json).expect("ocr-status.json を読み込めませんでした")
}

#[test]
fn text_similarity_matches_upstream() {
    let golden = golden();
    for pair in &golden.text_pairs {
        assert_eq!(normalize_text(&pair.actual), pair.normalized, "正規化 {:?}", pair.actual);
        let actual = text_similarity(&pair.expected, &pair.actual);
        assert!(
            (actual - pair.similarity).abs() < 1e-9,
            "類似度 {:?} と {:?}: 本家 {} / Rust {}",
            pair.expected,
            pair.actual,
            pair.similarity,
            actual
        );
    }
}

#[test]
fn parse_data_matches_upstream() {
    let golden = golden();
    let mut mismatch_list = Vec::new();
    for (index, case) in golden.cases.iter().enumerate() {
        let input = &case.input;
        let food_list: Vec<FoodRecognition> = input
            .food_recognition_list
            .iter()
            .map(|f| FoodRecognition {
                name: f.name.clone(),
                num: f.num,
                match_list: f.match_list.iter().map(|m| (m.name.clone(), m.score)).collect(),
                empty: f.empty,
                occluded: f.region.occluded.unwrap_or(false),
            })
            .collect();
        let actual = parse_data(
            &input.raw_text,
            &lines(&input.line_list),
            &food_list,
            &lines(&input.primary_line_list),
            &lines(&input.source_coordinate_line_list),
        );
        let expected = StatusResult {
            name: case.result.name.clone(),
            pokemon_name: case.result.pokemon_name.clone(),
            lv: case.result.lv,
            help_time: case.result.help_time.clone(),
            bag: case.result.bag,
            food_list: case.result.food_list.clone(),
            main_skill_name: case.result.main_skill_name.clone(),
            main_skill_lv: case.result.main_skill_lv,
            sub_skill_list: case.result.sub_skill_list.clone(),
            nature: case.result.nature.clone(),
            pokemon_candidate_list: Vec::new(),
            raw_text: input.raw_text.clone(),
        };
        if actual != expected {
            mismatch_list.push(format!("#{index}\n  本家: {expected:?}\n  Rust: {actual:?}"));
        }
    }
    assert!(
        mismatch_list.is_empty(),
        "{} / {} 件が本家と一致しません\n{}",
        mismatch_list.len(),
        golden.cases.len(),
        mismatch_list.iter().take(5).cloned().collect::<Vec<_>>().join("\n")
    );
}
