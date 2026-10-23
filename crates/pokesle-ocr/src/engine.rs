//! OCR エンジン共通の型。エンジンを差し替えられるよう、入力は PNG のバイト列にそろえる。

/// 認識した1行(座標は入力画像のピクセル)。
#[derive(Clone, Debug, PartialEq)]
pub struct OcrLine {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

pub trait OcrEngine {
    /// 設定画面などに出すエンジン名。
    fn name(&self) -> &'static str;

    /// 画像(PNG)の文字を行ごとに認識する。
    fn recognize(&self, png: &[u8]) -> Result<Vec<OcrLine>, String>;

    /// 文字だけを黒く抜き出した画像でも認識して、結果を足すか。
    /// 色付きの文字(緑の「Lv.」など)を読み落とすエンジンで有効にする。
    /// 認識の回数に応じて課金されるエンジンは、無効のままにする。
    fn needs_binarized_pass(&self) -> bool {
        false
    }
}

/// 同じ高さに並んだ近い行(「Lv. 67」と「サーナイト」など)を、1行につなぐ。
///
/// 本家が使う Tesseract は横に並んだ文字を1行で返すが、エンジンによっては別の行になる。
/// つなぎ目は空白2つにする(本家のサブスキルの2列分割は、2つ以上の空白を区切りとして使うため)。
/// `max_gap_ratio` は、つなぐ行どうしの隙間の上限(行の高さの何倍まで)。
pub fn merge_same_row(line_list: &[OcrLine], max_gap_ratio: f64) -> Vec<OcrLine> {
    let mut sorted: Vec<&OcrLine> = line_list.iter().collect();
    sorted.sort_by(|a, b| a.x0.total_cmp(&b.x0));

    let mut merged: Vec<OcrLine> = Vec::new();
    for line in sorted {
        let height = line.y1 - line.y0;
        // 縦の重なりが半分以上あり、左側にあって隙間が小さい行のうち、一番近いものにつなぐ
        let target = merged
            .iter_mut()
            .filter(|prev| {
                let overlap = prev.y1.min(line.y1) - prev.y0.max(line.y0);
                let min_height = height.min(prev.y1 - prev.y0);
                let gap = line.x0 - prev.x1;
                overlap >= min_height * 0.5 && gap >= -min_height * 0.5 && gap <= height.max(prev.y1 - prev.y0) * max_gap_ratio
            })
            .max_by(|a, b| a.x1.total_cmp(&b.x1));
        match target {
            Some(prev) => {
                prev.text = format!("{}  {}", prev.text, line.text);
                prev.x1 = prev.x1.max(line.x1);
                prev.y0 = prev.y0.min(line.y0);
                prev.y1 = prev.y1.max(line.y1);
            }
            None => merged.push(line.clone()),
        }
    }
    merged.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> OcrLine {
        OcrLine { text: text.to_string(), x0, y0, x1, y1 }
    }

    #[test]
    fn joins_neighbors_on_the_same_row() {
        let list = [line("サーナイト", 425.0, 936.0, 749.0, 993.0), line("Lv. 67", 199.0, 944.0, 351.0, 984.0)];
        let merged = merge_same_row(&list, 2.5);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "Lv. 67  サーナイト");
        assert_eq!((merged[0].x0, merged[0].x1, merged[0].y0, merged[0].y1), (199.0, 749.0, 936.0, 993.0));
    }

    #[test]
    fn keeps_far_or_different_row_lines_apart() {
        let list = [
            line("左", 100.0, 100.0, 200.0, 140.0),
            line("遠い", 900.0, 100.0, 1000.0, 140.0),
            line("下の段", 120.0, 300.0, 220.0, 340.0),
        ];
        assert_eq!(merge_same_row(&list, 2.5).len(), 3);
    }
}
