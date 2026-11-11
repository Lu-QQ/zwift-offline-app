use crate::detector::resolve_engine;
use crate::settings::Settings;
use serde::Deserialize;
use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[derive(Serialize)]
pub struct StravaStatus {
    pub configured: bool,
    pub token_file: Option<String>,
    pub expires_at: Option<i64>,
    pub expired: bool,
    pub player_id: Option<String>,
}

/// 找到 storage 下含 profile.bin 的玩家目录(单玩家模式与 zwift_offline.py 的逻辑一致)
pub fn player_dir(settings: &Settings) -> Option<PathBuf> {
    let (src, exe) = resolve_engine(settings)?;
    let storage = match (src, exe) {
        (Some(s), _) => s.join("storage"),
        (None, Some(e)) => e.parent()?.join("storage"),
        _ => return None,
    };
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(&storage).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if !path.is_dir() || !path.join("profile.bin").is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.parse::<u64>().is_err() {
            continue;
        }
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if best.as_ref().map(|(m, _)| mtime > *m).unwrap_or(true) {
            best = Some((mtime, path));
        }
    }
    best.map(|(_, p)| p)
}

fn read_token_meta(path: &PathBuf) -> Option<i64> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut lines = content.lines();
    let _client_id = lines.next()?;
    let _secret = lines.next()?;
    let _access = lines.next()?;
    let _refresh = lines.next()?;
    lines.next()?.trim().parse().ok()
}

#[tauri::command]
pub fn strava_status(s: tauri::State<'_, crate::AppState>) -> StravaStatus {
    let settings = s.settings.lock().unwrap().clone();
    let Some(pdir) = player_dir(&settings) else {
        return StravaStatus { configured: false, token_file: None, expires_at: None, expired: false, player_id: None };
    };
    let token_file = pdir.join("strava_token.txt");
    if !token_file.is_file() {
        return StravaStatus {
            configured: false,
            token_file: None,
            expires_at: None,
            expired: false,
            player_id: pdir.file_name().map(|n| n.to_string_lossy().to_string()),
        };
    }
    let expires_at = read_token_meta(&token_file);
    let expired = expires_at
        .map(|e| e < chrono_now())
        .unwrap_or(false);
    StravaStatus {
        configured: true,
        token_file: Some(token_file.to_string_lossy().to_string()),
        expires_at,
        expired,
        player_id: pdir.file_name().map(|n| n.to_string_lossy().to_string()),
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

const SUCCESS_PAGE: &str = "<html><head><meta charset=\"utf-8\"><title>授权成功</title></head>\
<body style=\"font-family:sans-serif;text-align:center;padding-top:60px\">\
<h2>✅ Strava 授权成功</h2><p>已自动保存令牌,现在可以关闭此页面并回到启动器。</p></body></html>";
const ERROR_PAGE: &str = "<html><head><meta charset=\"utf-8\"></head>\
<body style=\"font-family:sans-serif;text-align:center;padding-top:60px\">\
<h2>❌ 授权失败</h2><p>请回到启动器查看错误信息。</p></body></html>";

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

fn handle_request(stream: &mut TcpStream) -> Result<Option<String>, String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| e.to_string())?;
    let mut url = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
    if url.starts_with("http") {
        // 绝对 URI 形式,取 path+query
        url = url.splitn(4, '/').nth(3).map(|rest| format!("/{rest}")).unwrap_or(url);
    }
    let mut success = false;
    let mut code: Option<String> = None;
    if url.starts_with("/authorization") {
        if let Some(q) = url.split('?').nth(1) {
            for kv in q.split('&') {
                let (k, v) = kv.split_once('=').unwrap_or(("", ""));
                if k == "code" {
                    code = Some(v.to_string());
                    success = true;
                }
            }
        }
    }
    let body = if success { SUCCESS_PAGE } else { ERROR_PAGE };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
    if success {
        Ok(code)
    } else if url == "/favicon.ico" {
        Ok(None) // 忽略 favicon
    } else {
        Err("浏览器回调里没有授权码(可能取消了授权)".into())
    }
}

#[tauri::command]
pub fn strava_authorize(
    app: AppHandle,
    s: tauri::State<'_, crate::AppState>,
    client_id: String,
    client_secret: String,
) -> Result<String, String> {
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Err("请填写 Strava 应用的 Client ID 和 Client Secret".into());
    }
    let settings: Settings = s.settings.lock().unwrap().clone();
    let port = settings.strava_port_or_default();
    let pdir = player_dir(&settings)
        .ok_or("未找到玩家档案目录(storage 下含 profile.bin 的目录)。请先把服务跑起来、进一次游戏生成档案,再做授权。")?;

    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("监听 127.0.0.1:{port} 失败: {e}(换个回调端口试试)"))?;

    let redirect = format!("http://localhost:{port}/authorization");
    let auth_url = format!(
        "https://www.strava.com/oauth/authorize?client_id={client_id}&redirect_uri={redirect}&response_type=code&approval_prompt=force&scope=read,activity:write"
    );
    app.opener()
        .open_url(auth_url, None::<&str>)
        .map_err(|e| format!("无法打开浏览器: {e}"))?;

    // 5 分钟超时等待回调
    listener
        .set_nonblocking(false)
        .map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut deadline = 60 * 5;
        while deadline > 0 {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    match handle_request(&mut stream) {
                        Ok(Some(code)) => {
                            let _ = tx.send(Ok(code));
                            return;
                        }
                        Ok(None) => {} // favicon 等无关请求
                        Err(e) => {
                            // 无关请求不算致命,继续等真正的授权回调
                            let _ = tx.send(Err(e.to_string()));
                            return;
                        }
                    }
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_secs(1));
                    deadline -= 1;
                }
            }
        }
        let _ = tx.send(Err("等待授权超时(5 分钟)".into()));
    });

    let code = rx
        .recv_timeout(Duration::from_secs(60 * 5 + 10))
        .map_err(|_| "等待授权超时".to_string())?
        .map_err(|e| format!("授权失败: {e}"))?;

    // 换取 token
    let resp = ureq::post("https://www.strava.com/oauth/token")
        .send_form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .map_err(|e| format!("向 Strava 换取令牌失败: {e}(检查 Client ID/Secret 是否正确)"))?;
    let tokens: TokenResponse = resp
        .into_json()
        .map_err(|e| format!("解析 Strava 响应失败: {e}"))?;

    let token_file = pdir.join("strava_token.txt");
    let content = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        client_id, client_secret, tokens.access_token, tokens.refresh_token, tokens.expires_at
    );
    std::fs::write(&token_file, content).map_err(|e| format!("写入令牌失败: {e}"))?;

    Ok(format!(
        "授权成功!令牌已写入 {}(到期时间戳 {},启动器会在过期时自动刷新)",
        token_file.display(),
        tokens.expires_at
    ))
}
