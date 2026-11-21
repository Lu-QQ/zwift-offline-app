use crate::settings::Settings;
use crate::util::*;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Serialize, Clone)]
pub struct CheckItem {
    pub id: &'static str,
    pub name: String,
    /// ok=通过 / warn=可继续但有提醒 / fail=阻断 / info=仅展示
    pub status: String,
    pub detail: String,
    pub hint: String,
}

fn ok(id: &'static str, name: &str, detail: String) -> CheckItem {
    CheckItem { id, name: name.into(), status: "ok".into(), detail, hint: String::new() }
}
fn warn(id: &'static str, name: &str, detail: String, hint: &str) -> CheckItem {
    CheckItem { id, name: name.into(), status: "warn".into(), detail, hint: hint.into() }
}
fn fail(id: &'static str, name: &str, detail: String, hint: &str) -> CheckItem {
    CheckItem { id, name: name.into(), status: "fail".into(), detail, hint: hint.into() }
}

/// 引擎是否就绪,返回 (源码模式目录, exe 路径) 其中之一
pub fn resolve_engine(s: &Settings) -> Option<(Option<PathBuf>, Option<PathBuf>)> {
    if let Some(exe) = &s.engine_exe {
        let p = PathBuf::from(exe);
        if p.is_file() {
            return Some((None, Some(p)));
        }
    }
    if let Some(dir) = &s.engine_path {
        let p = PathBuf::from(dir);
        if p.join("standalone.py").is_file() && p.join("zwift_offline.py").is_file() {
            return Some((Some(p), None));
        }
    }
    None
}

/// 已配置的有效路径优先，其次查找已有源码引擎，最后准备随包引擎。
pub fn discover_engine(app: &tauri::AppHandle, settings: &mut Settings) -> Result<bool, String> {
    if resolve_engine(settings).is_some() {
        return Ok(false);
    }
    if let Some(path) = find_engine_source() {
        settings.engine_path = Some(path.to_string_lossy().into_owned());
        return Ok(true);
    }

    #[cfg(target_os = "macos")]
    if let Some(path) = crate::mac_engine::prepare(app)? {
        settings.engine_path = Some(path.to_string_lossy().into_owned());
        return Ok(true);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        if let Some(path) = find_engine_exe() {
            settings.engine_exe = Some(path.to_string_lossy().into_owned());
            return Ok(true);
        }
    }

    Ok(false)
}

/// 引擎根目录(源码目录,或 exe 所在目录),ssl/ 挂在这里
pub fn engine_ssl_root(engine: &(Option<PathBuf>, Option<PathBuf>)) -> PathBuf {
    match (&engine.0, &engine.1) {
        (Some(src), _) => src.clone(),
        (None, Some(exe)) => exe.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
        _ => PathBuf::new(),
    }
}

/// 解析 Zwift 目录:设置里手动指定的优先,否则自动探测
pub fn resolve_zwift_dir(s: &Settings) -> Option<PathBuf> {
    if let Some(d) = &s.zwift_dir {
        let p = PathBuf::from(d);
        if p.join("data").is_dir() {
            return Some(p);
        }
    }
    zwift_candidates().into_iter().next()
}

/// 检测 Python 依赖是否齐全,返回缺失模块列表
pub fn missing_python_deps(python: &Python) -> Result<Vec<String>, String> {
    let mut cmd = std::process::Command::new(&python.0);
    let out = cmd.args(&python.1).arg("-c").arg(r#"
import importlib
import json
failed = []
for name in ('flask', 'flask_login', 'flask_sqlalchemy', 'gevent', 'google.protobuf',
             'Crypto', 'jwt', 'dns.resolver', 'fitdecode', 'stravalib'):
    try:
        importlib.import_module(name)
    except Exception:
        failed.append(name)
print(json.dumps(failed))
"#).output().map_err(|error| format!("无法运行 {}：{error}", python.0))?;
    if !out.status.success() {
        return Err(format!("{} 退出码异常：{}", python.0, out.status));
    }
    let output = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(output.lines().last().unwrap_or_default())
        .map_err(|error| format!("无法读取 Python 检测结果：{error}"))
}

#[tauri::command]
pub async fn detect_run(
    app: tauri::AppHandle,
    service: tauri::State<'_, crate::service::ServiceState>,
) -> Result<Vec<CheckItem>, String> {
    let service_running = service.is_running();
    // 检测要跑 certutil/netstat 等子进程(秒级),放后台线程避免界面卡顿
    tauri::async_runtime::spawn_blocking(move || detect_run_impl(app, service_running))
        .await
        .map_err(|e| format!("后台任务失败: {e}"))
}

fn detect_run_impl(app: tauri::AppHandle, service_running: bool) -> Vec<CheckItem> {
    // 引擎缺失时现场重跑自动寻找(启动时的自动寻找只跑一次;引擎后到位/
    // 手动放入的场景,点「重新检测」就能找到并记住,不用重启应用)
    let (settings, engine_error) = {
        let state = app.state::<crate::AppState>();
        let mut settings = state.settings.lock().unwrap();
        let error = match discover_engine(&app, &mut settings) {
            Ok(true) => {
                let _ = crate::settings::save(&app, &settings);
                None
            }
            Ok(false) => None,
            Err(error) => Some(error),
        };
        (settings.clone(), error)
    };
    let mut items = Vec::new();

    // 1. 管理员权限
    if cfg!(target_os = "macos") {
        items.push(ok("admin", "管理员权限", "配置或还原系统设置时，macOS 会请求授权".into()));
    } else if is_elevated() {
        items.push(ok("admin", "管理员权限", "当前以管理员身份运行".into()));
    } else {
        items.push(warn(
            "admin",
            "管理员权限",
            "当前不是管理员".into(),
            "配置系统证书、修改 hosts、启动服务(80/443 端口)都需要管理员权限。请右键启动器,选择“以管理员身份运行”。",
        ));
    }

    // 2. Zwift 安装
    match resolve_zwift_dir(&settings) {
        Some(dir) => {
            let cacert = zwift_cacert(&dir);
            let cacert_ok = cacert.is_file();
            let appended = cacert_ok
                && std::fs::read_to_string(&cacert)
                    .map(|c| c.contains(CACERT_MARKER))
                    .unwrap_or(false);
            let launcher = if is_windows() {
                dir.join("ZwiftLauncher.exe").is_file()
            } else {
                dir.is_dir()
            };
            let mut detail = format!("安装目录:{}", dir.display());
            if !launcher {
                detail.push_str("(未找到 Zwift 启动程序)");
            }
            if !cacert_ok {
                detail.push_str(";未找到 data/cacert.pem,目录可能不完整");
            } else if appended {
                detail.push_str(";证书已配置");
            }
            let status = if cacert_ok {
                "ok"
            } else {
                "warn"
            };
            let item = if status == "ok" {
                ok("zwift", "Zwift 客户端", detail)
            } else {
                warn("zwift", "Zwift 客户端", detail, "请在“设置”里手动选择 Zwift 安装目录(包含 data 子目录)。")
            };
            items.push(item);
        }
        None => items.push(fail(
            "zwift",
            "Zwift 客户端",
            "未检测到 Zwift".into(),
            "请先安装 Zwift(zwift.com 下载),或在“设置”里手动指定安装目录。",
        )),
    }

    // 3. 引擎(exe 优先,其次源码)
    let engine = resolve_engine(&settings);
    match &engine {
        Some((Some(src), None)) => {
            let ssl_ok = src.join("ssl/cert-zwift-com.p12").is_file() && src.join("ssl/cert-zwift-com.pem").is_file();
            let mut detail = format!("源码模式:{}", src.display());
            if !ssl_ok {
                detail.push_str("(缺少 ssl 证书文件)");
            }
            items.push(ok("engine", "zoffline 引擎", detail));
            // 3a. Python(源码模式需要)
            match find_engine_python(src).as_ref() {
                Some(p) => {
                    items.push(ok("python", "Python", format!("{} · {}", p.2, p.0)));
                    items.push(match missing_python_deps(p) {
                        Ok(missing) if missing.is_empty() => ok("deps", "Python 依赖", "依赖完整,可以直接启动".into()),
                        Ok(missing) => warn(
                            "deps",
                            "Python 依赖",
                            format!("无法导入模块：{}", missing.join(", ")),
                            "点击「一键安装 Python 依赖」，安装到引擎使用的环境。",
                        ),
                        Err(error) => warn("deps", "Python 依赖", error, "请重新检测，确认上面显示的解释器可运行。"),
                    });
                }
                None => items.push(fail(
                    "python",
                    "Python",
                    if cfg!(target_os = "macos") { "未检测到 Python 3.11 或更新版本".into() } else { "未检测到 Python 3".into() },
                    if cfg!(target_os = "macos") {
                        "从 Python 官网安装 macOS 版，完成后点「重新检测」。"
                    } else {
                        "点下方「自动安装 Python」，或改用 zoffline.exe 模式。"
                    },
                )),
            }
        }
        Some((None, Some(exe))) => {
            items.push(ok("engine", "zoffline 引擎", format!("exe 模式:{}", exe.display())));
        }
        _ => items.push(fail(
            "engine",
            "zoffline 引擎",
            engine_error.unwrap_or_else(|| "未找到 zoffline".into()),
            "点下方「⬇ 一键下载引擎」自动搞定;或在“路径设置”里选择 zwift-offline 源码目录(含 standalone.py)/ zoffline.exe。",
        )),
    }

    // 4. 端口(服务已在运行时 80/443 被占是正常状态;service_running 由外层传入)
    for (port, proto, label) in [(80u16, "tcp", "80 (CDN)"), (443, "tcp", "443 (主服务)"), (3025, "tcp", "3025 (TCP)"), (3024, "udp", "3024 (UDP)")] {
        let name = format!("端口 {label}");
        items.push(match probe_port(port, proto) {
            Ok(()) => ok("port", &name, "空闲".into()),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse && service_running => {
                ok("port", &name, "zoffline 服务占用中(正常运行)".into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                let owner = port_owner(port, proto).unwrap_or_else(|| "未识别到进程".into());
                warn("port", &name, format!("本地端口被占用：{owner}"), "请确认占用端口的服务，再重试启动 zoffline。")
            }
            Err(error) => warn("port", &name, format!("无法绑定端口：{error}"), "请检查系统权限或网络限制。"),
        });
    }

    // 5. 证书与 hosts
    let ssl_root = engine.as_ref().map(engine_ssl_root).unwrap_or_default();
    if !ssl_root.as_os_str().is_empty() && cert_in_store(&ssl_root) {
        items.push(ok("cert", "系统根证书", "*.zwift.com 证书已导入".into()));
    } else {
        items.push(warn(
            "cert",
            "系统根证书",
            "未导入 zoffline 证书".into(),
            "点击“一键配置 Zwift”自动导入;Zwift 将无法信任离线服务器。",
        ));
    }
    if hosts_has_entry() {
        items.push(ok("hosts", "hosts 重定向", "zwift.com 域名已指向本机".into()));
    } else {
        items.push(warn(
            "hosts",
            "hosts 重定向",
            "hosts 未配置".into(),
            "点击“一键配置 Zwift”自动添加;不配置则 Zwift 会连官方服务器。",
        ));
    }

    items
}
