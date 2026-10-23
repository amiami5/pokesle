//! Google Cloud Vision API(DOCUMENT_TEXT_DETECTION)による文字認識。
//!
//! 画像を Google のサーバーへ送る。API キーは利用者が自分で取得して設定する。

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::{OcrEngine, OcrLine};

const ENDPOINT: &str = "https://vision.googleapis.com/v1/images:annotate";
const TIMEOUT: Duration = Duration::from_secs(30);

/// 料金表のページ(設定画面からリンクする)
pub const PRICING_URL: &str = "https://cloud.google.com/vision/pricing";
/// 月あたりの無料枠(画像1枚が1ユニット)。料金表の記載で、変わることがある。
pub const FREE_UNITS_PER_MONTH: u32 = 1000;

/// 今月の利用回数。月が変わったら数え直す。
///
/// 利用回数は、このアプリから送った画像の枚数の目安。同じ API キーを他で使った分や、
/// Google 側の集計の月の区切り(厳密な時刻)とは一致しないことがある。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VisionUsage {
    /// 数えている月("2026-10" の形)
    pub month: String,
    pub count: u32,
}

impl VisionUsage {
    /// `month` の利用回数。数えている月が違えば 0。
    pub fn count_in(&self, month: &str) -> u32 {
        if self.month == month { self.count } else { 0 }
    }

    /// `month` の無料枠の残り。
    pub fn free_remaining_in(&self, month: &str) -> u32 {
        FREE_UNITS_PER_MONTH.saturating_sub(self.count_in(month))
    }

    pub fn record(&mut self, month: &str) {
        if self.month != month {
            self.month = month.to_string();
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
    }
}

/// 今の月("2026-10" の形)。Google Cloud の請求の月に近づけるため、太平洋標準時(UTC-8)で数える。
pub fn current_month_key() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    month_key_at(secs)
}

fn month_key_at(unix_secs: i64) -> String {
    let days = (unix_secs - 8 * 3600).div_euclid(86400);
    // 日数から年月日への変換(Howard Hinnant の civil_from_days)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}")
}

pub struct GoogleVision {
    api_key: String,
    usage: Option<Arc<Mutex<VisionUsage>>>,
}

impl GoogleVision {
    pub fn new(api_key: &str) -> Self {
        Self { api_key: api_key.trim().to_string(), usage: None }
    }

    /// 認識に成功するたびに、利用回数を数える。
    pub fn with_usage(mut self, usage: Arc<Mutex<VisionUsage>>) -> Self {
        self.usage = Some(usage);
        self
    }
}

impl OcrEngine for GoogleVision {
    fn name(&self) -> &'static str {
        "Google Cloud Vision"
    }

    fn recognize(&self, png: &[u8]) -> Result<Vec<OcrLine>, String> {
        if self.api_key.is_empty() {
            return Err("Google Cloud Vision の API キーが設定されていません。".into());
        }

        let body = json!({
            "requests": [{
                "image": { "content": STANDARD.encode(png) },
                "features": [{ "type": "DOCUMENT_TEXT_DETECTION" }],
                "imageContext": { "languageHints": ["ja"] },
            }],
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| e.without_url().to_string())?;
        // キーは URL に載せず、ヘッダーで渡す(エラー文やログに残さないため)
        let response = client
            .post(ENDPOINT)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .map_err(|e| format!("Google Cloud Vision に接続できませんでした: {}", e.without_url()))?;

        let status = response.status();
        let value: Value = response
            .json()
            .map_err(|e| format!("Google Cloud Vision の応答を読み取れませんでした({status}): {}", e.without_url()))?;
        if !status.is_success() {
            return Err(format!("Google Cloud Vision がエラーを返しました({status}): {}", error_message(&value)));
        }
        let line_list = lines_from_response(&value)?;
        if let Some(usage) = &self.usage
            && let Ok(mut usage) = usage.lock()
        {
            usage.record(&current_month_key());
        }
        Ok(line_list)
    }
}

fn error_message(value: &Value) -> String {
    value["error"]["message"].as_str().unwrap_or("詳細不明").to_string()
}

/// `images:annotate` の応答から、行ごとの文字と座標を作る。
/// 行の区切りは、各文字に付く `detectedBreak`(LINE_BREAK / EOL_SURE_SPACE)で判断する。
fn lines_from_response(value: &Value) -> Result<Vec<OcrLine>, String> {
    let response = &value["responses"][0];
    if let Some(message) = response["error"]["message"].as_str() {
        return Err(format!("Google Cloud Vision がエラーを返しました: {message}"));
    }

    let mut line_list = Vec::new();
    let page_list = response["fullTextAnnotation"]["pages"].as_array().into_iter().flatten();
    for block in page_list.flat_map(|page| page["blocks"].as_array().into_iter().flatten()) {
        for paragraph in block["paragraphs"].as_array().into_iter().flatten() {
            let mut line = LineBuilder::default();
            for word in paragraph["words"].as_array().into_iter().flatten() {
                for symbol in word["symbols"].as_array().into_iter().flatten() {
                    line.push_symbol(symbol);
                    match symbol["property"]["detectedBreak"]["type"].as_str() {
                        Some("SPACE" | "SURE_SPACE") => line.text.push(' '),
                        Some("LINE_BREAK" | "EOL_SURE_SPACE") => line_list.extend(line.take()),
                        _ => {}
                    }
                }
            }
            line_list.extend(line.take());
        }
    }
    Ok(line_list)
}

struct LineBuilder {
    text: String,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Default for LineBuilder {
    fn default() -> Self {
        Self { text: String::new(), x0: f64::MAX, y0: f64::MAX, x1: f64::MIN, y1: f64::MIN }
    }
}

impl LineBuilder {
    fn push_symbol(&mut self, symbol: &Value) {
        self.text.push_str(symbol["text"].as_str().unwrap_or(""));
        for vertex in symbol["boundingBox"]["vertices"].as_array().into_iter().flatten() {
            // 0 の座標は応答から省かれるので、無ければ 0 とみなす
            let (x, y) = (vertex["x"].as_f64().unwrap_or(0.0), vertex["y"].as_f64().unwrap_or(0.0));
            self.x0 = self.x0.min(x);
            self.y0 = self.y0.min(y);
            self.x1 = self.x1.max(x);
            self.y1 = self.y1.max(y);
        }
    }

    /// 溜めた文字を1行として取り出す。空なら None。
    fn take(&mut self) -> Option<OcrLine> {
        let finished = std::mem::take(self);
        let text = finished.text.trim().to_string();
        (!text.is_empty() && finished.x0 <= finished.x1).then_some(OcrLine {
            text,
            x0: finished.x0,
            y0: finished.y0,
            x1: finished.x1,
            y1: finished.y1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(text: &str, x: i64, brk: Option<&str>) -> Value {
        let mut symbol = json!({
            "text": text,
            "boundingBox": { "vertices": [
                { "x": x, "y": 10 }, { "x": x + 20, "y": 10 }, { "x": x + 20, "y": 40 }, { "x": x, "y": 40 },
            ] },
        });
        if let Some(brk) = brk {
            symbol["property"] = json!({ "detectedBreak": { "type": brk } });
        }
        symbol
    }

    #[test]
    fn splits_lines_by_detected_break() {
        let value = json!({ "responses": [{ "fullTextAnnotation": { "pages": [{ "blocks": [{ "paragraphs": [{
            "words": [
                { "symbols": [symbol("ピ", 0, None), symbol("カ", 20, Some("SPACE"))] },
                { "symbols": [symbol("4", 60, None), symbol("5", 80, Some("LINE_BREAK"))] },
                { "symbols": [symbol("い", 0, None), symbol("じ", 20, Some("EOL_SURE_SPACE"))] },
            ],
        }] }] }] } }] });
        let line_list = lines_from_response(&value).unwrap();
        assert_eq!(line_list.len(), 2);
        assert_eq!(line_list[0].text, "ピカ 45");
        assert_eq!((line_list[0].x0, line_list[0].x1, line_list[0].y0, line_list[0].y1), (0.0, 100.0, 10.0, 40.0));
        assert_eq!(line_list[1].text, "いじ");
    }

    #[test]
    fn missing_zero_coordinates_are_treated_as_zero() {
        let value = json!({ "responses": [{ "fullTextAnnotation": { "pages": [{ "blocks": [{ "paragraphs": [{
            "words": [{ "symbols": [{
                "text": "あ",
                "boundingBox": { "vertices": [{ "x": 30 }, { "x": 50, "y": 20 }, { "x": 50, "y": 20 }, { "x": 30 }] },
            }] }],
        }] }] }] } }] });
        let line = lines_from_response(&value).unwrap().remove(0);
        assert_eq!((line.x0, line.y0, line.x1, line.y1), (30.0, 0.0, 50.0, 20.0));
    }

    #[test]
    fn empty_response_has_no_lines() {
        // 文字が無い画像では fullTextAnnotation 自体が無い
        assert!(lines_from_response(&json!({ "responses": [{}] })).unwrap().is_empty());
    }

    #[test]
    fn api_error_is_reported() {
        let value = json!({ "responses": [{ "error": { "code": 3, "message": "Bad image data." } }] });
        assert!(lines_from_response(&value).unwrap_err().contains("Bad image data."));
    }

    #[test]
    fn month_key_follows_pacific_standard_time() {
        assert_eq!(month_key_at(0), "1969-12"); // 1970-01-01 00:00 UTC は太平洋時間ではまだ前の月
        assert_eq!(month_key_at(1_793_491_200), "2026-10"); // 2026-11-01 00:00 UTC = 10-31 16:00 PST
        assert_eq!(month_key_at(1_793_491_200 + 8 * 3600), "2026-11");
        assert_eq!(month_key_at(1_709_251_200 + 8 * 3600), "2024-03"); // うるう年の2月末日の翌日
        assert_eq!(month_key_at(1_709_251_200 + 8 * 3600 - 1), "2024-02");
    }

    #[test]
    fn usage_counts_per_month() {
        let mut usage = VisionUsage::default();
        assert_eq!(usage.count_in("2026-10"), 0);
        usage.record("2026-10");
        usage.record("2026-10");
        assert_eq!(usage.count_in("2026-10"), 2);
        assert_eq!(usage.free_remaining_in("2026-10"), FREE_UNITS_PER_MONTH - 2);
        // 月が変わると数え直す。月が違う表示では 0
        assert_eq!(usage.count_in("2026-11"), 0);
        usage.record("2026-11");
        assert_eq!(usage.count_in("2026-11"), 1);
        assert_eq!(usage.count_in("2026-10"), 0);
    }

    #[test]
    fn empty_key_is_rejected_before_sending() {
        let error = GoogleVision::new("  ").recognize(&[]).unwrap_err();
        assert!(error.contains("API キー"));
    }
}
