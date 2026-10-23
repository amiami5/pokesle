//! 読み取り結果(`StatusResult`)を、ボックスのポケモンに変換する。
//! 本家 `screenshot-import.vue` の `createBox` と同じ検査をする。

use pokesle_core::data::master;
use pokesle_core::sim_pokemon::BoxPokemon;

use crate::status::StatusResult;

/// ボックスに追加できる形にする。足りない項目があれば、画面に出すエラーを返す。
///
/// `pokemon_name` は、種族を読み取れなかった(または候補から選んだ)ときに指定する。
/// 指定が無ければ、読み取り結果の種族を使う。
pub fn to_box_pokemon(result: &StatusResult, pokemon_name: Option<&str>, shiny: bool) -> Result<BoxPokemon, String> {
    let m = master();
    let name = pokemon_name.or(result.pokemon_name.as_deref());
    let pokemon = name.and_then(|n| m.pokemon(n)).ok_or("ポケモンの種族を特定できませんでした。")?;
    let lv = result.lv.ok_or("レベルを取得できませんでした。")?;
    let nature = result
        .nature
        .as_deref()
        .filter(|n| m.nature(n).is_some())
        .ok_or("せいかくを取得できませんでした。")?;

    // 解放前のポケモンは、食材とサブスキルを持たない
    if !pokemon.kaihou && !result.food_list.iter().all(|f| f.as_deref().is_some_and(|f| m.food(f).is_some())) {
        return Err("食材構成を取得できませんでした。".into());
    }
    if !pokemon.kaihou && result.sub_skill_list.len() < 5 {
        return Err("サブスキルを5件取得できませんでした。".into());
    }

    Ok(BoxPokemon {
        name: pokemon.name.clone(),
        lv: f64::from(lv),
        skill_lv: Some(f64::from(result.main_skill_lv.unwrap_or(1))),
        food_list: result.food_list.iter().flatten().cloned().collect(),
        sub_skill_list: result.sub_skill_list.clone(),
        nature: nature.to_string(),
        shiny,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gardevoir() -> StatusResult {
        StatusResult {
            pokemon_name: Some("サーナイト".into()),
            lv: Some(67),
            nature: Some("おとなしい".into()),
            main_skill_lv: Some(5),
            food_list: vec![Some("とくせんリンゴ".into()), Some("ワカクサコーン".into()), Some("ワカクサコーン".into())],
            sub_skill_list: ["おてつだいスピードM", "食材確率アップS", "スキル確率アップS", "スキルレベルアップM", "最大所持数アップS"]
                .map(String::from)
                .to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn converts_a_complete_result() {
        let pokemon = to_box_pokemon(&gardevoir(), None, true).unwrap();
        assert_eq!(pokemon.name, "サーナイト");
        assert_eq!((pokemon.lv, pokemon.skill_lv), (67.0, Some(5.0)));
        assert_eq!(pokemon.food_list, ["とくせんリンゴ", "ワカクサコーン", "ワカクサコーン"]);
        assert_eq!(pokemon.sub_skill_list.len(), 5);
        assert_eq!(pokemon.nature, "おとなしい");
        assert!(pokemon.shiny);
    }

    #[test]
    fn skill_lv_defaults_to_one() {
        let result = StatusResult { main_skill_lv: None, ..gardevoir() };
        assert_eq!(to_box_pokemon(&result, None, false).unwrap().skill_lv, Some(1.0));
    }

    #[test]
    fn explicit_pokemon_name_wins() {
        let result = StatusResult { pokemon_name: None, ..gardevoir() };
        assert!(to_box_pokemon(&result, None, false).is_err());
        assert_eq!(to_box_pokemon(&result, Some("エルレイド"), false).unwrap().name, "エルレイド");
    }

    #[test]
    fn reports_missing_items() {
        let error = |result: StatusResult| to_box_pokemon(&result, None, false).unwrap_err();
        assert_eq!(error(StatusResult { lv: None, ..gardevoir() }), "レベルを取得できませんでした。");
        assert_eq!(error(StatusResult { nature: Some("ふめい".into()), ..gardevoir() }), "せいかくを取得できませんでした。");
        let mut missing_food = gardevoir();
        missing_food.food_list[2] = None;
        assert_eq!(error(missing_food), "食材構成を取得できませんでした。");
        let mut missing_sub_skill = gardevoir();
        missing_sub_skill.sub_skill_list.pop();
        assert_eq!(error(missing_sub_skill), "サブスキルを5件取得できませんでした。");
    }
}
