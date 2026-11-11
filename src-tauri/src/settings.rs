use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Settings {
    /// zoffline 源码目录(含 standalone.py),源码模式使用
    pub engine_path: Option<String>,
    /// zoffline.exe 路径(可选,exe 模式优先于源码模式)
    pub engine_exe: Option<String>,
    /// Zwift 安装目录(自动探测失败时手动指定)
    pub zwift_dir: Option<String>,
    /// Strava 授权回调端口,默认 8000
    pub strava_port: u16,
}

impl Settings {
    pub fn strava_port_or_default(&self) -> u16 {
        if self.strava_port == 0 {
            8000
        } else {
            self.strava_port
        }
    }
}

pub fn settings_file(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("无法获取应用数据目录");
    std::fs::create_dir_all(&dir).ok();
    dir.join("settings.json")
}

pub fn load(app: &tauri::AppHandle) -> Settings {
    let path = settings_file(app);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &tauri::AppHandle, s: &Settings) -> Result<(), String> {
    let path = settings_file(app);
    serde_json::to_string_pretty(s)
        .map_err(|e| e.to_string())
        .and_then(|json| std::fs::write(path, json).map_err(|e| e.to_string()))
}
