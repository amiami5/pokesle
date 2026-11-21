//! Windows の exe に埋め込むアイコン(エクスプローラー・タスクバー用)とビルド時情報の埋め込み

use std::process::Command;

fn git_path(arg: &str) -> Option<String> {
    let out = Command::new("git").args(["rev-parse", "--git-path", arg]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}

fn git_short_hash() -> String {
    if let Ok(out) = Command::new("git").args(["rev-parse", "--short=8", "HEAD"]).output()
        && out.status.success()
        && let Ok(s) = String::from_utf8(out.stdout)
    {
        let s = s.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    if let Ok(sha) = std::env::var("GITHUB_SHA") {
        let short: String = sha.chars().take(8).collect();
        if !short.is_empty() {
            return short;
        }
    }
    "unknown".to_string()
}

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if let Some(head) = git_path("HEAD") {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Ok(out) = Command::new("git").args(["symbolic-ref", "-q", "HEAD"]).output()
        && out.status.success()
        && let Ok(sym) = String::from_utf8(out.stdout)
        && let Some(ref_path) = git_path(sym.trim())
    {
        println!("cargo:rerun-if-changed={ref_path}");
    }
    println!("cargo:rustc-env=POKESLE_GIT_HASH={}", git_short_hash());

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=アイコンの埋め込みに失敗: {e}");
        }
    }
}
