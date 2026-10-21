//! ボックスの入出力(本家 `models/pokemon-box/pokemon-box.js`)

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data::master;
use crate::sim_pokemon::BoxPokemon;

/// 保存形式(本家 localStorage の `pokemonBox` と同じ構造)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoxFile {
    /// 最終更新(ISO 8601)
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub list: Vec<BoxPokemon>,
}

impl BoxFile {
    /// 位置情報(index)を振り直す
    pub fn reindex(&mut self) {
        for (i, p) in self.list.iter_mut().enumerate() {
            p.index = i;
        }
    }
}

/// 登録できるポケモンか確認する(本家 `PokemonBox.check`)
pub fn check(pokemon: &BoxPokemon) -> Result<(), String> {
    let m = master();
    let base = m.pokemon(&pokemon.name).ok_or("知らないポケモン")?;
    if pokemon.lv.is_nan() || pokemon.lv <= 0.0 {
        return Err("知らないレベル".into());
    }
    if !base.kaihou {
        if pokemon.food_list.iter().any(|f| m.food(f).is_none()) {
            return Err("知らない食材".into());
        }
        if pokemon.food_list.len() != 3 {
            return Err("食材が3つじゃない".into());
        }
        if pokemon.sub_skill_list.iter().any(|s| m.sub_skill(s).is_none()) {
            return Err("知らないサブスキル".into());
        }
        if pokemon.sub_skill_list.len() != 5 {
            return Err("サブスキルが5つじゃない".into());
        }
    }
    if m.nature(&pokemon.nature).is_none() {
        return Err("知らないせいかく".into());
    }
    Ok(())
}

/// TSV の列番号の上限
const MAX_TSV_COLUMN: usize = 1000;

/// TSV の列設定(本家 `config.pokemonBox.tsv`、1始まりの列番号)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsvColumns {
    pub name: Option<usize>,
    pub lv: Option<usize>,
    pub skill_lv: Option<usize>,
    #[serde(rename = "foodABC")]
    pub food_abc: Option<usize>,
    pub food_list: Vec<Option<usize>>,
    pub sub_skill_list: Vec<Option<usize>>,
    pub nature: Option<usize>,
    pub shiny: Option<usize>,
    pub fix: Option<usize>,
    pub sleep_time: Option<usize>,
    pub training: Option<usize>,
    pub next_exp: Option<usize>,
    pub memo: Option<usize>,
    pub favorite: Option<usize>,
}

impl TsvColumns {
    pub fn from_config(config: &crate::config::Config) -> TsvColumns {
        config
            .extra
            .get("pokemonBox")
            .and_then(|v| v.get("tsv"))
            .and_then(|v| serde_json::from_value::<TsvColumns>(v.clone()).ok())
            .map(TsvColumns::clamped)
            .unwrap_or_else(TsvColumns::default_columns)
    }

    /// 列番号の上限を超えるものは「なし」にする(書き出し時に巨大な配列を確保しないため)
    fn clamped(mut self) -> TsvColumns {
        let limit = |c: &mut Option<usize>| {
            if c.is_some_and(|x| x > MAX_TSV_COLUMN) {
                *c = None;
            }
        };
        for c in [
            &mut self.name, &mut self.lv, &mut self.skill_lv, &mut self.food_abc, &mut self.nature, &mut self.shiny,
            &mut self.fix, &mut self.sleep_time, &mut self.training, &mut self.next_exp, &mut self.memo, &mut self.favorite,
        ] {
            limit(c);
        }
        self.food_list.iter_mut().chain(&mut self.sub_skill_list).for_each(limit);
        self
    }

    /// 設定(`pokemonBox.tsv`)へ書き戻す
    pub fn save_to_config(&self, config: &mut crate::config::Config) {
        let tsv = serde_json::to_value(self).expect("TSV の列設定のシリアライズに失敗しました");
        let pokemon_box = config.extra.entry("pokemonBox").or_insert_with(|| Value::Object(Default::default()));
        if let Value::Object(map) = pokemon_box {
            map.insert("tsv".into(), tsv);
        } else {
            *pokemon_box = serde_json::json!({ "tsv": tsv });
        }
    }

    pub fn default_columns() -> TsvColumns {
        TsvColumns {
            name: Some(1),
            lv: Some(2),
            skill_lv: Some(3),
            food_abc: None,
            food_list: vec![Some(4), Some(5), Some(6)],
            sub_skill_list: vec![Some(7), Some(8), Some(9), Some(10), Some(11)],
            nature: Some(12),
            shiny: Some(13),
            fix: Some(14),
            sleep_time: Some(15),
            training: Some(16),
            next_exp: Some(17),
            memo: Some(18),
            favorite: Some(19),
        }
    }
}

/// JS の `Number(x) || null`
fn num_or_null(s: &str) -> Option<f64> {
    let t = s.trim();
    let v = if t.is_empty() { 0.0 } else { t.parse::<f64>().unwrap_or(f64::NAN) };
    if v == 0.0 || v.is_nan() { None } else { Some(v) }
}

/// TSV を読み込む(本家 `PokemonBox.import`)。読めない行は除外する。
pub fn import_tsv(text: &str, cols: &TsvColumns) -> Vec<BoxPokemon> {
    let m = master();
    let mut list = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        let mut cells: Vec<&str> = vec![""];
        cells.extend(line.split('\t'));
        let cell = |c: Option<usize>| c.and_then(|i| cells.get(i).copied()).unwrap_or("");

        let name = cell(cols.name).to_string();
        let Some(base) = m.pokemon(&name) else { continue };
        let lv_text = cell(cols.lv).trim();
        let lv = if lv_text.is_empty() { 0.0 } else { lv_text.parse::<f64>().unwrap_or(f64::NAN) };

        let mut food_list: Vec<String> = Vec::new();
        if cols.food_abc.is_some() {
            let abc = cell(cols.food_abc);
            let chars: Vec<char> = abc.chars().collect();
            if let Some(pos) = chars.windows(3).position(|w| w[0] == 'A' && matches!(w[1], 'A' | 'B') && matches!(w[2], 'A' | 'B' | 'C')) {
                food_list = chars[pos..pos + 3]
                    .iter()
                    .filter_map(|c| base.food_list.get((*c as u8 - b'A') as usize).map(|f| f.name.clone()))
                    .collect();
            }
        }
        if cols.food_list.len() >= 3 && cols.food_list.iter().take(3).all(|c| c.is_some_and(|x| x != 0)) {
            food_list = cols.food_list.iter().take(3).map(|c| cell(*c).to_string()).collect();
        }

        let p = BoxPokemon {
            name,
            lv,
            skill_lv: num_or_null(cell(cols.skill_lv)),
            food_list,
            sub_skill_list: cols.sub_skill_list.iter().take(5).map(|c| cell(*c).to_string()).collect(),
            nature: cell(cols.nature).to_string(),
            shiny: !cell(cols.shiny).is_empty(),
            fix: num_or_null(cell(cols.fix)),
            sleep_time: num_or_null(cell(cols.sleep_time)),
            training: num_or_null(cell(cols.training)),
            next_exp: num_or_null(cell(cols.next_exp)),
            memo: Some(cell(cols.memo).to_string()).filter(|s| !s.is_empty()),
            favorite: !cell(cols.favorite).is_empty(),
            index: 0,
        };

        if p.lv == 0.0 || p.lv.is_nan() {
            continue;
        }
        if !base.kaihou {
            if p.food_list.len() < 3 || !p.food_list.iter().all(|f| m.food(f).is_some()) {
                continue;
            }
            if !p.sub_skill_list.iter().all(|s| m.sub_skill(s).is_some()) {
                continue;
            }
        }
        if p.nature.is_empty() {
            continue;
        }
        list.push(p);
    }
    for (i, p) in list.iter_mut().enumerate() {
        p.index = i;
    }
    list
}

/// JS の数値の文字列化(整数は小数点なし)
fn js_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { v.to_string() }
}

/// TSV へ書き出す(本家 `pokemon-box-tsv-popup.vue` の `exportList`)
pub fn export_tsv(list: &[BoxPokemon], cols: &TsvColumns) -> String {
    let m = master();
    let all = [cols.name, cols.lv, cols.skill_lv, cols.food_abc, cols.nature, cols.shiny, cols.fix, cols.sleep_time, cols.training, cols.next_exp, cols.memo, cols.favorite];
    let column_num = all.iter().chain(&cols.food_list).chain(&cols.sub_skill_list).flatten().copied().max().unwrap_or(0);
    let opt_num = |v: Option<f64>| v.map(js_num).unwrap_or_default();

    list.iter()
        .map(|p| {
            let mut arr = vec![String::new(); column_num];
            let mut set = |c: Option<usize>, v: String| {
                if let Some(c) = c.filter(|&c| c > 0) {
                    arr[c - 1] = v;
                }
            };
            set(cols.name, p.name.clone());
            set(cols.lv, js_num(p.lv));
            set(cols.skill_lv, opt_num(p.skill_lv));
            if cols.food_abc.is_some() {
                let base = m.pokemon(&p.name);
                let abc = p
                    .food_list
                    .iter()
                    .map(|f| {
                        let i = base.and_then(|b| b.food_list.iter().position(|x| &x.name == f)).unwrap_or(0);
                        (b'A' + i as u8) as char
                    })
                    .collect();
                set(cols.food_abc, abc);
            }
            for (c, f) in cols.food_list.iter().zip(&p.food_list).take(3) {
                set(*c, f.clone());
            }
            for (c, s) in cols.sub_skill_list.iter().zip(&p.sub_skill_list).take(5) {
                set(*c, s.clone());
            }
            set(cols.nature, p.nature.clone());
            set(cols.shiny, if p.shiny { "1".into() } else { String::new() });
            set(cols.fix, opt_num(p.fix));
            set(cols.sleep_time, opt_num(p.sleep_time));
            set(cols.training, opt_num(p.training));
            set(cols.next_exp, opt_num(p.next_exp));
            set(cols.memo, p.memo.clone().unwrap_or_default());
            set(cols.favorite, if p.favorite { "1".into() } else { String::new() });
            arr.join("	")
        })
        .collect::<Vec<_>>()
        .join("
")
}

/// 本家ブラウザ版の localStorage `pokemonBox` の JSON を読み込む
pub fn import_browser_json(value: &Value) -> Result<BoxFile, String> {
    let mut file: BoxFile = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    file.reindex();
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_default_tsv() {
        let tsv = "ピカチュウ\t30\t\tリラックスカカオ\tとくせんリンゴ\tあまいミツ\tおてつだいスピードM\tきのみの数S\t食材確率アップS\tスキル確率アップS\t最大所持数アップS\tいじっぱり\t1\t\t600\n不明\t1";
        let list = import_tsv(tsv, &TsvColumns::default_columns());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].lv, 30.0);
        assert_eq!(list[0].skill_lv, None);
        assert_eq!(list[0].sleep_time, Some(600.0));
        assert!(list[0].shiny);
        assert!(check(&list[0]).is_ok());
    }

    #[test]
    fn export_then_import() {
        let tsv = "ピカチュウ	30		リラックスカカオ	とくせんリンゴ	あまいミツ	おてつだいスピードM	きのみの数S	食材確率アップS	スキル確率アップS	最大所持数アップS	いじっぱり	1		600			メモ";
        let mut cols = TsvColumns::default_columns();
        let list = import_tsv(tsv, &cols);
        let out = export_tsv(&list, &cols);
        assert_eq!(out, format!("{tsv}	"));
        // 列をずらして書き出しても同じ設定で読み戻せる
        cols.skill_lv = Some(20);
        cols.food_abc = Some(21);
        let back = import_tsv(&export_tsv(&list, &cols), &cols);
        assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&list).unwrap());
    }
}
