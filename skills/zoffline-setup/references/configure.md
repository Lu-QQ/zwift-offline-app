# 安装与配置(含引擎放置、Python、hosts/证书)

## 引擎放置(默认离线,无需 GitHub)

1. 首选用户指定目录;否则把**本 skill 内置引擎**复制到用户认可的固定位置:
   - Windows 建议 `C:\Users\<用户>\AppData\Local\zwift-offline` 或 `D:\zwift-offline`
   - macOS 建议 `~/zwift-offline`
   - 复制源:Skill 目录下的 `assets/zwift-offline/`。复制到上述运行目录后再启动，引擎在运行目录下使用 `storage/`。
2. 若用户要更新引擎:下载 `https://ghfast.top/https://github.com/fatsbrown/zwift-offline/archive/refs/heads/master.zip`(失败换 `https://gh-proxy.com/...` 前缀,再失败 GitHub 直连),解压后**保留旧目录的 storage/** 只换代码。
   fatsbrown fork 是活跃维护版;上游 zoffline/zoffline 已弃维。

## Python(缺才装)

- Windows 静默安装:下载 `https://mirrors.huaweicloud.com/python/3.12.9/python-3.12.9-amd64.exe`(约27MB)→ 管理员运行 `python-3.12.9-amd64.exe /quiet InstallAllUsers=1 PrependPath=1 Include_launcher=1` → 复检 `py -3 --version`
- 依赖:`py -3 -m pip install -r <引擎>/requirements.txt -i https://pypi.tuna.tsinghua.edu.cn/simple`

## 客户端配置三件套(全部幂等,先备份)

hosts 行内容:
```
127.0.0.1 us-or-rly101.zwift.com secure.zwift.com cdn.zwift.com launcher.zwift.com
```

1. **hosts**:备份为 `hosts.zoffline.bak`(仅首次);已有 zwift 行则跳过。写入时**保留原有 CRLF 行尾**(用逐行 split_inclusive 处理,别整体 join("\n"))
2. **根证书**:
   - Win:`certutil -importpfx Root <引擎>\ssl\cert-zwift-com.p12`(先按 `references/detect.md` 算指纹查重)
   - mac:`security import ssl/cert-zwift-com.pem -k /Library/Keychains/System.keychain && security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ssl/cert-zwift-com.pem`(需 sudo)
3. **Zwift cacert.pem 追加**:备份 `cacert.pem.zoffline.bak`;文件已含标记 `MIIEQTCCAymgAwIBAgIUVPfyk0BzcKB2` 则跳过;否则把引擎 `ssl/cert-zwift-com.pem` 全文追加到末尾
4. 配完 Windows 上 `taskkill /F /IM ZwiftLauncher.exe`(让 Zwift 重读 cacert)

## 还原官方(全部可逆)

1. hosts:删除我们加的那一行(按备份恢复亦可)
2. 根证书:Win `certutil -delstore Root <指纹>`;mac `security delete-certificate -c "*.zwift.com"`
3. cacert:从 `cacert.pem.zoffline.bak` 恢复;无备份且有标记则按标记截掉追加段
4. 引擎目录与 storage 数据**不动**
