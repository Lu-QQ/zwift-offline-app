use crate::settings::Settings;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::{Aes256, Block};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

const LOGIN_URL: &str = "https://www.zwift.com/sign-in?redirect_uri=https%3A%2F%2Fwww.zwift.com%2F";
const LOGIN_LABEL: &str = "zwift-login";

/// 官方登录窗口里的捕获脚本:用户在 Zwift 官方登录页(www.zwift.com/sign-in,
/// React SPA)输入时实时读取账号密码;登录成功后页面会路由离开 /sign-in,
/// 轮询检测到即认为官方验证通过,发 zwift-login-success 事件。
/// 只在 zwift.com 域名的页面上生效;凭据只发回本机启动器。
const CAPTURE_JS: &str = r#"(function(){
  if(!/(^|\.)zwift\.com$/i.test(location.hostname)) return;
  var u='',p='',fired=false;
  function grab(){
    var a=document.querySelector('input[type="email"],input[name="username"],#username');
    var b=document.querySelector('input[type="password"]');
    if(a&&a.value)u=a.value; if(b&&b.value)p=b.value;
  }
  function emit(t,d){ try{ window.__TAURI__.event.emit(t,d); }catch(e){} }
  document.addEventListener('input',function(){ grab(); emit('zwift-cred',{u:u,p:p}); },true);
  document.addEventListener('submit',function(){ grab(); emit('zwift-cred',{u:u,p:p}); },true);
  // SPA 表单不一定触发原生 submit:持续轮询,一旦已捕获凭据且页面离开登录路由即算成功
  setInterval(function(){
    grab();
    if(!fired && u && p && location.pathname.indexOf('/sign-in') === -1){
      fired=true; emit('zwift-login-success',{u:u,p:p});
    }
  },800);
  // 登录页内出现错误提示 → 通知失败并允许重试
  setInterval(function(){
    var err=document.querySelector('.alert-error,[class*="error"] [role="alert"]');
    if(err && err.textContent && err.textContent.length>4 && fired===false){
      emit('zwift-login-failed',{});
    }
  },1500);
})();"#;

/// 打开 Zwift 官方登录窗口(加载真实官方登录页)。
/// 打开期间临时挂起 hosts 里的 zwift 劫持行,否则页面会指向本地 zoffline。
fn debug_log(msg: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("zl_login_debug.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {}", chrono_like_now(), msg);
    }
}

fn chrono_like_now() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("[{ms}]")
}

#[tauri::command]
pub fn zwift_login_window(app: AppHandle) -> Result<(), String> {
    debug_log("zwift_login_window called");
    if let Some(w) = app.get_webview_window(LOGIN_LABEL) {
        let _ = w.close();
    }
    let suspended = crate::util::hosts_suspend_zwift();
    debug_log(&format!("hosts suspend -> {:?}", suspended));
    let win = WebviewWindowBuilder::new(&app, LOGIN_LABEL, WebviewUrl::External(LOGIN_URL.parse().unwrap()))
        .title("登录 Zwift 官方账号")
        .inner_size(520.0, 720.0)
        .initialization_script(CAPTURE_JS)
        .build()
        .map_err(|e| {
            debug_log(&format!("window build error: {e}"));
            // 窗口创建失败也要把 hosts 恢复回来
            let _ = crate::util::hosts_restore_suspended();
            e.to_string()
        })?;
    debug_log("window built");
    // 轮询记录窗口 URL 变化(白屏排查用),60 秒后自动停
    let win_for_log = win.clone();
    std::thread::spawn(move || {
        let mut last = String::new();
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Ok(u) = win_for_log.url() {
                let s = u.to_string();
                if s != last {
                    debug_log(&format!("url -> {s}"));
                    last = s;
                }
            } else {
                debug_log("url() errored");
            }
        }
    });
    let _ = win.set_focus();
    debug_log("focused, done");
    Ok(())
}

/// 关闭登录窗口并恢复 hosts(前端在保存完成后调用;窗口被直接关闭时由
/// lib.rs 的 on_window_event Destroyed 兜底恢复;进程异常退出时下次启动自愈)
#[tauri::command]
pub fn zwift_login_cleanup(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LOGIN_LABEL) {
        let _ = w.close();
    }
    let _ = crate::util::hosts_restore_suspended();
}

/// 与 zwift_offline.py 的 encrypt_credentials 完全一致:
/// AES-256-CFB8(pycryptodome MODE_CFB 默认 8-bit segment),文件 = IV(16字节) + 密文,
/// 明文 = "username\npassword" UTF-8,key = storage/credentials-key.bin(32字节,引擎首启生成)

/// 手写 AES-CFB8(流式,8-bit 反馈),与 pycryptodome AES.MODE_CFB 默认参数逐字节一致
fn cfb8_process(key: &[u8; 32], iv: &[u8; 16], data: &mut [u8], encrypting: bool) {
    let cipher = Aes256::new(key.into());
    let mut state = *iv;
    for b in data.iter_mut() {
        let mut block = Block::clone_from_slice(&state);
        cipher.encrypt_block(&mut block);
        let ks = block[0];
        if encrypting {
            let c = *b ^ ks;
            *b = c;
            state.copy_within(1..16, 0);
            state[15] = c;
        } else {
            let c = *b;
            *b = c ^ ks;
            state.copy_within(1..16, 0);
            state[15] = c;
        }
    }
}

fn engine_root(settings: &Settings) -> Option<PathBuf> {
    let (src, exe) = crate::detector::resolve_engine(settings)?;
    match (src, exe) {
        (Some(s), _) => Some(s),
        (None, Some(e)) => e.parent().map(|p| p.to_path_buf()),
        _ => None,
    }
}

fn ensure_key(root: &PathBuf) -> Result<[u8; 32], String> {
    let path = root.join("storage").join("credentials-key.bin");
    if path.is_file() {
        let data = std::fs::read(&path).map_err(|e| format!("读取 credentials-key.bin 失败: {e}"))?;
        if data.len() != 32 {
            return Err(format!("credentials-key.bin 长度异常({} 字节,应为 32)", data.len()));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&data);
        Ok(key)
    } else {
        // 引擎首启也会生成同样的随机 key;这里提前生成,引擎会直接复用
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|e| format!("生成随机 key 失败: {e}"))?;
        std::fs::create_dir_all(root.join("storage")).ok();
        std::fs::write(&path, key).map_err(|e| format!("写入 credentials-key.bin 失败: {e}"))?;
        Ok(key)
    }
}

fn credentials_path(root: &PathBuf, player_id: &str) -> PathBuf {
    root.join("storage").join(player_id).join("zwift_credentials.bin")
}

fn encrypt_credentials(key: &[u8; 32], username: &str, password: &str) -> Vec<u8> {
    let mut iv = [0u8; 16];
    getrandom::fill(&mut iv).expect("getrandom iv");
    let mut buf = format!("{username}\n{password}").into_bytes();
    cfb8_process(key, &iv, &mut buf, true);
    let mut out = iv.to_vec();
    out.extend_from_slice(&buf);
    out
}

fn decrypt_credentials(key: &[u8; 32], data: &[u8]) -> Option<(String, String)> {
    if data.len() < 16 {
        return None;
    }
    let (iv, rest) = data.split_at(16);
    let iv: [u8; 16] = iv.try_into().ok()?;
    let mut buf = rest.to_vec();
    cfb8_process(key, &iv, &mut buf, false);
    let text = String::from_utf8(buf).ok()?;
    let mut lines = text.splitn(2, '\n');
    let u = lines.next()?.to_string();
    let p = lines.next().unwrap_or("").to_string();
    Some((u, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_to_bytes(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// 与 zwift_offline.py 的 pycryptodome AES.MODE_CFB(默认 8-bit)跨端一致性:
    /// 同样的 key/iv/明文,密文必须逐字节一致
    #[test]
    fn cfb8_matches_pycryptodome() {
        let key_hex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let iv_hex = "101112131415161718191a1b1c1d1e1f";
        let pt_hex = "74657374406578616d706c652e636f6d0a70407373e4b8ade69687e5af86e7a081313233";
        let ct_hex = "9dc437bb903d050713148311f74eb17ddb4b24152133c23cf27c913715232db96b40277b";
        let mut key = [0u8; 32];
        key.copy_from_slice(&hex_to_bytes(key_hex));
        let mut iv = [0u8; 16];
        iv.copy_from_slice(&hex_to_bytes(iv_hex));

        // 加密方向
        let mut buf = hex_to_bytes(pt_hex);
        cfb8_process(&key, &iv, &mut buf, true);
        assert_eq!(hex(&buf), ct_hex, "加密结果与 pycryptodome 不一致");

        // 解密方向
        let mut buf2 = hex_to_bytes(ct_hex);
        cfb8_process(&key, &iv, &mut buf2, false);
        assert_eq!(hex(&buf2), pt_hex, "解密结果与明文不一致");
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// 端到端:Rust 按命令同款逻辑写 key + zwift_credentials.bin,
    /// 再用 zwift_offline.py 的 decrypt_credentials 同款 Python 代码读回验证
    #[test]
    fn credentials_file_roundtrip_with_engine_python() {
        let dir = std::env::temp_dir().join("zl_cred_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let key_path = dir.join("credentials-key.bin");
        let cred_path = dir.join("zwift_credentials.bin");

        let mut key = [0u8; 32];
        getrandom::fill(&mut key).unwrap();
        std::fs::write(&key_path, key).unwrap();
        std::fs::write(&cred_path, encrypt_credentials(&key, "wheel@test.cn", "骑行密码123!")).unwrap();

        let script = format!(
            "from Crypto.Cipher import AES\n\
             key = open(r'{key}', 'rb').read()\n\
             data = open(r'{cred}', 'rb').read()\n\
             cipher = AES.new(key, AES.MODE_CFB, iv=data[:16])\n\
             lines = cipher.decrypt(data[16:]).decode('UTF-8').splitlines()\n\
             print(lines[0] + '|' + lines[1])",
            key = key_path.display(),
            cred = cred_path.display(),
        );
        let out = std::process::Command::new("py")
            .args(["-3", "-c", &script])
            .output()
            .expect("无法运行 py -3");
        assert!(out.status.success(), "python 解密脚本报错: {}", String::from_utf8_lossy(&out.stderr));
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(text, "wheel@test.cn|骑行密码123!", "引擎侧解密结果不匹配: {text}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 用官方 Zwift 登录接口验证账号密码(与 online_sync.py 相同的端点)。
/// 调用前必须挂起 hosts 里的 zwift 劫持行,否则会连到本地假 zoffline 被误验证通过。
fn verify_login(username: &str, password: &str) -> Result<(), String> {
    crate::util::hosts_suspend_zwift()?;
    let result = ureq::post("https://secure.zwift.com/auth/realms/zwift/tokens/access/codes")
        .timeout(std::time::Duration::from_secs(30))
        .send_form(&[
            ("client_id", "Zwift_Mobile_Link"),
            ("grant_type", "password"),
            ("username", username),
            ("password", password),
        ]);
    let _ = crate::util::hosts_restore_suspended();
    match result {
        Ok(resp) if resp.status() == 200 => Ok(()),
        Ok(resp) => Err(format!("官方 Zwift 返回 {}(账号或密码错误?)", resp.status())),
        Err(e) => Err(format!("无法连接官方 Zwift 服务器(需要联网): {e}")),
    }
}

#[derive(Serialize)]
pub struct ZwiftSyncStatus {
    pub configured: bool,
    pub username: Option<String>,
    pub player_id: Option<String>,
}

#[tauri::command]
pub fn zwift_sync_status(s: State<'_, crate::AppState>) -> ZwiftSyncStatus {
    let settings = s.settings.lock().unwrap().clone();
    let Some(root) = engine_root(&settings) else {
        return ZwiftSyncStatus { configured: false, username: None, player_id: None };
    };
    let Some(pdir) = crate::strava::player_dir(&settings) else {
        return ZwiftSyncStatus { configured: false, username: None, player_id: None };
    };
    let player_id = pdir.file_name().map(|n| n.to_string_lossy().to_string());
    let file = credentials_path(&root, player_id.as_deref().unwrap_or(""));
    if !file.is_file() {
        return ZwiftSyncStatus { configured: false, username: None, player_id };
    }
    let username = match ensure_key(&root) {
        Ok(key) => std::fs::read(&file).ok().and_then(|data| decrypt_credentials(&key, &data)),
        Err(_) => None,
    };
    ZwiftSyncStatus {
        configured: true,
        username: username.map(|(u, _)| u),
        player_id,
    }
}

#[tauri::command]
pub fn zwift_sync_save(
    s: State<'_, crate::AppState>,
    username: String,
    password: String,
    verify: bool,
) -> Result<String, String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("用户名和密码不能为空".into());
    }
    let settings = s.settings.lock().unwrap().clone();
    let root = engine_root(&settings)
        .ok_or("未配置 zoffline 引擎,请先在环境检测页设置")?;
    let pdir = crate::strava::player_dir(&settings)
        .ok_or("未找到玩家档案目录(storage 下含 profile.bin 的目录)。请先把服务跑起来、进一次游戏生成档案,再配置同步。")?;
    let player_id = pdir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or("玩家目录名异常")?;

    if verify {
        verify_login(&username, &password)?;
    }
    let key = ensure_key(&root)?;
    let data = encrypt_credentials(&key, username.trim(), &password);
    let file = credentials_path(&root, &player_id);
    std::fs::write(&file, data).map_err(|e| format!("写入凭据失败: {e}"))?;
    Ok(format!(
        "已保存{}。之后每次骑行结束,zoffline 会自动把活动上传到官方 Zwift 服务器(需联网)。",
        if verify { "并验证通过" } else { "" }
    ))
}

#[tauri::command]
pub fn zwift_sync_clear(s: State<'_, crate::AppState>) -> Result<String, String> {
    let settings = s.settings.lock().unwrap().clone();
    let Some(root) = engine_root(&settings) else {
        return Err("未配置引擎".into());
    };
    let Some(pdir) = crate::strava::player_dir(&settings) else {
        return Err("未找到玩家档案目录".into());
    };
    let player_id = pdir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let file = credentials_path(&root, &player_id);
    if file.is_file() {
        std::fs::remove_file(&file).map_err(|e| format!("删除失败: {e}"))?;
        Ok("已停止同步官方 Zwift(凭据已删除)".into())
    } else {
        Ok("本来就没有配置过".into())
    }
}
