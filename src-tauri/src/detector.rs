use crate::settings::Settings;
use crate::util::*;
use serde::Serialize;
use std::path::PathBuf;

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
pub fn missing_python_deps(python: &(String, Vec<String>, String)) -> Vec<String> {
    let mut cmd = std::process::Command::new(&python.0);
    cmd.args(&python.1).arg("-c").arg(
        "import importlib.util as u; \
         mods=['flask','flask_login','flask_sqlalchemy','gevent','google.protobuf',\
         'Crypto','jwt','dns.resolver','fitdecode','stravalib']; \
         print('\\n'.join(m for m in mods if u.find_spec(m) is None))",
    );
    let out = cmd.output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => vec!["无法执行 Python".into()],
    }
}

#[tauri::command]
pub fn detect_run(s: tauri::State<'_, crate::AppState>, service: tauri::State<'_, crate::service::ServiceState>) -> Vec<CheckItem> {
    let settings = s.settings.lock().unwrap().clone();
    let mut items = Vec::new();

    // 1. 管理员权限
    if is_elevated() {
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
    let python = find_python();
    match &engine {
        Some((Some(src), None)) => {
            let ssl_ok = src.join("ssl/cert-zwift-com.p12").is_file() && src.join("ssl/cert-zwift-com.pem").is_file();
            let mut detail = format!("源码模式:{}", src.display());
            if !ssl_ok {
                detail.push_str("(缺少 ssl 证书文件)");
            }
            items.push(ok("engine", "zoffline 引擎", detail));
            // 3a. Python(源码模式需要)
            match &python {
                Some(p) => {
                    items.push(ok("python", "Python", p.2.clone()));
                    let missing = missing_python_deps(p);
                    if missing.is_empty() {
                        items.push(ok("deps", "Python 依赖", "依赖完整,可以直接启动".into()));
                    } else {
                        items.push(warn(
                            "deps",
                            "Python 依赖",
                            format!("缺少模块:{}", missing.join(", ")),
                            "点击“一键安装依赖”(执行 pip install -r requirements.txt)。",
                        ));
                    }
                }
                None => items.push(fail(
                    "python",
                    "Python",
                    "未检测到 Python 3".into(),
                    "点下方「⬇ 自动安装 Python(推荐)」一键搞定;或手动到 python.org 下载安装(勾选 Add python.exe to PATH);或改用 zoffline.exe 模式(不需要 Python)。",
                )),
            }
        }
        Some((None, Some(exe))) => {
            items.push(ok("engine", "zoffline 引擎", format!("exe 模式:{}", exe.display())));
        }
        _ => items.push(fail(
            "engine",
            "zoffline 引擎",
            "未找到 zoffline".into(),
            "在“设置”里选择 zwift-offline 源码目录(含 standalone.py),或选择 zoffline.exe。源码目录默认与本启动器同级。",
        )),
    }

    // 4. 端口(服务已在运行时 80/443 被占是正常状态)
    let service_running = service.is_running();
    for (port, proto, label) in [(80u16, "tcp", "80 (CDN)"), (443, "tcp", "443 (主服务)"), (3025, "tcp", "3025 (TCP)"), (3024, "udp", "3024 (UDP)")] {
        let free = if proto == "tcp" { tcp_port_free(port) } else { udp_port_free(port) };
        if free {
            items.push(ok("port", &format!("端口 {label}"), "空闲".into()));
        } else if service_running && (port == 80 || port == 443 || port == 3024 || port == 3025) {
            items.push(ok("port", &format!("端口 {label}"), "zoffline 服务占用中(正常运行)".into()));
        } else {
            let owner = port_owner(port, if proto == "tcp" { "tcp" } else { "udp" }).unwrap_or_else(|| "未知".into());
            items.push(warn(
                "port",
                &format!("端口 {label}"),
                format!("被占用:{owner}"),
                "请关闭占用该端口的程序(常见:IIS/其他 Web 服务),否则 zoffline 无法启动。",
            ));
        }
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
