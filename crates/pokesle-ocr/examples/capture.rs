//! adb でスクリーンショットを撮って PNG に保存する。使い方: capture <出力先.png> [adb.exeのパス]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("使い方: capture <出力先.png> [adb.exeのパス]");
        std::process::exit(2);
    };
    let Some(adb) = pokesle_ocr::adb::find_adb(args.get(1).map(String::as_str)) else {
        eprintln!("adb.exe が見つかりません。");
        std::process::exit(1);
    };
    println!("adb: {}", adb.display());
    match pokesle_ocr::adb::capture_png(&adb, None) {
        Ok(png) => {
            std::fs::write(out, &png).expect("保存に失敗しました");
            println!("{} バイトを {out} に保存しました", png.len());
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
