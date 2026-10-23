//! ポケモン詳細画面のスクリーンショット(上から順に複数枚)を読み取って、結果を表示する。
//! 使い方: read_pokemon [--samples <見本のフォルダ>] <画像1.png> [画像2.png ...]

use pokesle_ocr::windows_ocr::WindowsOcr;

fn main() {
    let mut path_list: Vec<String> = std::env::args().skip(1).collect();
    let mut sample_list = Vec::new();
    if path_list.first().map(String::as_str) == Some("--samples") && path_list.len() >= 2 {
        sample_list = pokesle_ocr::food::load_food_samples(std::path::Path::new(&path_list[1]));
        path_list.drain(..2);
    }
    if path_list.is_empty() {
        eprintln!("使い方: read_pokemon <画像1.png> [画像2.png ...]");
        std::process::exit(2);
    }
    let png_list: Vec<Vec<u8>> =
        path_list.iter().map(|path| std::fs::read(path).expect("画像を読み込めませんでした")).collect();
    match pokesle_ocr::reader::read_pngs(&WindowsOcr::japanese(), &png_list, &sample_list) {
        Ok(read) => {
            let result = read.status;
            let text = |value: &Option<String>| value.clone().unwrap_or_else(|| "(読めず)".into());
            let number = |value: Option<u32>| value.map_or("(読めず)".into(), |v| v.to_string());
            println!("種族      : {}", text(&result.pokemon_name));
            if !result.pokemon_candidate_list.is_empty() {
                println!("候補      : {}", result.pokemon_candidate_list.join("、"));
            }
            println!("名前      : {}", text(&result.name));
            println!("Lv        : {}", number(result.lv));
            println!("せいかく  : {}", text(&result.nature));
            println!("メインスキル: {} Lv{}", text(&result.main_skill_name), number(result.main_skill_lv));
            println!("サブスキル: {}", result.sub_skill_list.join(" / "));
            println!("おてつだい時間: {}", text(&result.help_time));
            println!("最大所持数: {}", number(result.bag));
            let food_list: Vec<String> =
                result.food_list.iter().map(|f| f.clone().unwrap_or_else(|| "―".into())).collect();
            println!("食材      : {}", food_list.join(" / "));
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
