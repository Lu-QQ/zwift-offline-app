mod configurator;
mod detector;
mod service;
mod settings;
mod strava;
mod util;
mod zwiftsync;

use settings::Settings;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub settings: Mutex<Settings>,
}

#[tauri::command]
fn get_settings(s: tauri::State<'_, AppState>) -> Settings {
    s.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, s: tauri::State<'_, AppState>, settings: Settings) -> Result<(), String> {
    settings::save(&app, &settings)?;
    *s.settings.lock().unwrap() = settings.clone();
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 上次进程异常退出留下的 hosts 挂起残留,启动时自愈
            let _ = util::hosts_restore_suspended();
            let mut loaded = settings::load(app.handle());
            // 自动寻找引擎路径:先源码目录,找不到再找 zoffline.exe
            let mut changed = false;
            if loaded.engine_path.is_none() {
                if let Some(p) = util::find_engine_source() {
                    loaded.engine_path = Some(p.to_string_lossy().to_string());
                    changed = true;
                }
            }
            if loaded.engine_path.is_none() && loaded.engine_exe.is_none() {
                if let Some(p) = util::find_engine_exe() {
                    loaded.engine_exe = Some(p.to_string_lossy().to_string());
                    changed = true;
                }
            }
            if changed {
                let _ = settings::save(app.handle(), &loaded);
            }
            app.manage(AppState {
                settings: Mutex::new(loaded),
            });
            app.manage(service::ServiceState::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            detector::detect_run,
            configurator::config_apply,
            configurator::restore_run,
            configurator::install_deps,
            configurator::install_python,
            service::service_start,
            service::service_stop,
            service::service_status,
            service::service_logs,
            service::launch_zwift,
            strava::strava_status,
            strava::strava_authorize,
            zwiftsync::zwift_sync_status,
            zwiftsync::zwift_sync_save,
            zwiftsync::zwift_sync_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
