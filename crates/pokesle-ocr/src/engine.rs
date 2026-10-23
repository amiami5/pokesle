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
}
