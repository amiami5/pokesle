//! 見本どうしの近さを確かめる(開発用)。使い方: sample_eval <画像.png>:<枠1の食材>,<枠2>,<枠3> ...
//! 各枠のアイコンを総当たりで比べ、同じ食材どうしと違う食材どうしの類似度を表示する。

fn main() {
    let mut icon_list = Vec::new();
    for arg in std::env::args().skip(1) {
        let (path, names) = arg.rsplit_once(':').expect("<画像>:<食材,食材,食材> の形で指定してください");
        let image = image::open(path).expect("画像を読み込めませんでした").to_rgb8();
        let icons = pokesle_ocr::food::food_icon_list(&image);
        let file = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().to_string();
        for (slot, (icon, name)) in icons.into_iter().zip(names.split(',')).enumerate() {
            icon_list.push((format!("{file}#{}", slot + 1), name.to_string(), icon));
        }
    }
    for (label, name, icon) in &icon_list {
        let mut row: Vec<(f64, String)> = icon_list
            .iter()
            .filter(|(other_label, _, _)| other_label != label)
            .map(|(other_label, other_name, other)| {
                let mark = if other_name == name { "◎" } else { "" };
                (pokesle_ocr::food::pixel_similarity(icon, other), format!("{mark}{other_name}({other_label})"))
            })
            .collect();
        row.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top: Vec<String> = row.iter().take(4).map(|(s, n)| format!("{n} {s:.3}")).collect();
        println!("{label} {name}: {}", top.join(" / "));
    }
}
