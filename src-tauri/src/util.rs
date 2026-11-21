use std::path::PathBuf;
use std::process::Command;

pub fn is_windows() -> bool {
    cfg!(windows)
}

/// 运行命令并返回 (exit code 是否为 0, stdout+stderr 合并文本)
pub fn run_cmd(cmd: &mut Command) -> (bool, String) {
    let output = cmd.output();
    match output {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), text)
        }
        Err(e) => (false, e.to_string()),
    }
}

pub fn hosts_path() -> PathBuf {
    if is_windows() {
        PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

/// Zwift 域名重定向行里的域名标记(用于幂等判断与还原)
pub const ZOFFLINE_DOMAINS: &str = "us-or-rly101.zwift.com secure.zwift.com cdn.zwift.com launcher.zwift.com";

/// cacert.pem 中已追加过的证书指纹标记(与 configure_client.bat 的判断一致)
pub const CACERT_MARKER: &str = "MIIEQTCCAymgAwIBAgIUVPfyk0BzcKB2eYhXZ+W9WZRY5HEwDQYJKoZIhvcNAQEL";

/// 默认候选目录自动探测 Zwift 安装位置
pub fn zwift_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if is_windows() {
        for env in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(base) = std::env::var(env).ok() {
                let dir = PathBuf::from(base).join("Zwift");
                if dir.join("data").is_dir() {
                    out.push(dir);
                }
            }
        }
        // 注册表卸载信息兜底
        #[cfg(windows)]
        {
            use winreg::enums::*;
            use winreg::RegKey;
            for root in [HKEY_LOCAL_MACHINE] {
                for sub in [
                    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Zwift",
                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Zwift",
                ] {
                    if let Ok(key) = RegKey::predef(root).open_subkey(sub) {
                        if let Some(loc) = key.get_value::<String, _>("InstallLocation").ok() {
                            if !loc.is_empty() {
                                let dir = PathBuf::from(&loc);
                                if dir.join("data").is_dir() {
                                    out.push(dir);
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        for base in ["/Applications", "~/Applications"] {
            let expanded = if let Some(p) = base.strip_prefix("~/") {
                if let Some(home) = std::env::var("HOME").ok() {
                    PathBuf::from(home).join(p)
                } else {
                    continue;
                }
            } else {
                PathBuf::from(base)
            };
            if expanded.join("Zwift.app").is_dir() {
                out.push(expanded.join("Zwift.app"));
            }
        }
    }
    out
}

/// Zwift 目录 -> 它使用的 cacert.pem 路径
/// Windows: <dir>\data\cacert.pem
/// macOS:   ~/Library/Application Support/Zwift/data/cacert.pem
pub fn zwift_cacert(zwift_dir: &std::path::Path) -> PathBuf {
    if is_windows() {
        zwift_dir.join("data").join("cacert.pem")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/shared".into());
        PathBuf::from(home)
            .join("Library/Application Support/Zwift/data/cacert.pem")
    }
}

pub type Python = (String, Vec<String>, String);

#[cfg(target_os = "macos")]
const MIN_ENGINE_PYTHON_MINOR: u32 = 11;
#[cfg(target_os = "macos")]
const ENGINE_PYTHON_VERSIONS: &[&str] = &["3.12", "3.11", "3.13", "3.14"];

fn probe_python(program: &str, args: &[&str], min_minor: u32) -> Option<Python> {
    let out = Command::new(program).args(args).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string()
        + &String::from_utf8_lossy(&out.stderr);
    let version = text.lines().find_map(|line| line.trim().strip_prefix("Python 3."))?;
    let minor: u32 = version.split('.').next()?.parse().ok()?;
    if minor < min_minor {
        return None;
    }
    Some((program.to_string(), args.iter().map(|arg| arg.to_string()).collect(), format!("Python 3.{version}")))
}

/// Finder 启动应用时的 PATH 通常不含 Homebrew 或 uv 安装目录。
pub fn find_python() -> Option<Python> {
    if is_windows() {
        return probe_python("py", &["-3"], 0)
            .or_else(|| probe_python("python", &[], 0))
            .or_else(|| probe_python("python3", &[], 0));
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(python) = probe_python("python3", &[], MIN_ENGINE_PYTHON_MINOR) {
            return Some(python);
        }
        for version in ENGINE_PYTHON_VERSIONS {
            for path in [
                format!("/Library/Frameworks/Python.framework/Versions/{version}/bin/python{version}"),
                format!("/opt/homebrew/bin/python{version}"),
                format!("/usr/local/bin/python{version}"),
            ] {
                if let Some(python) = probe_python(&path, &[], MIN_ENGINE_PYTHON_MINOR) {
                    return Some(python);
                }
            }
        }
        let user_uv = home_dir().map(|home| home.join(".local/bin/uv"));
        for uv in [PathBuf::from("/opt/homebrew/bin/uv"), PathBuf::from("/usr/local/bin/uv")]
            .into_iter()
            .chain(user_uv)
        {
            for &version in ENGINE_PYTHON_VERSIONS {
                let Ok(result) = Command::new(&uv)
                    .args(["--no-python-downloads", "python", "find", "--no-project", "--system", version])
                    .output()
                else { break };
                if result.status.success() {
                    if let Some(python) = probe_python(String::from_utf8_lossy(&result.stdout).trim(), &[], MIN_ENGINE_PYTHON_MINOR) {
                        return Some(python);
                    }
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    probe_python("python3", &[], 0).or_else(|| probe_python("python", &[], 0))
}

#[cfg(target_os = "macos")]
pub const ENGINE_VENV: &str = ".zoffline-launcher-venv";

#[cfg(target_os = "macos")]
pub fn engine_venv_python(engine: &std::path::Path) -> Option<Python> {
    probe_python(&engine.join(ENGINE_VENV).join("bin/python").to_string_lossy(), &[], MIN_ENGINE_PYTHON_MINOR)
}

pub fn find_engine_python(engine: &std::path::Path) -> Option<Python> {
    #[cfg(target_os = "macos")]
    if let Some(python) = engine_venv_python(engine) {
        return Some(python);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = engine;
    find_python()
}

/// 当前进程是否有管理员/root 权限
pub fn is_elevated() -> bool {
    if is_windows() {
        let (ok, _) = run_cmd(Command::new("net").arg("session"));
        ok
    } else {
        let (ok, text) = run_cmd(Command::new("id").arg("-u"));
        ok && text.trim() == "0"
    }
}

/// hosts 文件里是否已有 zoffline 重定向行
pub fn hosts_has_entry() -> bool {
    std::fs::read_to_string(hosts_path())
        .map(|c| c.contains("us-or-rly101.zwift.com"))
        .unwrap_or(false)
}

/// 拆出 chunk 的行内容与行尾(\r\n / \n / 无),保留 hosts 原有行尾风格
pub fn split_eol(chunk: &str) -> (&str, &str) {
    if let Some(s) = chunk.strip_suffix("\r\n") {
        (s, "\r\n")
    } else if let Some(s) = chunk.strip_suffix('\n') {
        (s, "\n")
    } else {
        (chunk, "")
    }
}

/// 临时挂起 hosts 里的 zwift 劫持行(加注释前缀),返回是否有改动。
/// 用于打开官方登录窗口/真实验证:配置过 zoffline 的机器 secure.zwift.com
/// 指向 127.0.0.1,不挂起的话登录页/验证会打到本地假服务上。
/// 挂起 = 给行加 "# zl-suspended " 前缀;恢复 = hosts_restore_suspended() 去掉前缀。
/// 无状态设计:进程崩溃也不会丢备份,下次启动自动清残留。保留原行尾(CRLF/LF)。
pub fn hosts_suspend_zwift() -> Result<bool, String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取 hosts 失败: {e}"))?;
    let mut changed = false;
    let mut out = String::new();
    for chunk in content.split_inclusive('\n') {
        let (line, eol) = split_eol(chunk);
        let t = line.trim_start();
        if !t.starts_with('#') && line.contains("secure.zwift.com") {
            out.push_str(&format!("# zl-suspended {line}{eol}"));
            changed = true;
        } else {
            out.push_str(chunk);
        }
    }
    if !changed {
        return Ok(false);
    }
    std::fs::write(&path, out).map_err(|e| format!("写 hosts 失败(需要管理员): {e}"))?;
    Ok(true)
}

/// 恢复被挂起的 hosts 行(去掉 "# zl-suspended " 前缀),幂等可随时调用
pub fn hosts_restore_suspended() -> Result<bool, String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取 hosts 失败: {e}"))?;
    if !content.contains("# zl-suspended ") {
        return Ok(false);
    }
    let mut out = String::new();
    for chunk in content.split_inclusive('\n') {
        let (line, eol) = split_eol(chunk);
        if let Some(rest) = line.strip_prefix("# zl-suspended ") {
            out.push_str(rest);
            out.push_str(eol);
        } else {
            out.push_str(chunk);
        }
    }
    std::fs::write(&path, out).map_err(|e| format!("写 hosts 失败: {e}"))?;
    Ok(true)
}

/// Windows 根证书库里 zoffline 证书的指纹(历史版本,仅兜底用)
pub const CERT_THUMBPRINT_LEGACY: &str = "54f7f293407370a07679885767e5bd599458e471";

/// 从引擎的 ssl/cert-zwift-com.pem 计算 certs 指纹(sha1(DER)),
/// 这才是 certutil -store 里显示的"证书哈希(sha1)",旧脚本的硬编码常量已过时。
pub fn pem_cert_thumbprint(engine: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(engine.join("ssl/cert-zwift-com.pem")).ok()?;
    let mut in_cert = false;
    let mut b64 = String::new();
    for line in content.lines() {
        if line.contains("BEGIN CERTIFICATE") {
            in_cert = true;
            continue;
        }
        if line.contains("END CERTIFICATE") {
            break;
        }
        if in_cert {
            b64.push_str(line.trim());
        }
    }
    if b64.is_empty() {
        return None;
    }
    use base64::Engine;
    use sha1::{Digest, Sha1};
    let der = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .ok()?;
    let mut h = Sha1::new();
    h.update(&der);
    let digest = h.finalize();
    Some(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// 根证书库里是否有 zoffline 证书(指纹从引擎 pem 动态计算)
pub fn cert_in_store(engine: &std::path::Path) -> bool {
    let Some(tp) = pem_cert_thumbprint(engine) else {
        return false;
    };
    if is_windows() {
        let (ok, text) = run_cmd(Command::new("certutil").arg("-store").arg("Root"));
        let normalized: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        ok && normalized.to_lowercase().contains(&tp)
    } else {
        let (ok, _) = run_cmd(Command::new("security").args([
            "find-certificate",
            "-c",
            "*.zwift.com",
            "/Library/Keychains/System.keychain",
        ]));
        ok
    }
}

/// 端口能否绑定(能绑 = 没被占用)
pub fn probe_port(port: u16, proto: &str) -> std::io::Result<()> {
    // macOS 对指定 IP 的低端口有权限限制。引擎默认监听通配地址。
    let host = if cfg!(target_os = "macos") { "0.0.0.0" } else { "127.0.0.1" };
    if proto == "tcp" {
        std::net::TcpListener::bind((host, port)).map(drop)
    } else {
        std::net::UdpSocket::bind((host, port)).map(drop)
    }
}

// ---------- 引擎自动寻找 ----------

pub(crate) fn is_engine_dir(p: &std::path::Path) -> bool {
    p.is_dir() && p.join("standalone.py").is_file() && p.join("zwift_offline.py").is_file()
}

fn home_dir() -> Option<PathBuf> {
    if is_windows() {
        std::env::var("USERPROFILE").ok().map(PathBuf::from).filter(|p| p.is_absolute())
    } else {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// 在 base_dir(启动器安装目录)附近找引擎目录。
/// 优先级:启动器同级(老用户的引擎带着 storage 骑行数据)> 安装目录内的内置引擎
/// > 各级祖先的子目录。安装目录内部(第 0 级)通常是安装包自带的全新引擎,
/// 若同级已存在用户自己的引擎,必须让老数据优先。
fn engine_around(base_dir: &std::path::Path) -> Option<PathBuf> {
    // 第一轮:从安装目录的上级开始,逐级找"同级 zwift-offline / 自身就是引擎 / 子目录引擎"。
    // 注意:爬到盘符根目录时 parent() 为 None,必须 break —— 不能用 `?` 提前返回,
    // 否则第二轮(内置引擎)永远不会执行(真实踩坑:装得深的目录永远找不到内置引擎)
    if let Some(parent) = base_dir.parent() {
        let mut ancestor = parent.to_path_buf();
        for _ in 0..6 {
            if is_engine_dir(&ancestor) {
                return Some(ancestor);
            }
            let candidate = ancestor.join("zwift-offline");
            if is_engine_dir(&candidate) {
                return Some(candidate);
            }
            if let Ok(rd) = std::fs::read_dir(&ancestor) {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if is_engine_dir(&path) {
                        return Some(path);
                    }
                }
            }
            match ancestor.parent() {
                Some(p) => ancestor = p.to_path_buf(),
                None => break,
            }
        }
    }
    // 第二轮:安装目录内部(内置引擎,全新无数据,优先级最低)。
    // resources 数组打包会保留相对结构,落在 engine/zwift-offline/
    for bundled in [base_dir.join("zwift-offline"), base_dir.join("engine").join("zwift-offline")] {
        if is_engine_dir(&bundled) {
            return Some(bundled);
        }
    }
    if let Ok(rd) = std::fs::read_dir(base_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if is_engine_dir(&path) {
                return Some(path);
            }
        }
    }
    None
}

fn engine_around_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?.to_path_buf();
    engine_around(&exe_dir)
}

/// 自动寻找 zoffline 源码目录
pub fn find_engine_source() -> Option<PathBuf> {
    if let Some(p) = engine_around_exe() {
        return Some(p);
    }
    if let Some(home) = home_dir() {
        for rel in ["zwift-offline", "Documents/zwift-offline", "Desktop/zwift-offline", "Downloads/zwift-offline"] {
            let p = home.join(rel);
            if is_engine_dir(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// 自动寻找 zoffline.exe
#[cfg(not(target_os = "macos"))]
pub fn find_engine_exe() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let mut ancestor = exe.parent()?.to_path_buf();
        for _ in 0..6 {
            let direct = ancestor.join("zoffline.exe");
            if direct.is_file() {
                return Some(direct);
            }
            if let Ok(rd) = std::fs::read_dir(&ancestor) {
                for entry in rd.flatten() {
                    let p = entry.path().join("zoffline.exe");
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
            ancestor = ancestor.parent()?.to_path_buf();
        }
    }
    if let Some(home) = home_dir() {
        for rel in ["Downloads/zoffline.exe", "Desktop/zoffline.exe", "Documents/zoffline.exe"] {
            let p = home.join(rel);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 服务健康探活:测 80 端口(zoffline 的 CDN,纯 HTTP)。
/// 不要探 443:裸 TCP 连了就断会让 gevent 在 SSL 握手时抛 SSLEOFError,
/// 服务端日志每次探活都刷一条吓人的 traceback(80/443 同进程,等价判定)。
pub fn https_alive() -> bool {
    std::net::TcpStream::connect_timeout(
        &"127.0.0.1:80".parse().unwrap(),
        std::time::Duration::from_millis(1200),
    )
    .is_ok()
}

/// 查端口占用者:netstat/lsof 输出 "pid/name" 之类的人类可读信息
pub fn port_owner(port: u16, proto: &str) -> Option<String> {
    if is_windows() {
        let (_ok, text) = run_cmd(Command::new("netstat").args(["-ano", "-p", proto]));
        for line in text.lines() {
            if line.contains(&format!(":{port} ")) {
                let pid = line.split_whitespace().last()?;
                let (_ok2, tl) = run_cmd(Command::new("tasklist").args(["/FI", &format!("PID eq {pid}")]));
                let name = tl
                    .lines()
                    .skip(2)
                    .next()
                    .and_then(|l| l.split_whitespace().next().map(|s| s.to_string()))
                    .unwrap_or_else(|| "未知进程".into());
                return Some(format!("PID {pid} ({name})"));
            }
        }
        None
    } else {
        let mut cmd = Command::new("/usr/sbin/lsof");
        cmd.args(["-nP", "-a", &format!("-i{}:{port}", proto.to_ascii_uppercase())]);
        if proto == "tcp" {
            cmd.arg("-sTCP:LISTEN");
        }
        let (ok, text) = run_cmd(&mut cmd);
        if !ok {
            return None;
        }
        text.lines()
            .skip(1)
            .filter(|line| !line.contains("->"))
            .find_map(|line| {
                let mut columns = line.split_whitespace();
                Some(format!("{} (PID {})", columns.next()?, columns.next()?))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_engine(p: &std::path::Path) {
        std::fs::create_dir_all(p).unwrap();
        std::fs::write(p.join("standalone.py"), b"").unwrap();
        std::fs::write(p.join("zwift_offline.py"), b"").unwrap();
    }

    /// 安装目录里只有内置引擎(engine/zwift-offline)时能被发现
    #[test]
    fn finds_bundled_engine() {
        let base = std::env::temp_dir().join("zl_eng_test1");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        mk_engine(&base.join("engine").join("zwift-offline"));
        let found = engine_around(&base).expect("应找到内置引擎");
        assert!(found.ends_with("engine\\zwift-offline") || found.ends_with("engine/zwift-offline"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 同级 zwift-offline(带用户数据)优先于安装目录内的内置引擎
    #[test]
    fn prefers_sibling_over_bundled() {
        let root = std::env::temp_dir().join("zl_eng_test2");
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("zoffline-launcher");
        std::fs::create_dir_all(&base).unwrap();
        mk_engine(&base.join("engine").join("zwift-offline")); // 内置(全新)
        mk_engine(&root.join("zwift-offline")); // 同级(用户数据)
        let found = engine_around(&base).expect("应找到引擎");
        assert!(found == root.join("zwift-offline"), "同级老数据引擎必须优先,实际: {found:?}");
        let _ = std::fs::remove_dir_all(&root);
    }


    /// hosts 挂起/恢复往返:挂起后 zwift 行被注释,恢复后与原文一致。
    /// 直接操作真 hosts 文件(当前环境为管理员,可写),跑完即恢复。
    #[test]
    fn hosts_suspend_restore_roundtrip() {
        let before = std::fs::read_to_string(hosts_path()).unwrap();
        // 挂起:若文件里没有 zwift 劫持行,suspend 返回 false 且不改文件
        let changed = hosts_suspend_zwift().unwrap();
        let suspended = std::fs::read_to_string(hosts_path()).unwrap();
        if changed {
            assert!(suspended != before, "挂起后 hosts 应有变化");
            assert!(
                suspended
                    .lines()
                    .filter(|l| l.contains("secure.zwift.com"))
                    .all(|l| l.trim_start().starts_with('#')),
                "挂起后所有含 secure.zwift.com 的行都应被注释"
            );
            hosts_restore_suspended().unwrap();
        } else {
            assert_eq!(suspended, before, "无劫持行时挂起不应改动文件");
        }
        // 无论挂起与否,恢复后必须与最初一致
        assert_eq!(std::fs::read_to_string(hosts_path()).unwrap(), before, "恢复后 hosts 必须与原文一致");
    }
}
