# 环境检测

逐项检测,汇总成中文清单(ok/warn/fail + 一句话说明)再向用户报告。

## 1. 管理员权限
- Windows: `net session` 退出码 0 = 有;否则提示"右键以管理员身份运行终端/本工具"
- macOS: `id -u` = 0

## 2. Zwift 客户端
- Windows: `C:\Program Files (x86)\Zwift\` 存在(或注册表 `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Zwift` 的 InstallLocation)
- macOS: `/Applications/Zwift.app`
- cacert 路径:Win=`<Zwift目录>\data\cacert.pem`;mac=`~/Library/Application Support/Zwift/data/cacert.pem`
- **未安装时(明确告知用户,不自动装)**:
  1. 告诉用户:"Zwift 游戏本体还没装,这一步没法替你装 —— 请到 https://www.zwift.com/download 下载安装(需要注册 Zwift 账号),装好后叫我继续"
  2. 引擎放置、Python、依赖安装**可以先做**;hosts/证书里的 cacert 追加这两步依赖 Zwift 目录,**等装好再做**
  3. 非标准安装位置时,请用户告知目录,后续 cacert 步骤用该目录

## 3. Python
- Windows 用 `py -3 --version`(**不要用 `python`**,那是商店占位符);macOS 用 `python3 --version`
- 依赖检查:`py -3 -c "import flask,gevent,google.protobuf,Crypto,jwt,dns.resolver,fitdecode,stravalib"` 报哪个模块缺装哪个(或整体 `pip install -r requirements.txt`)

## 4. 引擎(zwift-offline)
- 有效 = 目录里同时有 `standalone.py` 和 `zwift_offline.py`
- 候选:用户指定目录 > `~/.agents/skills/zoffline-setup/assets/zwift-offline`(本 skill 内置,默认就用它)> 常见位置(`~/zwift-offline`、`~/Documents/zwift-offline`、启动器安装目录旁)

## 5. 端口
- 80 / 443 / 3025(TCP)、3024(UDP)应空闲;被占时 `netstat -ano`+`tasklist` 查占用者告知用户
- 注意:zoffline 服务本身在跑时占用是**正常**状态

## 6. 系统根证书
- Windows: `certutil -store Root` 输出里找引擎 `ssl/cert-zwift-com.pem` 证书的 sha1(算法:对 pem 中 CERTIFICATE 段 base64 解码后的 DER 做 sha1,小写十六进制;**不要**用网上老的 54f7f293… 常量,已过时)
- macOS: `security find-certificate -c "*.zwift.com"`

## 7. hosts
- hosts 路径:Win=`C:\Windows\System32\drivers\etc\hosts`,mac=`/etc/hosts`
- 查是否含 `us-or-rly101.zwift.com`(配置过的标志)

## 8. cacert.pem 是否已追加
- 内容含 `MIIEQTCCAymgAwIBAgIUVPfyk0BzcKB2`(zoffline 证书段首行)即已配置
