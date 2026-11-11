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

/// 检测 Python:Windows 依次试 py -3 / python,macOS 试 python3。
/// 返回 Some((启动器程序, 参数, 版本号))
pub fn find_python() -> Option<(String, Vec<String>, String)> {
    let try_pair = |program: &str, args: &[&str]| -> Option<(String, Vec<String>, String)> {
        let mut cmd = Command::new(program);
        cmd.args(args).arg("--version");
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        let version = text
            .lines()
            .find(|l| l.contains("Python"))
            .map(|l| l.trim().to_string())
            .unwrap_or(text.trim().to_string());
        if !version.contains('3') {
            return None;
        }
        Some((program.to_string(), args.iter().map(|s| s.to_string()).collect(), version))
    };
    if is_windows() {
        try_pair("py", &["-3"])
            .or_else(|| try_pair("python", &[]))
            .or_else(|| try_pair("python3", &[]))
    } else {
        try_pair("python3", &[]).or_else(|| try_pair("python", &[]))
    }
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
pub fn tcp_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

// ---------- 引擎自动寻找 ----------

fn is_engine_dir(p: &std::path::Path) -> bool {
    p.is_dir() && p.join("standalone.py").is_file() && p.join("zwift_offline.py").is_file()
}

fn home_dir() -> Option<PathBuf> {
    if is_windows() {
        std::env::var("USERPROFILE").ok().map(PathBuf::from).filter(|p| p.is_absolute())
    } else {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// exe 所在位置向上最多 6 级祖先,每级:自身是否引擎目录 / 子目录里有没有引擎目录
fn engine_around_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut ancestor = exe.parent()?.to_path_buf();
    for _ in 0..6 {
        if is_engine_dir(&ancestor) {
            return Some(ancestor);
        }
        let candidate = ancestor.join("zwift-offline");
        if is_engine_dir(&candidate) {
            return Some(candidate);
        }
        // 扫这一级的所有子目录(只看一层)
        if let Ok(rd) = std::fs::read_dir(&ancestor) {
            for entry in rd.flatten() {
                let path = entry.path();
                if is_engine_dir(&path) {
                    return Some(path);
                }
            }
        }
        ancestor = ancestor.parent()?.to_path_buf();
    }
    None
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

pub fn udp_port_free(port: u16) -> bool {
    std::net::UdpSocket::bind(("127.0.0.1", port)).is_ok()
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
        let (_ok, text) = run_cmd(Command::new("lsof").args(["-i", &format!(":{port}")]));
        text.lines().skip(1).next().map(|l| l.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
