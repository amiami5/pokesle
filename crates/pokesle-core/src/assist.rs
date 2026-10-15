//! ポケモン編集の入力アシスト(本家 `components/pokemon-edit-popup.vue` の inferAssist と
//! `models/utils/convert-romaji.js`)。名前・Lv・食材・サブスキル・せいかくを1行ずつ書いたテキストから個体を組み立てる。

use crate::data::master;
use crate::sim_pokemon::BoxPokemon;

type Kana = [Option<&'static str>; 5];

/// 子音 → 母音(あいうえお)ごとの変換先
const ROMAJI: &[(&str, Kana)] = &[
    ("", [Some("ア"), Some("イ"), Some("ウ"), Some("エ"), Some("オ")]),
    ("B", [Some("バ"), Some("ビ"), Some("ブ"), Some("ベ"), Some("ボ")]),
    ("C", [Some("カ"), Some("シ"), Some("ク"), Some("セ"), Some("コ")]),
    ("D", [Some("ダ"), Some("ヂ"), Some("ヅ"), Some("デ"), Some("ド")]),
    ("F", [Some("ファ"), Some("フィ"), Some("フ"), Some("フェ"), Some("フォ")]),
    ("G", [Some("ガ"), Some("ギ"), Some("グ"), Some("ゲ"), Some("ゴ")]),
    ("H", [Some("ハ"), Some("ヒ"), Some("フ"), Some("ヘ"), Some("ホ")]),
    ("J", [Some("ジャ"), Some("ジ"), Some("ジュ"), Some("ジェ"), Some("ジョ")]),
    ("K", [Some("カ"), Some("キ"), Some("ク"), Some("ケ"), Some("コ")]),
    ("M", [Some("マ"), Some("ミ"), Some("ム"), Some("メ"), Some("モ")]),
    ("N", [Some("ナ"), Some("ニ"), Some("ヌ"), Some("ネ"), Some("ノ")]),
    ("P", [Some("パ"), Some("ピ"), Some("プ"), Some("ペ"), Some("ポ")]),
    ("Q", [Some("クァ"), Some("クィ"), Some("ク"), Some("クェ"), Some("クォ")]),
    ("R", [Some("ラ"), Some("リ"), Some("ル"), Some("レ"), Some("ロ")]),
    ("S", [Some("サ"), Some("シ"), Some("ス"), Some("セ"), Some("ソ")]),
    ("T", [Some("タ"), Some("チ"), Some("ツ"), Some("テ"), Some("ト")]),
    ("V", [Some("ヴァ"), Some("ヴィ"), Some("ヴ"), Some("ヴェ"), Some("ヴォ")]),
    ("W", [Some("ワ"), None, None, None, None]),
    ("Y", [Some("ヤ"), None, Some("ユ"), None, Some("ヨ")]),
    ("Z", [Some("ザ"), Some("ジ"), Some("ズ"), Some("ゼ"), Some("ゾ")]),
    ("CH", [Some("チャ"), Some("チ"), Some("チュ"), Some("チェ"), Some("チョ")]),
    ("DH", [Some("デャ"), Some("ディ"), Some("デュ"), Some("デェ"), Some("デョ")]),
    ("SH", [Some("シャ"), Some("シ"), Some("シュ"), Some("シェ"), Some("ショ")]),
    ("TH", [Some("テャ"), Some("ティ"), Some("テュ"), Some("テェ"), Some("テョ")]),
    ("WH", [Some("ウァ"), Some("ウィ"), Some("ウ"), Some("ウェ"), Some("ウォ")]),
    ("TS", [Some("ツァ"), Some("ツィ"), Some("ツ"), Some("ツェ"), Some("ツォ")]),
    ("KY", [Some("キャ"), Some("キィ"), Some("キュ"), Some("キェ"), Some("キョ")]),
    ("SY", [Some("シャ"), Some("シ"), Some("シュ"), Some("シェ"), Some("ショ")]),
    ("TY", [Some("チャ"), Some("チ"), Some("チュ"), Some("チェ"), Some("チョ")]),
    ("NY", [Some("ニャ"), Some("ニィ"), Some("ニュ"), Some("ニェ"), Some("ニョ")]),
    ("HY", [Some("ヒャ"), Some("ヒィ"), Some("ヒュ"), Some("ヒェ"), Some("ヒョ")]),
    ("MY", [Some("ミャ"), Some("ミィ"), Some("ミュ"), Some("ミェ"), Some("ミョ")]),
    ("RY", [Some("リャ"), Some("リィ"), Some("リュ"), Some("リェ"), Some("リョ")]),
    ("GY", [Some("ギャ"), Some("ギィ"), Some("ギュ"), Some("ギェ"), Some("ギョ")]),
    ("ZY", [Some("ジャ"), Some("ジ"), Some("ジュ"), Some("ジェ"), Some("ジョ")]),
    ("DY", [Some("ヂャ"), Some("ヂィ"), Some("ヂュ"), Some("ヂェ"), Some("ヂョ")]),
    ("BY", [Some("ビャ"), Some("ビィ"), Some("ビュ"), Some("ビェ"), Some("ビョ")]),
    ("PY", [Some("ピャ"), Some("ピィ"), Some("ピュ"), Some("ピェ"), Some("ピョ")]),
    ("L", [Some("ァ"), Some("ィ"), Some("ゥ"), Some("ェ"), Some("ォ")]),
    ("X", [Some("ァ"), Some("ィ"), Some("ゥ"), Some("ェ"), Some("ォ")]),
    ("LK", [Some("ヵ"), None, None, Some("ヶ"), None]),
    ("XK", [Some("ヵ"), None, None, Some("ヶ"), None]),
    ("LY", [Some("ャ"), Some("ィ"), Some("ュ"), Some("ェ"), Some("ョ")]),
    ("XY", [Some("ャ"), Some("ィ"), Some("ュ"), Some("ェ"), Some("ョ")]),
    ("LT", [None, None, Some("ッ"), None, None]),
    ("XT", [None, None, Some("ッ"), None, None]),
    ("LTS", [None, None, Some("ッ"), None, None]),
    ("XTS", [None, None, Some("ッ"), None, None]),
];

/// 本家 convertRomaji。小文字のローマ字をカタカナにする(大文字は変換されない。本家のまま)
pub fn convert_romaji(input: &str) -> String {
    // 子音の連続は促音(ッ)に。N と Y は対象外
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<char> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let is_target = c.is_ascii_alphabetic() && "BCDFGHJKLMPQRSTVWXYZ".contains(c.to_ascii_uppercase());
        if is_target {
            let mut j = i;
            while j < chars.len() && chars[j].eq_ignore_ascii_case(&c) {
                j += 1;
            }
            if j - i >= 2 {
                out.extend(std::iter::repeat_n('ッ', j - i - 1));
                out.push(c);
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    let mut s: String = out.into_iter().collect();

    // 長い綴りから順に置き換える(同じ長さは本家の並び順: - → N → NN → 表の順)
    let mut list: Vec<(String, &str)> = vec![("-".into(), "ー"), ("n".into(), "ン"), ("nn".into(), "ン")];
    const VOWEL: [&str; 5] = ["a", "i", "u", "e", "o"];
    for (consonant, kana) in ROMAJI {
        for (v, k) in kana.iter().enumerate() {
            if let Some(k) = k {
                list.push((format!("{}{}", consonant.to_lowercase(), VOWEL[v]), k));
            }
        }
    }
    list.sort_by_key(|(r, _)| std::cmp::Reverse(r.len()));
    for (romaji, kana) in &list {
        s = s.replace(romaji.as_str(), kana);
    }
    s
}

fn hiragana_to_katakana(s: &str) -> String {
    s.chars().map(|c| if ('\u{3041}'..='\u{3096}').contains(&c) { char::from_u32(c as u32 + 0x60).unwrap_or(c) } else { c }).collect()
}

fn katakana_to_hiragana(s: &str) -> String {
    s.chars().map(|c| if ('\u{30a1}'..='\u{30f6}').contains(&c) { char::from_u32(c as u32 - 0x60).unwrap_or(c) } else { c }).collect()
}

/// 本家の `new RegExp(chars.join('.*'))` を `exec` したときの最初の一致の長さ。一致しなければ None。
/// (最も左で成立する開始位置から、最後の1文字を最も右の出現位置まで貪欲に伸ばす)
fn fuzzy_match_len(pattern: &[char], target: &[char]) -> Option<usize> {
    let (first, last) = (*pattern.first()?, *pattern.last()?);
    for start in 0..target.len() {
        if target[start] != first {
            continue;
        }
        if pattern.len() == 1 {
            return Some(1);
        }
        // start から先で pattern の部分列になっているか
        let mut k = 1;
        let mut end = start;
        for (i, &c) in target.iter().enumerate().skip(start + 1) {
            if k < pattern.len() && c == pattern[k] {
                k += 1;
                end = i;
            }
        }
        if k == pattern.len() {
            let last_pos = target.iter().rposition(|&c| c == last).unwrap_or(end);
            return Some(last_pos.max(end) - start + 1);
        }
    }
    None
}

fn fuzzy_test(pattern: &str, target: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    p.is_empty() || fuzzy_match_len(&p, &target.chars().collect::<Vec<_>>()).is_some()
}

fn infer_name(name: &str, pokemon: &mut BoxPokemon) {
    if name.is_empty() {
        return;
    }
    let pattern: Vec<char> = hiragana_to_katakana(&convert_romaji(name)).chars().collect();
    let mut best: Option<(usize, usize, &str)> = None;
    for p in &master().pokemon {
        let target: Vec<char> = hiragana_to_katakana(&p.name).chars().collect();
        if let Some(len) = fuzzy_match_len(&pattern, &target) {
            let key = (len, p.name.chars().count());
            if best.is_none_or(|(l, n, _)| key < (l, n)) {
                best = Some((key.0, key.1, p.name.as_str()));
            }
        }
    }
    if let Some((_, _, n)) = best {
        pokemon.name = n.to_string();
    }
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// `s:5`(スキルレベル)の数字
fn skill_lv_line(s: &str) -> Option<f64> {
    let rest = s.strip_prefix(['s', 'S'])?.trim_start().strip_prefix(':')?.trim_start();
    if is_digits(rest) { rest.parse().ok() } else { None }
}

fn is_food_line(s: &str) -> bool {
    s.chars().count() == 3 && s.chars().all(|c| matches!(c.to_ascii_lowercase(), 'a'..='h'))
}

fn convert_food_abc(abc: &str, pokemon: &mut BoxPokemon) {
    let Some(base) = master().pokemon(&pokemon.name) else { return };
    let padded: Vec<char> = format!("{abc}   ").chars().take(3).collect();
    pokemon.food_list.resize(3, String::new());
    for (i, letter) in padded.iter().enumerate() {
        let code = letter.to_ascii_uppercase() as u32;
        let code = if (65..=90).contains(&code) {
            code - 65
        } else if (49..=64).contains(&code) {
            code - 49
        } else {
            99
        };
        pokemon.food_list[i] = base.food_name_list.get(code as usize).cloned().unwrap_or_default();
    }
}

fn convert_sub_skill(name: &str, index: usize, pokemon: &mut BoxPokemon) {
    if name.is_empty() {
        return;
    }
    let katakana = hiragana_to_katakana(&convert_romaji(name).to_uppercase());
    let upper = name.to_uppercase();
    let mut list: Vec<_> = master().sub_skills.iter().collect();
    list.sort_by_key(|s| s.input_sort);
    if let Some(s) = list.into_iter().find(|s| fuzzy_test(&katakana, &s.katakana) || s.name == upper) {
        pokemon.sub_skill_list.resize(5, String::new());
        pokemon.sub_skill_list[index] = s.name.clone();
    }
}

fn convert_nature(name: Option<&str>, pokemon: &mut BoxPokemon) {
    let Some(name) = name.filter(|n| !n.is_empty()) else { return };
    let pattern = katakana_to_hiragana(&convert_romaji(name));
    let len = pattern.chars().count() as i64;
    let mut hits: Vec<_> = master().natures.iter().filter(|n| fuzzy_test(&pattern, &n.name)).collect();
    // 名前の長さが入力に近い順(同じなら元の並び)
    hits.sort_by_key(|n| (n.name.chars().count() as i64 - len).abs());
    if let Some(n) = hits.first() {
        pokemon.nature = n.name.clone();
    }
}

/// 本家 inferAssist。テキストの内容で `pokemon` を更新する
pub fn infer(text: &str, pokemon: &mut BoxPokemon) {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let Some((name, rest)) = lines.split_first() else {
        // 本家は空のときも食材・サブスキルを空にする
        pokemon.food_list = vec![String::new(); 3];
        pokemon.sub_skill_list = vec![String::new(); 5];
        pokemon.shiny = false;
        pokemon.favorite = false;
        return;
    };
    infer_name(name, pokemon);

    let has_any = |l: &str, words: &[&str]| {
        let l = l.to_lowercase();
        words.iter().any(|w| l.contains(w))
    };
    let shiny_words = ["shiny", "irochi", "irochigai", "iroti", "irotigai"];
    let favorite_words = ["favorite", "okiniiri"];
    pokemon.shiny = rest.iter().any(|l| has_any(l, &shiny_words));
    pokemon.favorite = rest.iter().any(|l| has_any(l, &favorite_words));

    let status: Vec<&str> = rest.iter().copied().filter(|l| !has_any(l, &shiny_words) && !has_any(l, &favorite_words)).collect();
    if let Some(lv) = status.iter().find(|l| is_digits(l)).and_then(|l| l.parse::<f64>().ok()) {
        pokemon.lv = lv;
    }
    if let Some(lv) = status.iter().find_map(|l| skill_lv_line(l)) {
        pokemon.skill_lv = Some(lv);
    }

    let non_lv: Vec<&str> = status.iter().copied().filter(|l| !is_digits(l) && skill_lv_line(l).is_none()).collect();
    pokemon.food_list = vec![String::new(); 3];
    if let Some(food) = non_lv.iter().find(|l| is_food_line(l)) {
        convert_food_abc(food, pokemon);
    }
    let other: Vec<&str> = non_lv.iter().copied().filter(|l| !is_food_line(l)).collect();
    pokemon.sub_skill_list = vec![String::new(); 5];
    for (i, line) in other.iter().take(5).enumerate() {
        convert_sub_skill(line, i, pokemon);
    }
    convert_nature(other.get(5).copied(), pokemon);
}

/// 本家 syncAssistWithPokemon。個体の内容を入力アシストのテキストにする
pub fn to_text(pokemon: &BoxPokemon) -> String {
    let base = master().pokemon(&pokemon.name);
    let abc: String = pokemon
        .food_list
        .iter()
        .map(|f| {
            if f.is_empty() {
                String::new()
            } else {
                let i = base.and_then(|b| b.food_list.iter().position(|x| x.name == *f)).unwrap_or(0);
                char::from(b'A' + i as u8).to_string()
            }
        })
        .collect();
    let mut lines: Vec<String> = vec![pokemon.name.clone(), pokemon.lv.to_string()];
    if pokemon.shiny {
        lines.push("shiny".into());
    }
    if let Some(s) = pokemon.skill_lv.filter(|s| *s != 0.0) {
        lines.push(format!("s:{s}"));
    }
    lines.push(abc);
    lines.extend(pokemon.sub_skill_list.iter().cloned());
    lines.push(pokemon.nature.clone());
    lines.retain(|l| !l.is_empty());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> BoxPokemon {
        BoxPokemon { name: String::new(), lv: 1.0, nature: String::new(), ..Default::default() }
    }

    #[test]
    fn romaji() {
        assert_eq!(convert_romaji("fushigidane"), "フシギダネ");
        assert_eq!(convert_romaji("kinomi"), "キノミ");
        assert_eq!(convert_romaji("shokus"), "ショクs");
        assert_eq!(convert_romaji("nonki"), "ノンキ");
        assert_eq!(convert_romaji("gakkari"), "ガッカリ");
        assert_eq!(convert_romaji("pika-"), "ピカー");
    }

    #[test]
    fn infer_example_from_placeholder() {
        // 本家の入力例
        let mut p = blank();
        infer("fushigidane\n10\naab\notebo\nkinos\nsukim\nshokus\nshojil\nnonki", &mut p);
        assert_eq!(p.name, "フシギダネ");
        assert_eq!(p.lv, 10.0);
        assert_eq!(p.food_list.len(), 3);
        assert!(p.food_list.iter().all(|f| !f.is_empty()));
        assert_eq!(p.sub_skill_list[0], "おてつだいボーナス");
        assert_eq!(p.nature, "のんき");
    }

    #[test]
    fn shiny_and_skill_lv() {
        let mut p = blank();
        infer("pikachu\n25\nshiny\ns:5", &mut p);
        assert!(p.shiny);
        assert_eq!(p.skill_lv, Some(5.0));
        assert_eq!(p.lv, 25.0);
    }
}
