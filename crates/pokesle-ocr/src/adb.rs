//! adb でスマートフォンのスクリーンショット(PNG)を取得する。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(15);

/// adb.exe の場所を探す。指定があればそれを、なければ SDK の標準位置と PATH を順に見る。
pub fn find_adb(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }

    let mut sdk_list: Vec<PathBuf> = Vec::new();
    for name in ["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
        if let Some(dir) = std::env::var_os(name).filter(|v| !v.is_empty()) {
            sdk_list.push(PathBuf::from(dir));
        }
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        sdk_list.push(Path::new(&dir).join("Android").join("Sdk"));
    }
    if let Some(dir) = std::env::var_os("ProgramFiles") {
        sdk_list.push(Path::new(&dir).join("Android").join("Android Studio").join("sdk"));
    }
    for dir in sdk_list {
        let candidate = dir.join("platform-tools").join("adb.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("adb.exe"))
        .find(|candidate| candidate.is_file())
}

/// `adb [-s serial] exec-out screencap -p` を実行して PNG のバイト列を返す。
pub fn capture_png(adb: &Path, serial: Option<&str>) -> Result<Vec<u8>, String> {
    let mut command = Command::new(adb);
    if let Some(serial) = serial.map(str::trim).filter(|s| !s.is_empty()) {
        command.args(["-s", serial]);
    }
    command
        .args(["exec-out", "screencap", "-p"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("adb を起動できませんでした({}): {e}", adb.display()))?;
    let mut stdout = child.stdout.take().ok_or("adb の出力を取得できませんでした。")?;
    let mut stderr = child.stderr.take().ok_or("adb のエラー出力を取得できませんでした。")?;
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("ADB からの応答がタイムアウトしました。端末の接続を確認してください。".into());
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };

    let png = out_thread.join().unwrap_or_default();
    let message = err_thread.join().unwrap_or_default();
    if !status.success() {
        let message = message.trim();
        return Err(if message.is_empty() {
            format!("adb が異常終了しました({status})。")
        } else {
            message.to_string()
        });
    }
    if !png.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err("ADB から画像データを取得できませんでした。USB デバッグの許可を確認してください。".into());
    }
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_path_wins() {
        assert_eq!(
            find_adb(Some(r" C:	oolsdb.exe ")),
            Some(PathBuf::from(r"C:	oolsdb.exe"))
        );
    }

    #[test]
    fn blank_configured_path_is_ignored() {
        // 空文字は未設定扱い(環境次第で見つかるかどうかは変わるので、指定値にならないことだけ見る)
        assert_ne!(find_adb(Some("  ")), Some(PathBuf::from("")));
    }
}
