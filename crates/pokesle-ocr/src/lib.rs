//! スクリーンショットの取得(adb)と文字認識(OCR)。

pub mod adb;
pub mod engine;
#[cfg(windows)]
pub mod windows_ocr;
pub mod status;
pub mod google_vision;
