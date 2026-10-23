//! Windows 標準の OCR(Windows.Media.Ocr)。日本語の OCR 言語パックが必要。

use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine as WinOcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::HSTRING;

use crate::engine::{OcrEngine, OcrLine};

pub struct WindowsOcr {
    language_tag: String,
}

impl WindowsOcr {
    /// 日本語で認識する。
    pub fn japanese() -> Self {
        Self::new("ja-JP")
    }

    pub fn new(language_tag: &str) -> Self {
        Self { language_tag: language_tag.to_string() }
    }

    fn create_engine(&self) -> Result<WinOcrEngine, String> {
        let language = Language::CreateLanguage(&HSTRING::from(self.language_tag.as_str()))
            .map_err(|e| format!("言語 {} を作成できませんでした: {e}", self.language_tag))?;
        if !WinOcrEngine::IsLanguageSupported(&language).unwrap_or(false) {
            return Err(self.missing_language_message());
        }
        WinOcrEngine::TryCreateFromLanguage(&language).map_err(|_| self.missing_language_message())
    }

    fn missing_language_message(&self) -> String {
        format!(
            "Windows の OCR 言語パック({})が入っていません。管理者権限の PowerShell で {} を実行してください。",
            self.language_tag,
            self.install_command()
        )
    }

    /// 言語パックが無いときに、利用者へ案内する PowerShell のコマンド。
    pub fn install_command(&self) -> String {
        format!("Add-WindowsCapability -Online -Name \"Language.OCR~~~{}~0.0.1.0\"", self.language_tag)
    }

    /// この PC で認識が使えるか(OCR 言語パックが入っているか)を調べる。
    pub fn check(&self) -> Result<(), String> {
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        self.create_engine().map(|_| ())
    }

    fn decode(png: &[u8]) -> Result<SoftwareBitmap, String> {
        let stream = InMemoryRandomAccessStream::new().map_err(|e| e.to_string())?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(|e| e.to_string())?;
        writer.WriteBytes(png).map_err(|e| e.to_string())?;
        writer.StoreAsync().map_err(|e| e.to_string())?.get().map_err(|e| e.to_string())?;
        writer.DetachStream().map_err(|e| e.to_string())?;
        stream.Seek(0).map_err(|e| e.to_string())?;

        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| format!("画像を読み込めませんでした: {e}"))?;
        let bitmap = decoder
            .GetSoftwareBitmapAsync()
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;
        // OCR は Gray8 か Bgra8 を受け付ける。
        SoftwareBitmap::ConvertWithAlpha(&bitmap, BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied)
            .map_err(|e| e.to_string())
    }
}

impl OcrEngine for WindowsOcr {
    fn name(&self) -> &'static str {
        "Windows 標準 OCR"
    }

    fn recognize(&self, png: &[u8]) -> Result<Vec<OcrLine>, String> {
        // 呼び出しスレッドで WinRT を使えるようにする(初期化済みならエラーが返るだけなので無視する)。
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };

        let engine = self.create_engine()?;
        let bitmap = Self::decode(png)?;
        let max = WinOcrEngine::MaxImageDimension().unwrap_or(0) as i32;
        if max > 0 && (bitmap.PixelWidth().unwrap_or(0) > max || bitmap.PixelHeight().unwrap_or(0) > max) {
            return Err(format!("画像が大きすぎます(最大 {max}px)。"));
        }

        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| format!("文字認識に失敗しました: {e}"))?;

        let mut line_list = Vec::new();
        for line in result.Lines().map_err(|e| e.to_string())? {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for word in line.Words().map_err(|e| e.to_string())? {
                let rect = word.BoundingRect().map_err(|e| e.to_string())?;
                x0 = x0.min(rect.X as f64);
                y0 = y0.min(rect.Y as f64);
                x1 = x1.max((rect.X + rect.Width) as f64);
                y1 = y1.max((rect.Y + rect.Height) as f64);
            }
            let text = line.Text().map_err(|e| e.to_string())?.to_string_lossy();
            if text.trim().is_empty() || x0 > x1 {
                continue;
            }
            line_list.push(OcrLine { text: join_japanese(&text), x0, y0, x1, y1 });
        }
        Ok(line_list)
    }
}

/// Windows OCR は日本語の文字の間にも空白を入れるので、日本語どうしの間の空白だけ取り除く。
/// 英数字と日本語の境目や英数字どうしの間の空白は、単語の区切りとして残す。
fn join_japanese(text: &str) -> String {
    let char_list: Vec<char> = text.trim().chars().collect();
    let mut result = String::with_capacity(text.len());
    for (index, &c) in char_list.iter().enumerate() {
        if c.is_whitespace() {
            let before = index.checked_sub(1).map(|i| char_list[i]);
            let after = char_list.get(index + 1).copied();
            if let (Some(b), Some(a)) = (before, after)
                && !b.is_ascii()
                && !a.is_ascii()
            {
                continue;
            }
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::join_japanese;

    #[test]
    fn removes_space_between_japanese() {
        assert_eq!(join_japanese("ピ カ チ ュ ウ Lv. 45"), "ピカチュウ Lv. 45");
        assert_eq!(join_japanese("食 材 ド ロ ッ プ 12 個"), "食材ドロップ 12 個");
    }
}
