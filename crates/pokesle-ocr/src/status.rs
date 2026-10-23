//! ポケモン詳細画面のスクリーンショットから読み取った文字を、ポケモンの情報に解釈する。
//!
//! 本家 `src/models/ocr/pokemon-status-image-reader.ts` の画像処理を除いた部分
//! (行のリストから結果を作る `parseData` と、ポケモンの推定による補正 `correctResult`)の移植。
//! エンジンが違っても同じ解釈ができるよう、入力は OCR 行(文字と座標)にそろえてある。

use std::collections::HashSet;
use std::sync::OnceLock;

use pokesle_core::data::{Pokemon, master};
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::engine::OcrLine;

/// 解釈用の1行。サブスキルの枠の色(レア度)を画像から調べて入れておく。
#[derive(Clone, Debug, PartialEq)]
pub struct StatusLine {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub sub_skill_rarity: Option<u32>,
}

impl From<&OcrLine> for StatusLine {
    fn from(line: &OcrLine) -> Self {
        StatusLine {
            text: line.text.clone(),
            x0: line.x0,
            y0: line.y0,
            x1: line.x1,
            y1: line.y1,
            sub_skill_rarity: None,
        }
    }
}

/// 食材アイコンの認識結果(1枠ぶん)。
#[derive(Clone, Debug, Default)]
pub struct FoodRecognition {
    pub name: Option<String>,
    pub num: Option<f64>,
    /// 食材名と類似度(高い順)
    pub match_list: Vec<(String, f64)>,
    pub empty: bool,
    /// ポップアップに隠れていて画像比較ができない
    pub occluded: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusResult {
    pub name: Option<String>,
    pub pokemon_name: Option<String>,
    pub lv: Option<u32>,
    pub help_time: Option<String>,
    pub bag: Option<u32>,
    pub food_list: Vec<Option<String>>,
    pub main_skill_name: Option<String>,
    pub main_skill_lv: Option<u32>,
    pub sub_skill_list: Vec<String>,
    pub nature: Option<String>,
    /// 種族を1つに決められなかったときの、同じくらい有力な候補(アメの系統が分かっているときだけ)
    pub pokemon_candidate_list: Vec<String>,
    pub raw_text: String,
}

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("正規表現が正しくありません"))
}

pub fn normalize_text(text: &str) -> String {
    static SPACE: OnceLock<Regex> = OnceLock::new();
    static DOT: OnceLock<Regex> = OnceLock::new();
    static BOH: OnceLock<Regex> = OnceLock::new();
    static TETSUDAI: OnceLock<Regex> = OnceLock::new();
    let nfkc: String = text.nfkc().collect();
    let text = regex(&SPACE, r"[\s|｜_]").replace_all(&nfkc, "");
    let text = text.replace(['＄', '$'], "S");
    let text = regex(&DOT, r"[．。]").replace_all(&text, ".");
    // 小さい濁点が別文字になりやすい定型語は、類似度計算の前に代表的な崩れを戻す。
    let text = regex(&BOH, r"ボ[ポボ]+ー").replace_all(&text, "ボー");
    regex(&TETSUDAI, r"おてつた?だ(?:た?だ)+い").replace_all(&text, "おてつだい").into_owned()
}

fn levenshtein_distance(left: &[char], right: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for left_index in 1..=left.len() {
        let mut current = vec![left_index];
        for right_index in 1..=right.len() {
            let substitute = previous[right_index - 1] + usize::from(left[left_index - 1] != right[right_index - 1]);
            current.push((current[right_index - 1] + 1).min(previous[right_index] + 1).min(substitute));
        }
        previous = current;
    }
    previous[right.len()]
}

pub fn text_similarity(expected: &str, actual: &str) -> f64 {
    let expected = normalize_text(expected);
    let actual = normalize_text(actual);
    if actual.is_empty() {
        return 0.0;
    }
    if actual.contains(&expected) {
        return 1.0;
    }

    let expected: Vec<char> = expected.chars().collect();
    let actual: Vec<char> = actual.chars().collect();
    // OCR結果の前後に別の文字が付く場合があるため、候補と近い長さの部分文字列も比較する。
    let mut minimum_distance = levenshtein_distance(&expected, &actual);
    let minimum_length = expected.len().saturating_sub(2).max(1);
    let maximum_length = actual.len().min(expected.len() + 2);
    for length in minimum_length..=maximum_length {
        for start in 0..=(actual.len() - length) {
            minimum_distance = minimum_distance.min(levenshtein_distance(&expected, &actual[start..start + length]));
        }
    }
    1.0 - minimum_distance as f64 / expected.len().max(1) as f64
}

struct BestLine<'a> {
    line: &'a StatusLine,
    score: f64,
}

/// 名前に最も近い行(同点なら先のもの)。行がなければ None。
fn find_best_line<'a>(name: &str, line_list: &'a [StatusLine]) -> Option<BestLine<'a>> {
    let mut best: Option<BestLine> = None;
    for line in line_list {
        let score = text_similarity(name, &line.text);
        if best.as_ref().is_none_or(|b| score > b.score) {
            best = Some(BestLine { line, score });
        }
    }
    best
}

fn split_sub_skill_line(line: &StatusLine) -> Vec<StatusLine> {
    let char_list: Vec<char> = line.text.chars().collect();
    let total = char_list.len().max(1) as f64;
    let width = line.x1 - line.x0;
    let slice = |from: usize, to: usize| -> Option<StatusLine> {
        let text: String = char_list[from..to].iter().collect();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let lead = text.chars().take_while(|c| c.is_whitespace()).count();
        let text_start = from + lead;
        let text_end = text_start + trimmed.chars().count();
        Some(StatusLine {
            text: trimmed.to_string(),
            x0: line.x0 + width * text_start as f64 / total,
            x1: line.x0 + width * text_end as f64 / total,
            ..line.clone()
        })
    };

    let mut segment_list = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < char_list.len() {
        let c = char_list[index];
        let separator_len = if c == '|' || c == '｜' {
            1
        } else if c.is_whitespace() {
            let run = char_list[index..].iter().take_while(|c| c.is_whitespace()).count();
            if run >= 2 { run } else { 0 }
        } else {
            0
        };
        if separator_len == 0 {
            index += 1;
            continue;
        }
        segment_list.extend(slice(start, index));
        index += separator_len;
        start = index;
    }
    if let Some(mut last) = slice(start, char_list.len()) {
        // 最後の区間の右端は元の行の右端のまま(本家と同じ)
        last.x1 = line.x1;
        segment_list.push(last);
    }

    // 区切りを検出できない通常行では、座標と文字列を変えず従来どおり照合する。
    if segment_list.is_empty() { vec![line.clone()] } else { segment_list }
}

fn skill_alias_list(name: &str) -> Vec<String> {
    static SUFFIX: OnceLock<Regex> = OnceLock::new();
    // 画面では「(ランダム)」などの補足が省略されるスキルがあるため、マスター名と表示名の両方を候補にする。
    let short = regex(&SUFFIX, r"\(.+\)$").replace(name, "").into_owned();
    if short == name { vec![name.to_string()] } else { vec![name.to_string(), short] }
}

fn skill_similarity(expected: &str, actual: &str) -> f64 {
    let mut best = f64::MIN;
    for expected_alias in skill_alias_list(expected) {
        for actual_alias in skill_alias_list(actual) {
            best = best.max(text_similarity(&expected_alias, &actual_alias));
        }
    }
    best
}

const SUB_SKILL_UNLOCK_LV: [u32; 5] = [10, 25, 50, 70, 80];

fn enable_sub_skill_list(lv: Option<u32>, sub_skill_list: &[String]) -> &[String] {
    let Some(lv) = lv else { return &[] };
    let enable_length = SUB_SKILL_UNLOCK_LV.iter().filter(|&&unlock| lv >= unlock).count();
    &sub_skill_list[..enable_length.min(sub_skill_list.len())]
}

fn parse_help_time(help_time: Option<&str>) -> Option<f64> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let caps = regex(&PATTERN, r"(?:([0-9]+)時間)?([0-9]+)分([0-9]+)秒").captures(help_time?)?;
    let number = |i: usize| caps.get(i).map_or(0.0, |m| m.as_str().parse::<f64>().unwrap_or(0.0));
    Some(number(1) * 3600.0 + number(2) * 60.0 + number(3))
}

fn format_help_time(seconds: f64) -> String {
    let hour = (seconds / 3600.0).floor();
    let minute = (seconds % 3600.0 / 60.0).floor();
    let second = seconds % 60.0;
    let hour = if hour > 0.0 { format!("{hour}時間") } else { String::new() };
    format!("{hour}{minute}分{second}秒")
}

fn parse_bag(raw_text: &str) -> Option<u32> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    // 複数のOCR結果を連結しているため、先頭に余計な数字が付いた「524個」と正しい「24個」が共存する場合がある。
    // マスター上の最大値へ全所持数サブスキルと睡眠リボンを足した値を上限にし、異常値を推定材料から除外する。
    let maximum_bag = master().pokemon.iter().map(|p| p.bag).fold(0.0, f64::max) + 6.0 + 12.0 + 18.0 + 8.0;
    let bag_list: Vec<u32> = regex(&PATTERN, r"([0-9]+)個")
        .captures_iter(&normalize_text(raw_text))
        .filter_map(|caps| caps[1].parse::<u32>().ok())
        .filter(|&bag| bag >= 1 && f64::from(bag) <= maximum_bag)
        .collect();
    if bag_list.is_empty() {
        return None;
    }

    // 元画像・二値画像・枠抽出画像で繰り返し得られた値を優先し、同数なら先に現れた結果を採用する。
    let count = |bag: u32| bag_list.iter().filter(|&&b| b == bag).count();
    let mut best = bag_list[0];
    for &bag in &bag_list {
        if count(bag) > count(best) {
            best = bag;
        }
    }
    Some(best)
}

fn nature_is_speed(name: Option<&str>, good: bool) -> bool {
    let Some(nature) = name.and_then(|n| master().nature(n)) else { return false };
    let stat = if good { nature.good.as_deref() } else { nature.weak.as_deref() };
    stat == Some("手伝いスピード")
}

fn expected_help_time_list(
    pokemon: &Pokemon,
    lv: Option<u32>,
    sub_skill_list: &[String],
    nature_name: Option<&str>,
) -> Vec<f64> {
    let Some(lv) = lv else { return Vec::new() };

    let enable = enable_sub_skill_list(Some(lv), sub_skill_list);
    let mut speed_bonus = 0.0;
    if enable.iter().any(|s| s == "おてつだいスピードS") {
        speed_bonus += 0.07;
    }
    if enable.iter().any(|s| s == "おてつだいスピードM") {
        speed_bonus += 0.14;
    }
    speed_bonus = f64::min(speed_bonus, 0.35);

    let nature_rate = if nature_is_speed(nature_name, true) {
        0.9
    } else if nature_is_speed(nature_name, false) {
        1.075
    } else {
        1.0
    };
    let base_time = (pokemon.help * (1.0 - (f64::from(lv) - 1.0) * 0.002) * (1.0 - speed_bonus) * nature_rate).floor();

    // 睡眠時間は画像にないため、睡眠リボンなし・500時間・2000時間の全状態を比較対象にする。
    let ribbon_rate_list: Vec<f64> = match pokemon.remain_evolve_lv {
        1 => vec![1.0, 0.95, 0.95 * 0.88],
        2 => vec![1.0, 0.89, 0.89 * 0.75],
        _ => vec![1.0],
    };
    let mut list: Vec<f64> = Vec::new();
    for rate in ribbon_rate_list {
        let value = (base_time * rate).floor();
        if !list.contains(&value) {
            list.push(value);
        }
    }
    list
}

fn expected_bag_list(pokemon: &Pokemon, lv: Option<u32>, sub_skill_list: &[String]) -> Vec<f64> {
    let enable = enable_sub_skill_list(lv, sub_skill_list);
    let has = |name: &str| enable.iter().any(|s| s == name);
    let base_bag = pokemon.bag
        + if has("最大所持数アップS") { 6.0 } else { 0.0 }
        + if has("最大所持数アップM") { 12.0 } else { 0.0 }
        + if has("最大所持数アップL") { 18.0 } else { 0.0 };

    // 睡眠リボンの累計加算は画像だけでは特定できないため、到達し得る各段階を候補に残す。
    [0.0, 1.0, 3.0, 6.0, 8.0].iter().map(|ribbon_bag| base_bag + ribbon_bag).collect()
}

/// そのポケモンの `slot_index` 番目の枠で `food_name` を持つ場合の個数。持てなければ None。
fn food_num(pokemon: &Pokemon, food_name: &str, slot_index: usize) -> Option<f64> {
    // 本家の foodNumListMap は同名の食材が複数あると後のものが残る
    pokemon.food_list.iter().rev().find(|f| f.name == food_name)?.num_list.get(slot_index).copied().flatten()
}

fn is_allowed_food(pokemon: &Pokemon, food_name: &str, slot_index: usize) -> bool {
    food_num(pokemon, food_name, slot_index).is_some()
}

fn allowed_food_match<'a>(
    pokemon: &Pokemon,
    recognition: Option<&'a FoodRecognition>,
    slot_index: usize,
) -> Option<&'a (String, f64)> {
    let recognition = recognition?;
    let mut list: Vec<&(String, f64)> =
        recognition.match_list.iter().filter(|m| is_allowed_food(pokemon, &m.0, slot_index)).collect();
    list.sort_by(|left, right| {
        let by_score = right.1.total_cmp(&left.1);
        let Some(num) = recognition.num else { return by_score };
        // アイコンが半透明でも、表示個数が一致する食材を優先すれば色の近い候補を絞り込める。
        let matched = |m: &(String, f64)| food_num(pokemon, &m.0, slot_index) == Some(num);
        matched(right).cmp(&matched(left)).then(by_score)
    });
    list.into_iter().next()
}

/// 候補のうち値に最も近いもの(同差なら先のもの)。値を取得できていなければ捏造しない。
fn nearest_number(value: Option<f64>, candidate_list: &[f64]) -> Option<f64> {
    let value = value?;
    let mut best: Option<f64> = None;
    for &candidate in candidate_list {
        if best.is_none_or(|b| (candidate - value).abs() < (b - value).abs()) {
            best = Some(candidate);
        }
    }
    best
}

fn min_difference(list: &[f64], value: f64) -> f64 {
    list.iter().map(|v| (v - value).abs()).fold(f64::INFINITY, f64::min)
}

fn correct_bag_sub_skill_list(
    pokemon: &Pokemon,
    lv: Option<u32>,
    bag: Option<u32>,
    sub_skill_list: &[String],
) -> Vec<String> {
    let Some(bag) = bag else { return sub_skill_list.to_vec() };
    let bag = f64::from(bag);

    let mut candidate_list: Vec<Vec<String>> = vec![sub_skill_list.to_vec()];
    for (index, sub_skill) in sub_skill_list.iter().enumerate() {
        if sub_skill != "最大所持数アップM" && sub_skill != "最大所持数アップL" {
            continue;
        }
        candidate_list = candidate_list
            .iter()
            .flat_map(|candidate| {
                ["最大所持数アップM", "最大所持数アップL"].map(|replacement| {
                    let mut next = candidate.clone();
                    next[index] = replacement.to_string();
                    next
                })
            })
            .collect();
    }
    // 同じサブスキルを2つ持つ構成は存在しないため、OCR補正候補から除外する。
    candidate_list.retain(|candidate| {
        let mut seen = HashSet::new();
        candidate.iter().all(|s| seen.insert(s))
    });

    let difference = |candidate: &[String]| min_difference(&expected_bag_list(pokemon, lv, candidate), bag);
    let change_count = |candidate: &[String]| candidate.iter().zip(sub_skill_list).filter(|(a, b)| a != b).count();
    // M/Lは同じ青枠なので、所持数との誤差を最優先し、同点ならOCR結果を変えない候補を残す。
    candidate_list.sort_by(|left, right| {
        difference(left)
            .total_cmp(&difference(right))
            .then(change_count(left).cmp(&change_count(right)))
    });
    candidate_list.into_iter().next().unwrap_or_else(|| sub_skill_list.to_vec())
}

fn correct_level(
    pokemon: &Pokemon,
    recognized_lv: Option<u32>,
    actual_help_time: Option<f64>,
    sub_skill_list: &[String],
    nature: Option<&str>,
) -> Option<u32> {
    let (Some(recognized_lv), Some(actual_help_time)) = (recognized_lv, actual_help_time) else {
        return recognized_lv;
    };

    let difference = |lv: u32| {
        min_difference(&expected_help_time_list(pokemon, Some(lv), sub_skill_list, nature), actual_help_time)
    };
    let recognized_difference = difference(recognized_lv);
    let mut best: Option<(u32, f64)> = None;
    for lv in 1..=100 {
        let d = difference(lv);
        if best.is_none_or(|(_, best_d)| d < best_d) {
            best = Some((lv, d));
        }
    }

    // OCRが「Lv.11」を「Lv.1」と読んだ場合など、表示時間とほぼ完全に一致する別Lvだけを採用する。
    // 数秒以上の誤差がある候補まで補正すると、睡眠リボン等の未取得条件によって正しいLvを変える恐れがある。
    match best {
        Some((lv, d)) if d <= 3.0 && d + 3.0 < recognized_difference => Some(lv),
        _ => Some(recognized_lv),
    }
}

/// 候補のポケモンごとの点数(高い順)。各要素は (ポケモン, 点数, 名前の類似度)。
fn score_pokemon_list(
    result: &StatusResult,
    food_recognition_list: &[FoodRecognition],
    candy: Option<&str>,
) -> Vec<(&'static Pokemon, f64, f64)> {
    let actual_help_time = parse_help_time(result.help_time.as_deref());
    let mut score_list: Vec<(&'static Pokemon, f64, f64)> = master()
        .pokemon
        .iter()
        // 「○○のアメ」が読めていれば、その系統のポケモンだけを候補にする(ニックネームを変えていても判別できる)
        .filter(|pokemon| candy.is_none_or(|candy| pokemon.candy_name == candy))
        .map(|pokemon| {
            let name_similarity = result.name.as_deref().map_or(0.0, |n| text_similarity(&pokemon.name, n));
            let skill_similarity =
                result.main_skill_name.as_deref().map_or(0.0, |n| skill_similarity(&pokemon.skill_name, n));
            let expected_help_time =
                expected_help_time_list(pokemon, result.lv, &result.sub_skill_list, result.nature.as_deref());
            let help_difference = match actual_help_time {
                Some(actual) if !expected_help_time.is_empty() => Some(min_difference(&expected_help_time, actual)),
                _ => None,
            };
            let bag_difference = result.bag.map(|bag| {
                min_difference(&expected_bag_list(pokemon, result.lv, &result.sub_skill_list), f64::from(bag))
            });

            let mut score = 0.0;
            // ニックネームはマスター名と一致しないため、似ている場合だけ加点して不一致は減点しない。
            if name_similarity >= 0.55 {
                score += name_similarity * 8.0;
            }
            if result.main_skill_name.is_some() {
                score += skill_similarity * 8.0;
            }
            // おてつだい時間は種族差を見分けやすいため、近い候補同士でも差が付くよう数分以内の一致を強く評価する。
            if let Some(difference) = help_difference {
                score += 7.0 / (1.0 + difference / 60.0);
            }
            if let Some(difference) = bag_difference {
                score += f64::max(0.0, 1.0 - difference / 18.0) * 4.0;
            }
            for (slot_index, recognition) in food_recognition_list.iter().enumerate() {
                let best_match_score = recognition.match_list.first().map_or(0.0, |m| m.1);
                let Some(allowed) = allowed_food_match(pokemon, Some(recognition), slot_index) else { continue };
                if best_match_score <= 0.0 {
                    continue;
                }

                // 1位の誤認結果だけで判定せず、候補ポケモンが持てる食材のうち画像に最も近いものを評価する。
                score += allowed.1 / best_match_score * 4.0;
                if let Some(num) = recognition.num {
                    // 食材とくい補正を含む表示個数は、基礎値が同じキャタピーとフシギダネのような候補の判別に使える。
                    score += if food_num(pokemon, &allowed.0, slot_index) == Some(num) { 4.0 } else { -2.0 };
                }
            }
            (pokemon, score, name_similarity)
        })
        .collect();
    score_list.sort_by(|left, right| right.1.total_cmp(&left.1));
    score_list
}

fn estimate_pokemon(
    result: &StatusResult,
    food_recognition_list: &[FoodRecognition],
    candy: Option<&str>,
) -> Option<&'static Pokemon> {
    let score_list = score_pokemon_list(result, food_recognition_list, candy);
    let best = score_list.first()?;
    // 系統が分かっている場合は、名前の手がかりが無くても(ニックネーム)、候補の中の比較だけで決める
    if candy.is_none() && best.1 < 8.0 {
        return None;
    }

    // 同点に近い候補を無理に確定すると補正自体が誤りになるため、名前の強い一致か十分な点差を必要とする。
    let second_score = score_list.get(1).map_or(0.0, |s| s.1);
    (best.2 >= 0.8 || best.1 - second_score >= 0.75).then_some(best.0)
}

fn correct_result(result: StatusResult, food_recognition_list: &[FoodRecognition], candy: Option<&str>) -> StatusResult {
    let Some(pokemon) = estimate_pokemon(&result, food_recognition_list, candy) else { return result };

    let actual_help_time = parse_help_time(result.help_time.as_deref());
    // おてつだい時間はサブスキル(おてつだいスピード)で変わる。1枚目の画面だけでサブスキルが分からないとき、
    // 時間からLvを逆算すると、正しく読めたLvを書き換えてしまうので補正しない(アメの系統を使う読み取りのとき)
    let sub_skill_unknown = candy.is_some() && result.sub_skill_list.len() < 5;
    let corrected_lv = if sub_skill_unknown {
        result.lv
    } else {
        correct_level(pokemon, result.lv, actual_help_time, &result.sub_skill_list, result.nature.as_deref())
    };
    let corrected_sub_skill_list = correct_bag_sub_skill_list(pokemon, corrected_lv, result.bag, &result.sub_skill_list);
    let corrected_help_time = nearest_number(
        actual_help_time,
        &expected_help_time_list(pokemon, corrected_lv, &corrected_sub_skill_list, result.nature.as_deref()),
    );
    let corrected_bag = nearest_number(
        result.bag.map(f64::from),
        &expected_bag_list(pokemon, corrected_lv, &corrected_sub_skill_list),
    );

    let main_skill_text = result.main_skill_name.as_deref().unwrap_or("");
    let mut corrected_main_skill_name: Option<String> = None;
    let mut best_score = f64::MIN;
    for alias in skill_alias_list(&pokemon.skill_name) {
        let score = text_similarity(&alias, main_skill_text);
        if score > best_score {
            best_score = score;
            corrected_main_skill_name = Some(alias);
        }
    }

    let corrected_food_list: Vec<Option<String>> = (0..3)
        .map(|slot_index| {
            let recognized_food = result.food_list.get(slot_index).and_then(|f| f.as_deref());
            let recognition = food_recognition_list.get(slot_index);
            let first_food = || pokemon.food_name_list.first().cloned();
            // アイコンが表示されていない未設定枠は、低い類似度の候補から食材を補完しない。
            if recognition.is_some_and(|r| r.empty) {
                return None;
            }
            // 全食材が候補になる特別な個体でも1枠目はマスター先頭の固定食材なので、類似色への誤認を防ぐ。
            if slot_index == 0 && pokemon.food_name_list.len() > 3 {
                return first_food();
            }
            // ポップアップに隠れた1枠目は画像比較が成立しないため、種族ごとに固定の1枠目を使う。
            if slot_index == 0 && recognition.is_some_and(|r| r.occluded) {
                return first_food();
            }
            let recognized_num_matched = match recognition.and_then(|r| r.num) {
                None => true,
                Some(num) => recognized_food.is_some_and(|f| food_num(pokemon, f, slot_index) == Some(num)),
            };
            if let Some(food) = recognized_food
                && !food.is_empty()
                && is_allowed_food(pokemon, food, slot_index)
                && recognized_num_matched
            {
                return Some(food.to_string());
            }

            // 誤検知した食材は、その枠で選択可能なものに限定して画像類似度が最も高い候補へ置き換える。
            allowed_food_match(pokemon, recognition, slot_index)
                .map(|m| m.0.clone())
                .or_else(|| if slot_index == 0 { first_food() } else { None })
        })
        .collect();

    StatusResult {
        // 種族名に十分近い場合だけ表記揺れを直し、ニックネームはそのまま保持する。
        name: match &result.name {
            Some(name) if text_similarity(&pokemon.name, name) >= 0.65 => Some(pokemon.name.clone()),
            other => other.clone(),
        },
        pokemon_name: Some(pokemon.name.clone()),
        lv: corrected_lv,
        // 画面から時間を直接読めた場合は、睡眠リボン等の画像にない条件から逆算した値で上書きしない。
        help_time: if actual_help_time.is_some() {
            result.help_time
        } else {
            corrected_help_time.map(format_help_time)
        },
        bag: corrected_bag.map(|b| b as u32).or(result.bag),
        food_list: corrected_food_list,
        sub_skill_list: corrected_sub_skill_list,
        // 「(ランダム)」等が画面で省略される場合は、認識した表示に近い別名を返す。
        main_skill_name: corrected_main_skill_name,
        ..result
    }
}

/// 本家の `Array.prototype.toSorted` と同じ並びになるよう、`compare` が全順序でなくても
/// 落ちない挿入ソート(V8 の短い配列の動き: 先頭の連続を確認してから二分挿入)を使う。
fn sort_like_v8<T: Clone>(list: &mut Vec<T>, compare: impl Fn(&T, &T) -> std::cmp::Ordering) {
    use std::cmp::Ordering::Less;
    let n = list.len();
    if n < 2 {
        return;
    }
    // 先頭から続く昇順(同値を含む)、または厳密な降順の長さを数える
    let mut run = 2;
    if compare(&list[1], &list[0]) == Less {
        while run < n && compare(&list[run], &list[run - 1]) == Less {
            run += 1;
        }
        list[..run].reverse();
    } else {
        while run < n && compare(&list[run], &list[run - 1]) != Less {
            run += 1;
        }
    }
    for start in run..n {
        let pivot = list[start].clone();
        let (mut left, mut right) = (0, start);
        while left < right {
            let mid = left + (right - left) / 2;
            if compare(&pivot, &list[mid]) == Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        list.remove(start);
        list.insert(left, pivot);
    }
}

/// ポケモン詳細画面のOCR結果を解釈する。
///
/// - `raw_text`: 全体の認識文字(複数の前処理画像の結果を連結してよい)
/// - `line_list`: 行のリスト(全体)
/// - `primary_line_list`: Lv表記やスキル名の座標を取る元画像側の行
/// - `source_coordinate_line_list`: サブスキル枠の座標を取る行(`sub_skill_rarity` を設定済み)
pub fn parse_data(
    raw_text: &str,
    line_list: &[StatusLine],
    food_recognition_list: &[FoodRecognition],
    primary_line_list: &[StatusLine],
    source_coordinate_line_list: &[StatusLine],
) -> StatusResult {
    parse_data_with_candy(raw_text, line_list, food_recognition_list, primary_line_list, source_coordinate_line_list, None)
}

/// `parse_data` に、画面の「○○のアメ」から読んだ系統(マスターのアメ名。例: 「ラルトス」)を加えたもの。
/// 本家にはない手がかりで、ニックネームを変えた個体の種族判定に使う。
pub fn parse_data_with_candy(
    raw_text: &str,
    line_list: &[StatusLine],
    food_recognition_list: &[FoodRecognition],
    primary_line_list: &[StatusLine],
    source_coordinate_line_list: &[StatusLine],
    candy: Option<&str>,
) -> StatusResult {
    static LV: OnceLock<Regex> = OnceLock::new();
    static NAME_HEAD: OnceLock<Regex> = OnceLock::new();
    static HELP_TIME: OnceLock<Regex> = OnceLock::new();
    let m = master();
    let normalized_raw_text = normalize_text(raw_text);

    let lv_pattern = regex(&LV, r"(?i)Lv\.?([0-9]{1,3})(.*)");
    let mut level_line_list: Vec<(&StatusLine, u32, String)> = primary_line_list
        .iter()
        .filter_map(|line| {
            let normalized = normalize_text(&line.text);
            let caps = lv_pattern.captures(&normalized)?;
            Some((line, caps[1].parse().ok()?, caps[2].to_string()))
        })
        .collect();
    level_line_list.sort_by(|a, b| a.0.y0.total_cmp(&b.0.y0));

    // 一番上のLv表記が個体レベルで、その同じ行の残りがポケモン名またはニックネームになる。
    let pokemon_level_line = level_line_list.first();
    let lv = pokemon_level_line.map(|l| l.1);
    let parsed_name = pokemon_level_line
        .map(|l| regex(&NAME_HEAD, r"^[.:・\-]+").replace(&l.2, "").trim().to_string())
        .unwrap_or_default();

    // 1時間を超えるポケモンでは「時間」を含めて取得し、末尾の「17分53秒」だけを短い個体として扱わない。
    let help_time_caps = regex(&HELP_TIME, r"(?:([0-9]+)時間)?([0-9]+)分([0-9]+)秒").captures(&normalized_raw_text);
    let help_time = help_time_caps.map(|caps| {
        let hour = caps.get(1).map(|h| h.as_str().parse::<u32>().unwrap_or(0));
        let minute_text = &caps[2];
        // 時間表記がないのに分が60以上なら、左側の装飾やSP末尾を分へ連結した誤認として末尾2桁を使う。
        let minute: u32 = if minute_text.parse::<u32>().unwrap_or(0) >= 60 && hour.is_none() {
            minute_text[minute_text.len().saturating_sub(2)..].parse().unwrap_or(0)
        } else {
            minute_text.parse().unwrap_or(0)
        };
        let second: u32 = caps[3].parse().unwrap_or(0);
        format!("{}{minute}分{second}秒", hour.map_or(String::new(), |h| format!("{h}時間")))
    });

    // スキル名が最も近い行を探す(補足を省いた別名も候補にする)
    let mut main_skill: Option<(&str, f64)> = None;
    for skill in &m.skills {
        for alias in skill_alias_list(&skill.name) {
            let Some(best) = find_best_line(&alias, line_list) else { continue };
            if main_skill.is_none_or(|(_, score)| best.score > score) {
                main_skill = Some((&skill.name, best.score));
            }
        }
    }

    // メインスキル名と同じ元画像OCRから座標を取り、名前より下にあるLv表記だけをスキルレベル候補にする。
    // これにより、食材欄の「Lv.60」が「Lv.6o」と誤認されても、その先頭の6を拾わない。
    let primary_main_skill_y0 = main_skill.and_then(|(name, _)| {
        let mut best: Option<BestLine> = None;
        for alias in skill_alias_list(name) {
            if let Some(found) = find_best_line(&alias, primary_line_list)
                && best.as_ref().is_none_or(|b| found.score > b.score)
            {
                best = Some(found);
            }
        }
        best.map(|b| b.line.y0)
    });
    let base_y0 = primary_main_skill_y0.or(pokemon_level_line.map(|l| l.0.y0)).unwrap_or(-1.0);
    let main_skill_lv = level_line_list.iter().find(|item| item.0.y0 > base_y0 && item.1 <= 7).map(|item| item.1);

    // 元画像側で見出しを読めない場合もあるため、同じ座標系を持つ二値画像側を含めて下端を探す。
    let detail_status_line = find_best_line("詳細ステータス", source_coordinate_line_list);
    let sub_skill_line_list: Vec<&StatusLine> = source_coordinate_line_list
        .iter()
        .filter(|line| {
            line.y0 > base_y0
                // 見出しは一部しか読めないことがあるため、低めの一致率でも位置境界として採用する。
                && detail_status_line.as_ref().is_none_or(|d| d.score < 0.45 || line.y0 < d.line.y0)
        })
        .collect();

    // 2列のスキルが1つのOCR行に結合されることがあるため、空白と罫線で分けた領域ごとに候補を割り当てる。
    // 詳細ステータスより下を除外し、「おてつだいスピード」の評価表示をサブスキルと誤認しないようにする。
    #[derive(Clone)]
    struct SubSkillHit {
        name: String,
        line: StatusLine,
        score: f64,
    }
    let mut hit_list: Vec<SubSkillHit> = Vec::new();
    for line in sub_skill_line_list.into_iter().flat_map(split_sub_skill_line) {
        let mut best: Option<(&str, f64)> = None;
        for sub_skill in &m.sub_skills {
            let text_score = text_similarity(&sub_skill.name, &line.text);
            // 背景色はS/Mの末尾を取り違えた場合の決め手にする一方、薄い金枠の色を外しても明瞭な文字を捨てない。
            let rarity_score = if line.sub_skill_rarity == Some(sub_skill.rarity) { 0.16 } else { -0.08 };
            let score = text_score + rarity_score;
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((&sub_skill.name, score));
            }
        }
        if let Some((name, score)) = best {
            hit_list.push(SubSkillHit { name: name.to_string(), line, score });
        }
    }
    hit_list.retain(|hit| hit.score >= 0.72);
    hit_list.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut seen = HashSet::new();
    hit_list.retain(|hit| seen.insert(hit.name.clone()));
    // 説明文などの低精度な誤候補が画面上部にあっても、信頼度の高い5件を押し出さないよう先に候補を絞る。
    hit_list.truncate(5);
    sort_like_v8(&mut hit_list, |a, b| {
        let line_height = f64::max(a.line.y1 - a.line.y0, b.line.y1 - b.line.y0);
        if (a.line.y0 - b.line.y0).abs() <= line_height {
            a.line.x0.total_cmp(&b.line.x0)
        } else {
            a.line.y0.total_cmp(&b.line.y0)
        }
    });
    let sub_skill_list: Vec<String> = hit_list.into_iter().map(|hit| hit.name).collect();

    let mut nature: Option<(&str, f64)> = None;
    for item in &m.natures {
        if let Some(best) = find_best_line(&item.name, line_list)
            && nature.is_none_or(|(_, s)| best.score > s)
        {
            nature = Some((&item.name, best.score));
        }
    }

    let result = StatusResult {
        name: (!parsed_name.is_empty()).then_some(parsed_name),
        pokemon_name: None,
        lv,
        help_time,
        bag: parse_bag(raw_text),
        food_list: food_recognition_list.iter().map(|f| f.name.clone()).collect(),
        main_skill_name: main_skill.filter(|(_, score)| *score >= 0.65).map(|(name, _)| name.to_string()),
        main_skill_lv,
        sub_skill_list,
        nature: nature.filter(|(_, score)| *score >= 0.65).map(|(name, _)| name.to_string()),
        pokemon_candidate_list: Vec::new(),
        raw_text: raw_text.to_string(),
    };
    let mut result = correct_result(result, food_recognition_list, candy);
    if result.pokemon_name.is_none() && candy.is_some() {
        // 決め手が足りないときは、最高点との差が小さい候補を並べて、利用者に選んでもらう
        let score_list = score_pokemon_list(&result, food_recognition_list, candy);
        if let Some(best) = score_list.first().map(|s| s.1) {
            result.pokemon_candidate_list =
                score_list.iter().filter(|s| best - s.1 < 0.75).map(|s| s.0.name.clone()).collect();
        }
    }
    result
}

/// 行のリストから「○○のアメ」を探し、マスターのアメ名(系統の基本名)を返す。
/// 読み違いに備えて類似度で選び、似ていなければ None。
pub fn find_candy_name(line_list: &[StatusLine]) -> Option<&'static str> {
    find_candy_line(line_list).map(|(name, _)| name)
}

/// 「○○のアメ」の行を探し、マスターのアメ名とその行を返す。
pub fn find_candy_line(line_list: &[StatusLine]) -> Option<(&'static str, &StatusLine)> {
    let mut candy_name_list: Vec<&str> = master().pokemon.iter().map(|p| p.candy_name.as_str()).collect();
    candy_name_list.sort_unstable();
    candy_name_list.dedup();

    let mut best: Option<(&str, &StatusLine, f64)> = None;
    for candy_name in candy_name_list {
        let expected = format!("{candy_name}のアメ");
        let Some(found) = find_best_line(&expected, line_list) else { continue };
        if best.is_none_or(|(_, _, score)| found.score > score) {
            best = Some((candy_name, found.line, found.score));
        }
    }
    best.filter(|(_, _, score)| *score >= 0.8).map(|(name, line, _)| (name, line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, y0: f64) -> StatusLine {
        StatusLine { text: text.to_string(), x0: 100.0, y0, x1: 500.0, y1: y0 + 40.0, sub_skill_rarity: None }
    }

    fn family_result(nickname: &str, candy: Option<&str>) -> StatusResult {
        let line_list = vec![
            line(&format!("Lv. 67  {nickname}"), 900.0),
            line("ラルトスのアメ  1 , 955", 1060.0),
            line("おてつだい時間", 2030.0),
            line("29 分 51 秒ごと", 2030.0),
        ];
        let raw_text = line_list.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("
");
        parse_data_with_candy(&raw_text, &line_list, &[], &line_list, &line_list, candy)
    }

    #[test]
    fn finds_candy_name_from_line() {
        let list = vec![line("ラルト人のアメ  1 , 955", 1060.0), line("あと EXP 2 , 844", 1120.0)];
        assert_eq!(find_candy_name(&list), Some("ラルトス"));
        assert_eq!(find_candy_name(&[line("レベルアップ", 1263.0)]), None);
    }

    #[test]
    fn candy_restricts_nicknamed_pokemon_to_its_family() {
        let result = family_result("もちこ", Some("ラルトス"));
        // サーナイトとエルレイドは時間が同じで決まらない。候補はラルトスの系統に絞れる
        assert_eq!(result.pokemon_name, None);
        let family: Vec<&str> =
            master().pokemon.iter().filter(|p| p.candy_name == "ラルトス").map(|p| p.name.as_str()).collect();
        assert!(result.pokemon_candidate_list.iter().all(|name| family.contains(&name.as_str())));
        assert_eq!(&result.pokemon_candidate_list[..2], ["サーナイト", "エルレイド"]);
        // ニックネームは種族名に直さない
        assert_eq!(result.name.as_deref(), Some("もちこ"));
    }

    #[test]
    fn name_resolves_between_same_help_time() {
        // サーナイトとエルレイドはおてつだい時間が同じなので、名前が決め手になる
        let result = family_result("サーナイト", Some("ラルトス"));
        assert_eq!(result.pokemon_name.as_deref(), Some("サーナイト"));
        assert_eq!(result.lv, Some(67));
        assert_eq!(result.help_time.as_deref(), Some("29分51秒"));
    }
}
