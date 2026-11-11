use crate::detector::{resolve_engine, resolve_zwift_dir};
use crate::settings::Settings;
use crate::util::*;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct StepResult {
    pub id: String,
    pub name: String,
    /// done=已修改 / skipped=原本就满足 / error=失败
    pub status: String,
    pub detail: String,
}

fn emit_progress(app: &AppHandle, r: &StepResult) {
    let _ = app.emit("config:progress", r.clone());
}

fn done(id: &str, name: &str, detail: String) -> StepResult {
    StepResult { id: id.into(), name: name.into(), status: "done".into(), detail }
}
fn skipped(id: &str, name: &str, detail: String) -> StepResult {
    StepResult { id: id.into(), name: name.into(), status: "skipped".into(), detail }
}
fn errored(id: &str, name: &str, detail: String) -> StepResult {
    StepResult { id: id.into(), name: name.into(), status: "error".into(), detail }
}

/// 首次修改前做 .zoffline-launcher.bak 备份
fn backup_file(path: &Path) -> Result<(), String> {
    let bak = path.with_extension(format!(
        "{}.zoffline-launcher.bak",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    if !bak.exists() {
        std::fs::copy(path, &bak).map_err(|e| format!("备份失败 {}: {e}", bak.display()))?;
    }
    Ok(())
}

fn backup_path_of(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}.zoffline-launcher.bak",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ))
}

// ---------- hosts ----------

fn hosts_configure() -> StepResult {
    let id = "hosts";
    let name = "hosts 重定向";
    if hosts_has_entry() {
        return skipped(id, name, "hosts 已包含 zoffline 重定向".into());
    }
    let hosts = hosts_path();
    let content = std::fs::read_to_string(&hosts).map_err(|e| format!("读取 hosts 失败: {e}"));
    let content = match content {
        Ok(c) => c,
        Err(e) => return errored(id, name, e),
    };
    if let Err(e) = backup_file(&hosts) {
        return errored(id, name, e);
    }
    // 去掉可能残留的旧 zwift 行(比如手工加过一部分),再统一追加(保留原行尾风格)
    let mut cleaned = String::new();
    for chunk in content.split_inclusive('\n') {
        let (line, _eol) = crate::util::split_eol(chunk);
        let t = line.trim_start();
        if line.contains("zwift.com") && !t.starts_with('#') {
            continue;
        }
        cleaned.push_str(chunk);
    }
    let mut new_content = cleaned.trim_end().to_string();
    new_content.push_str("\n\n# zoffline-launcher 开始(还原官方服务器时可自动移除)\n");
    new_content.push_str(&format!("127.0.0.1 {ZOFFLINE_DOMAINS}\n"));
    new_content.push_str("# zoffline-launcher 结束\n");
    match std::fs::write(&hosts, new_content) {
        Ok(_) => done(id, name, format!("已写入 hosts(备份于 {})", backup_path_of(&hosts).display())),
        Err(e) => errored(id, name, format!("写入 hosts 失败: {e}(需要管理员权限)")),
    }
}

fn hosts_restore() -> StepResult {
    let id = "hosts";
    let name = "hosts 还原";
    let hosts = hosts_path();
    if !hosts_has_entry() {
        return skipped(id, name, "hosts 本来就没有 zoffline 配置".into());
    }
    let content = match std::fs::read_to_string(&hosts) {
        Ok(c) => c,
        Err(e) => return errored(id, name, format!("读取 hosts 失败: {e}")),
    };
    // 移除两段标记之间的内容,以及任何含 zwift 域名的非注释行(保留原行尾风格)
    let mut filtered = String::new();
    let mut in_block = false;
    for chunk in content.split_inclusive('\n') {
        let (line, _eol) = crate::util::split_eol(chunk);
        if line.contains("# zoffline-launcher 开始") {
            in_block = true;
            continue;
        }
        if line.contains("# zoffline-launcher 结束") {
            in_block = false;
            continue;
        }
        if in_block {
            continue;
        }
        if !line.trim_start().starts_with('#') && line.contains("us-or-rly101.zwift.com") {
            continue;
        }
        filtered.push_str(chunk);
    }
    let new_content = filtered;
    match std::fs::write(&hosts, new_content) {
        Ok(_) => done(id, name, "已从 hosts 移除 zoffline 重定向".into()),
        Err(e) => errored(id, name, format!("写入 hosts 失败: {e}(需要管理员权限)")),
    }
}

// ---------- 系统根证书 ----------

fn cert_configure(engine: &Path) -> StepResult {
    let id = "cert";
    let name = "系统根证书";
    if cert_in_store(engine) {
        return skipped(id, name, "根证书库里已有 zoffline 证书".into());
    }
    if is_windows() {
        let p12 = engine.join("ssl/cert-zwift-com.p12");
        if !p12.is_file() {
            return errored(id, name, format!("找不到 {}", p12.display()));
        }
        let (ok, out) = run_cmd(
            std::process::Command::new("certutil")
                .arg("-importpfx")
                .arg("Root")
                .arg(&p12),
        );
        if ok {
            done(id, name, "证书已导入系统根证书库".into())
        } else {
            errored(id, name, format!("certutil 导入失败: {out}"))
        }
    } else {
        let pem = engine.join("ssl/cert-zwift-com.pem");
        if !pem.is_file() {
            return errored(id, name, format!("找不到 {}", pem.display()));
        }
        let pem = pem.to_string_lossy().to_string();
        let script = format!(
            "security import '{}' -k /Library/Keychains/System.keychain && \
             security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain '{}'",
            pem, pem
        );
        let (ok, out) = run_cmd(std::process::Command::new("osascript").args([
            "-e",
            &format!("do shell script \"{}\" with administrator privileges", script.replace('"', "\\\"")),
        ]));
        if ok {
            done(id, name, "证书已导入并信任(钥匙串)".into())
        } else {
            errored(id, name, format!("security 导入失败: {out}"))
        }
    }
}

fn cert_restore(engine: &Path) -> StepResult {
    let id = "cert";
    let name = "系统根证书还原";
    if !cert_in_store(engine) {
        return skipped(id, name, "根证书库里本来就没有 zoffline 证书".into());
    }
    if is_windows() {
        // 优先用引擎 pem 算出的指纹,算不出就退回历史指纹
        let thumbprint = pem_cert_thumbprint(engine).unwrap_or_else(|| CERT_THUMBPRINT_LEGACY.to_string());
        let (ok, out) = run_cmd(
            std::process::Command::new("certutil")
                .args(["-delstore", "Root"])
                .arg(&thumbprint),
        );
        if ok {
            done(id, name, format!("已从根证书库删除 zoffline 证书({thumbprint})"))
        } else {
            errored(id, name, format!("certutil 删除失败: {out}"))
        }
    } else {
        let (ok, out) = run_cmd(std::process::Command::new("osascript").args([
            "-e",
            "do shell script \"security delete-certificate -c '*.zwift.com'\" with administrator privileges",
        ]));
        if ok {
            done(id, name, "已从钥匙串删除 zoffline 证书".into())
        } else {
            errored(id, name, format!("security 删除失败: {out}"))
        }
    }
}

// ---------- Zwift cacert.pem ----------

fn cacert_configure_with(zwift_dir: &Path, engine: &Path) -> StepResult {
    let id = "cacert";
    let name = "Zwift 内置证书链";
    let cacert = zwift_cacert(zwift_dir);
    if !cacert.is_file() {
        return errored(id, name, format!("找不到 {}", cacert.display()));
    }
    let content = std::fs::read_to_string(&cacert).unwrap_or_default();
    if content.contains(CACERT_MARKER) {
        return skipped(id, name, "cacert.pem 已包含 zoffline 证书".into());
    }
    let pem_path = engine.join("ssl/cert-zwift-com.pem");
    let pem = match std::fs::read_to_string(&pem_path) {
        Ok(p) => p,
        Err(e) => return errored(id, name, format!("读取 {} 失败: {e}", pem_path.display())),
    };
    if let Err(e) = backup_file(&cacert) {
        return errored(id, name, e);
    }
    let mut new_content = content.trim_end().to_string();
    new_content.push_str("\n\n# zoffline-launcher 追加开始\n");
    new_content.push_str(&pem);
    new_content.push_str("\n# zoffline-launcher 追加结束\n");
    match std::fs::write(&cacert, new_content) {
        Ok(_) => done(id, name, format!("已追加证书(备份于 {})", backup_path_of(&cacert).display())),
        Err(e) => errored(id, name, format!("写入失败: {e}(Zwift 装在受保护目录时需要管理员权限)")),
    }
}

fn cacert_restore(zwift_dir: &Path) -> StepResult {
    let id = "cacert";
    let name = "Zwift 内置证书链还原";
    let cacert = zwift_cacert(zwift_dir);
    if !cacert.is_file() {
        return skipped(id, name, "未找到 cacert.pem,跳过".into());
    }
    let bak = backup_path_of(&cacert);
    if bak.is_file() {
        return match std::fs::copy(&bak, &cacert) {
            Ok(_) => done(id, name, format!("已从备份 {} 还原", bak.display())),
            Err(e) => errored(id, name, format!("还原失败: {e}")),
        };
    }
    let content = std::fs::read_to_string(&cacert).unwrap_or_default();
    if !content.contains(CACERT_MARKER) {
        return skipped(id, name, "cacert.pem 未被修改过".into());
    }
    // 没有备份但被改过:按标记截掉追加段
    let mut filtered: Vec<&str> = Vec::new();
    let mut in_block = false;
    for line in content.lines() {
        if line.contains("# zoffline-launcher 追加开始") {
            in_block = true;
            continue;
        }
        if line.contains("# zoffline-launcher 追加结束") {
            in_block = false;
            continue;
        }
        if in_block {
            continue;
        }
        filtered.push(line);
    }
    match std::fs::write(&cacert, filtered.join("\n")) {
        Ok(_) => done(id, name, "已移除追加的证书段".into()),
        Err(e) => errored(id, name, format!("写入失败: {e}")),
    }
}

// ---------- 一键配置 / 一键还原 ----------

fn engine_ssl_root(engine: &(Option<PathBuf>, Option<PathBuf>)) -> PathBuf {
    crate::detector::engine_ssl_root(engine)
}

#[tauri::command]
pub fn config_apply(app: AppHandle, s: tauri::State<'_, crate::AppState>) -> Vec<StepResult> {
    let settings: Settings = s.settings.lock().unwrap().clone();
    let mut results = Vec::new();

    let engine = resolve_engine(&settings);
    let Some(engine) = engine else {
        let r = errored("engine", "zoffline 引擎", "未配置引擎,请先在设置里选择源码目录或 exe".into());
        emit_progress(&app, &r);
        return vec![r];
    };
    let ssl_root = engine_ssl_root(&engine);

    // hosts
    let r = hosts_configure();
    emit_progress(&app, &r);
    results.push(r);

    // 系统证书
    let r = cert_configure(&ssl_root);
    emit_progress(&app, &r);
    results.push(r);

    // Zwift cacert.pem
    match resolve_zwift_dir(&settings) {
        Some(zdir) => {
            let r = cacert_configure_with(&zdir, &ssl_root);
            emit_progress(&app, &r);
            results.push(r);
        }
        None => {
            let r = errored("cacert", "Zwift 内置证书链", "未找到 Zwift 安装目录,跳过".into());
            emit_progress(&app, &r);
            results.push(r);
        }
    }

    // 结束时杀掉 Zwift 启动器,让它重新读取 cacert.pem(与 configure_client.bat 一致)
    if is_windows() {
        let _ = run_cmd(std::process::Command::new("taskkill").args(["/F", "/IM", "ZwiftLauncher.exe"]));
    }

    results
}

#[tauri::command]
pub fn restore_run(app: AppHandle, s: tauri::State<'_, crate::AppState>) -> Vec<StepResult> {
    let settings: Settings = s.settings.lock().unwrap().clone();
    let mut results = Vec::new();
    let engine = resolve_engine(&settings);

    let r = hosts_restore();
    emit_progress(&app, &r);
    results.push(r);

    match engine.as_ref().map(engine_ssl_root) {
        Some(ssl_root) if !ssl_root.as_os_str().is_empty() => {
            let r = cert_restore(&ssl_root);
            emit_progress(&app, &r);
            results.push(r);
        }
        _ => {
            let r = errored("cert", "系统根证书还原", "未配置引擎,无法计算证书指纹,请先在设置里选择引擎".into());
            emit_progress(&app, &r);
            results.push(r);
        }
    }

    match resolve_zwift_dir(&settings) {
        Some(zdir) => {
            let r = cacert_restore(&zdir);
            emit_progress(&app, &r);
            results.push(r);
        }
        None => {
            let r = skipped("cacert", "Zwift 内置证书链还原", "未找到 Zwift 安装目录,跳过".into());
            emit_progress(&app, &r);
            results.push(r);
        }
    }

    results
}

// ---------- Python 自动安装 ----------

const PYTHON_INSTALLER: &str = "python-3.12.9-amd64.exe";
const PYTHON_MIRRORS: &[&str] = &[
    "https://mirrors.huaweicloud.com/python/3.12.9/python-3.12.9-amd64.exe",
    "https://www.python.org/ftp/python/3.12.9/python-3.12.9-amd64.exe",
];

fn emit_log(app: &AppHandle, detail: String) {
    let _ = app.emit("config:progress", StepResult {
        id: "python".into(),
        name: "Python".into(),
        status: "log".into(),
        detail,
    });
}

/// 自动安装 Python:国内镜像优先下载 → 静默安装(PrependPath)→ 复检。
/// 不打包进安装包:体积会从 2MB 涨到 28MB+,且 pip 依赖反正要联网。
#[tauri::command]
pub fn install_python(app: AppHandle) -> Result<String, String> {
    if find_python().is_some() {
        return Ok("检测到 Python 已安装,无需重复安装".into());
    }

    let tmp = std::env::temp_dir().join(PYTHON_INSTALLER);
    let mut last_err = String::new();
    for url in PYTHON_MIRRORS {
        emit_log(&app, format!("开始下载 {PYTHON_INSTALLER}(约 27MB)"));
        match download_with_progress(&app, url, &tmp) {
            Ok(size) => {
                emit_log(&app, format!("下载完成({size:.1} MB),开始静默安装(约 1-2 分钟,请不要关闭启动器)"));
                let status = std::process::Command::new(&tmp)
                    .args(["/quiet", "InstallAllUsers=1", "PrependPath=1", "Include_launcher=1", "Include_test=0"])
                    .status();
                let _ = std::fs::remove_file(&tmp);
                match status {
                    Ok(s) if s.success() => {
                        match find_python() {
                            Some((_, _, version)) => {
                                return Ok(format!("Python 安装成功({version}),可以继续安装依赖了"));
                            }
                            None => {
                                return Err("安装命令已执行但仍未检测到 Python,请重启启动器后再试,或手动安装".into());
                            }
                        }
                    }
                    Ok(s) => return Err(format!("Python 安装器退出码异常: {s}(可能需要管理员权限)")),
                    Err(e) => return Err(format!("无法运行 Python 安装器: {e}")),
                }
            }
            Err(e) => {
                last_err = format!("{url}: {e}");
                emit_log(&app, format!("该源下载失败,换下一个源…({last_err})"));
            }
        }
    }
    Err(format!("所有下载源都失败:{last_err}"))
}

fn download_with_progress(app: &AppHandle, url: &str, dest: &Path) -> Result<f64, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| e.to_string())?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done: u64 = 0;
    let mut last_pct = u8::MAX;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if total > 0 {
            let pct = (done * 100 / total) as u8;
            if pct != last_pct && pct % 5 == 0 {
                last_pct = pct;
                emit_log(app, format!("下载中 {pct}%({:.1}/{:.1} MB)", done as f64 / 1048576.0, total as f64 / 1048576.0));
            }
        }
    }
    drop(file);
    if total > 0 && done != total {
        return Err(format!("下载不完整({done}/{total} 字节)"));
    }
    Ok(done as f64 / 1048576.0)
}

/// 一键安装 Python 依赖(pip install -r requirements.txt)
#[tauri::command]
pub fn install_deps(app: AppHandle, s: tauri::State<'_, crate::AppState>) -> Result<String, String> {
    let settings: Settings = s.settings.lock().unwrap().clone();
    let engine = resolve_engine(&settings)
        .and_then(|(src, _)| src)
        .ok_or("源码模式才能安装依赖,请先在设置里选择引擎源码目录")?;
    let req = engine.join("requirements.txt");
    if !req.is_file() {
        return Err(format!("找不到 {}", req.display()));
    }
    let python = find_python().ok_or("未检测到 Python 3")?;
    let mut cmd = std::process::Command::new(&python.0);
    cmd.args(&python.1)
        .args(["-m", "pip", "install", "-r"])
        .arg(&req)
        .arg("--disable-pip-version-check");
    let (ok, out) = run_cmd_output(cmd, &app);
    if ok {
        Ok("依赖安装完成".into())
    } else {
        Err(format!("pip 安装失败:\n{out}"))
    }
}

/// 带实时输出转发版本的 run_cmd(用于长命令)
fn run_cmd_output(mut cmd: std::process::Command, app: &AppHandle) -> (bool, String) {
    use std::io::{BufRead, BufReader};
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    match cmd.spawn() {
        Ok(mut child) => {
            let mut collected = String::new();
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(|l| l.ok()) {
                    let _ = app.emit("config:progress", StepResult {
                        id: "pip".into(),
                        name: "pip".into(),
                        status: "log".into(),
                        detail: line.clone(),
                    });
                    collected.push_str(&line);
                    collected.push('\n');
                }
            }
            if let Some(err) = child.stderr.take() {
                for line in BufReader::new(err).lines().map_while(|l| l.ok()) {
                    let _ = app.emit("config:progress", StepResult {
                        id: "pip".into(),
                        name: "pip".into(),
                        status: "log".into(),
                        detail: line.clone(),
                    });
                    collected.push_str(&line);
                    collected.push('\n');
                }
            }
            let status = child.wait().map(|s| s.success()).unwrap_or(false);
            (status, collected)
        }
        Err(e) => (false, e.to_string()),
    }
}
