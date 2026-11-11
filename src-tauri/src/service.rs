use crate::detector::resolve_engine;
use crate::settings::Settings;
use crate::util::*;
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

const MAX_LOG_LINES: usize = 2000;

pub struct ServiceState {
    child: Mutex<Option<Child>>,
    pid: Mutex<Option<u32>>,
    logs: Mutex<VecDeque<String>>,
}

impl ServiceState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            pid: Mutex::new(None),
            logs: Mutex::new(VecDeque::new()),
        }
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.child.lock().unwrap();
        match guard.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(_)) => {
                    guard.take();
                    *self.pid.lock().unwrap() = None;
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            },
            None => false,
        }
    }

    fn push_log(&self, app: &AppHandle, line: String) {
        let mut logs = self.logs.lock().unwrap();
        logs.push_back(line.clone());
        while logs.len() > MAX_LOG_LINES {
            logs.pop_front();
        }
        drop(logs);
        let _ = app.emit("service:log", line);
    }

    pub fn clear_logs(&self) {
        self.logs.lock().unwrap().clear();
    }
}

#[derive(Serialize)]
pub struct ServiceStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub healthy: bool,
}

#[tauri::command]
pub fn service_status(service: tauri::State<'_, ServiceState>) -> ServiceStatus {
    let running = service.is_running();
    ServiceStatus {
        running,
        pid: if running { *service.pid.lock().unwrap() } else { None },
        healthy: running && https_alive(),
    }
}

#[tauri::command]
pub fn service_logs(service: tauri::State<'_, ServiceState>) -> Vec<String> {
    service.logs.lock().unwrap().iter().cloned().collect()
}

#[tauri::command]
pub fn service_start(
    app: AppHandle,
    s: tauri::State<'_, crate::AppState>,
    service: tauri::State<'_, ServiceState>,
) -> Result<String, String> {
    if service.is_running() {
        return Err("服务已在运行".into());
    }
    let settings: Settings = s.settings.lock().unwrap().clone();
    let engine = resolve_engine(&settings)
        .ok_or("未配置 zoffline 引擎,请先到“环境检测”页设置源码目录或 exe")?;

    let mut cmd = match (&engine.0, &engine.1) {
        (Some(src), _) => {
            let python = find_python().ok_or("源码模式需要 Python 3,请先安装或改用 exe 模式")?;
            let mut c = Command::new(&python.0);
            c.args(&python.1).arg("standalone.py").current_dir(src);
            c
        }
        (None, Some(exe)) => {
            let mut c = Command::new(exe);
            if let Some(dir) = exe.parent() {
                c.current_dir(dir);
            }
            c
        }
        _ => unreachable!(),
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动失败: {e}(源码模式请确认已安装 Python 依赖)"))?;

    let pid = child.id();
    if let Some(out) = child.stdout.take() {
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(|l| l.ok()) {
                let st = app.state::<ServiceState>();
                st.push_log(&app, line);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(|l| l.ok()) {
                let st = app.state::<ServiceState>();
                st.push_log(&app, format!("[stderr] {line}"));
            }
        });
    }

    *service.child.lock().unwrap() = Some(child);
    *service.pid.lock().unwrap() = Some(pid);
    service.clear_logs();
    service.push_log(&app, format!("=== zoffline 已启动 (PID {pid}) ==="));

    // 退出监视:进程死掉时通知前端
    let app2 = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(700));
        let st = app2.state::<ServiceState>();
        let was_running = {
            let mut guard = st.child.lock().unwrap();
            match guard.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        guard.take();
                        *st.pid.lock().unwrap() = None;
                        Some(status)
                    }
                    Ok(None) => None,
                    Err(_) => None,
                },
                None => break, // 已被 stop 处理
            }
        };
        if let Some(status) = was_running {
            let msg = format!("=== zoffline 已退出 ({status}) ===");
            let st = app2.state::<ServiceState>();
            st.push_log(&app2, msg.clone());
            let _ = app2.emit("service:exited", msg);
            break;
        }
    });

    Ok(format!("已启动 (PID {pid})"))
}

#[tauri::command]
pub fn service_stop(service: tauri::State<'_, ServiceState>, app: AppHandle) -> Result<String, String> {
    if !service.is_running() {
        return Ok("服务未在运行".into());
    }
    let pid = *service.pid.lock().unwrap();
    if let Some(pid) = pid {
        if is_windows() {
            let _ = run_cmd(Command::new("taskkill").args(["/T", "/F", "/PID"]).arg(pid.to_string()));
        } else if let Some(mut child) = service.child.lock().unwrap().take() {
            let _ = child.kill();
        }
    } else if let Some(mut child) = service.child.lock().unwrap().take() {
        let _ = child.kill();
    }
    // taskkill 异步生效,稍等确认
    for _ in 0..20 {
        if !service.is_running() {
            service.push_log(&app, "=== 服务已停止 ===".into());
            return Ok("已停止".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    let _ = app.emit("service:exited", "服务已停止".to_string());
    Ok("已发送停止指令".into())
}

/// 启动 Zwift 客户端
#[tauri::command]
pub fn launch_zwift(s: tauri::State<'_, crate::AppState>) -> Result<String, String> {
    let settings: Settings = s.settings.lock().unwrap().clone();
    let zwift = crate::detector::resolve_zwift_dir(&settings)
        .ok_or("未检测到 Zwift,请先安装或在设置里指定目录")?;
    if is_windows() {
        let exe = zwift.join("ZwiftLauncher.exe");
        if !exe.is_file() {
            return Err(format!("找不到 {}", exe.display()));
        }
        Command::new(&exe)
            .current_dir(&zwift)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok("Zwift 启动器已运行".into())
    } else {
        run_cmd(Command::new("open").args(["-a", "Zwift"]));
        Ok("已请求打开 Zwift".into())
    }
}
