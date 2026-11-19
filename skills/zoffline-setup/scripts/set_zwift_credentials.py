#!/usr/bin/env python3
"""写入 zoffline 引擎的官方 Zwift 同步凭据(zwift_credentials.bin)。

加密格式与 zwift_offline.py 的 encrypt_credentials 完全一致:
AES-256-CFB8(pycryptodome MODE_CFB 默认),文件 = IV(16B) + 密文,明文 = "user\\npass"。
--verify 时先临时挂起 hosts 里的 zwift 劫持行,调官方接口真实验证后再保存。

用法:
  python set_zwift_credentials.py --engine <引擎目录> --username a@b.c --password xxx [--verify]
"""
import argparse
import os
import subprocess
import sys
import tempfile
import time

try:
    from Crypto.Cipher import AES
    from Crypto.Random import get_random_bytes
except ImportError:
    sys.exit("缺 pycryptodome:先装引擎依赖 py -3 -m pip install -r requirements.txt")

SUSPEND_PREFIX = "# zl-suspended "


def hosts_path():
    return r"C:\Windows\System32\drivers\etc\hosts" if os.name == "nt" else "/etc/hosts"


def suspend_hosts():
    """给 hosts 里未注释的 zwift 劫持行加前缀;返回是否有改动。"""
    p = hosts_path()
    with open(p, "r", encoding="utf-8", errors="surrogateescape") as f:
        content = f.read()
    if "secure.zwift.com" not in content:
        return False
    out = []
    changed = False
    for chunk in content.splitlines(keepends=True):
        line = chunk.rstrip("\r\n")
        eol = chunk[len(line):]
        if not line.lstrip().startswith("#") and "secure.zwift.com" in line:
            out.append(SUSPEND_PREFIX + line + eol)
            changed = True
        else:
            out.append(chunk)
    if changed:
        with open(p, "w", encoding="utf-8", errors="surrogateescape", newline="") as f:
            f.write("".join(out))
    return changed


def restore_hosts():
    p = hosts_path()
    with open(p, "r", encoding="utf-8", errors="surrogateescape") as f:
        content = f.read()
    if SUSPEND_PREFIX not in content:
        return
    out = []
    for chunk in content.splitlines(keepends=True):
        line = chunk.rstrip("\r\n")
        eol = chunk[len(line):]
        if line.startswith(SUSPEND_PREFIX):
            out.append(line[len(SUSPEND_PREFIX):] + eol)
        else:
            out.append(chunk)
    with open(p, "w", encoding="utf-8", errors="surrogateescape", newline="") as f:
        f.write("".join(out))


def verify_login(username, password):
    """调官方 Keycloak 验证(临时绕开 hosts 劫持)。成功 True,失败抛 SystemExit。"""
    import urllib.request
    import urllib.parse
    suspended = suspend_hosts()
    try:
        data = urllib.parse.urlencode({
            "client_id": "Zwift_Mobile_Link",
            "grant_type": "password",
            "username": username,
            "password": password,
        }).encode()
        req = urllib.request.Request(
            "https://secure.zwift.com/auth/realms/zwift/tokens/access/codes", data=data)
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return r.status == 200
        except urllib.error.HTTPError as e:
            print(f"官方返回 {e.code}(账号或密码错误?)", file=sys.stderr)
            return False
        except Exception as e:
            print(f"无法连接官方服务器(需联网): {e}", file=sys.stderr)
            return False
    finally:
        if suspended:
            restore_hosts()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", required=True, help="zwift-offline 引擎目录")
    ap.add_argument("--username", required=True)
    ap.add_argument("--password", required=True)
    ap.add_argument("--verify", action="store_true", help="保存前先官方验证")
    a = ap.parse_args()

    storage = os.path.join(a.engine, "storage")
    if not os.path.isfile(os.path.join(storage, "credentials-key.bin")):
        os.makedirs(storage, exist_ok=True)
        with open(os.path.join(storage, "credentials-key.bin"), "wb") as f:
            f.write(get_random_bytes(32))
        print("已生成 credentials-key.bin(32 字节随机)")
    with open(os.path.join(storage, "credentials-key.bin"), "rb") as f:
        key = f.read()
    if len(key) != 32:
        sys.exit(f"credentials-key.bin 长度异常: {len(key)}(应 32)")

    if a.verify:
        # DNS 缓存可能残留,稍等劫持切换生效
        time.sleep(1)
        if not verify_login(a.username, a.password):
            sys.exit("官方验证未通过,未保存(可用 --verify 去掉强制写入,不建议)")

    # 找玩家目录(storage 下含 profile.bin 的数字目录)
    player = None
    for name in os.listdir(storage):
        d = os.path.join(storage, name)
        if name.isdigit() and os.path.isfile(os.path.join(d, "profile.bin")):
            player = player or d
    if not player:
        sys.exit("未找到玩家档案(storage 下无含 profile.bin 的数字目录):先跑一次服务进游戏生成")

    cipher = AES.new(key, AES.MODE_CFB)  # 默认即 CFB8,与引擎一致
    data = cipher.iv + cipher.encrypt((a.username + "\n" + a.password).encode("utf-8"))
    out = os.path.join(player, "zwift_credentials.bin")
    with open(out, "wb") as f:
        f.write(data)
    print(f"已写入 {out}")
    print("之后每次骑行结束,引擎会自动把活动上传到官方 Zwift(需联网)。")


if __name__ == "__main__":
    main()
