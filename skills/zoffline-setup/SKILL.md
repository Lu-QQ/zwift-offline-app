---
name: zoffline-setup
description: 配置、启动、修复或还原 zoffline(Zwift 离线服务器)。当用户提到 Zwift 离线、zoffline、zwift-offline、离线骑 Zwift、把 Zwift 客户端指向本地/离线服务器、Zwift 打不开/连不上离线服、还原官方 Zwift、Zwift 或 Strava 骑行数据自动同步上传时使用。覆盖 Windows 与 macOS:环境检测、引擎/Python 安装、hosts 与证书配置、服务启停、云同步凭据。
slug: zoffline-setup
displayName: zoffline 一键配置(Zwift 离线)
version: 1.0.0
summary: 让 AI Agent 替你把 Zwift 切到离线服务器:内置引擎离线开箱即用,自动装 Python、配 hosts/证书、启停服务、官方 Zwift 与 Strava 云同步凭据,全程可一键还原。沉淀了全部实战坑位(探活端口、证书指纹、CFB8 加密等)。
license: AGPL-3.0
---

# zoffline 一键配置

zoffline 让 Zwift 客户端连本地服务器离线骑行。本技能引导你(agent)替用户完成全部配置。**所有步骤都已踩过坑,按 references 执行,不要自创命令。**

## 引擎来源(离线优先,不依赖 GitHub)

- **默认**:`assets/zwift-offline/`(本 skill 自带的源码快照,开箱即用,用户无需联网下载)
- **更新**:仅当用户主动要新版本时,从镜像拉取 fatsbrown 维护版(见 references/configure.md"引擎放置")
- 其余需要联网的只有:Python 安装(华为云镜像)、pip 依赖(清华镜像)—— 都不走 GitHub

## 工作流

1. **确认意图**:安装配置 / 修故障 / 启停服务 / 还原官方 / 云同步。用户没说清就先问。
2. **环境检测**:按 `references/detect.md` 逐项体检(Zwift、Python、引擎、端口、证书、hosts、管理员权限),把结果汇总成中文清单告诉用户。
3. **征得同意**:列出接下来要装什么/改什么(Python、hosts、证书、引擎目录),**用户点头才动手**;涉及账号密码的只在用户主动提供后使用。
4. **执行**(按需选读):
   - 安装/配置 → `references/configure.md`
   - 服务启停与验证 → `references/service.md`
   - 数据同步官方 Zwift / Strava → `references/cloud-sync.md`
5. **验证**:每步做完立即验证(检测命令复跑 / 端口探活),全部通过再向用户报告。

## 关键坑速查(违反必翻车)

| 坑 | 正确做法 |
|---|---|
| Windows `python` 是商店占位符 | 一律 `py -3`;没有就先装(见 configure.md) |
| 健康探活裸连 443 | **探 80 端口**(纯 HTTP);443 裸连会让 gevent 抛 SSLEOFError 刷屏吓用户 |
| 验证官方账号/打开官方页 | 先临时注释 hosts 里的 zwift 行(加 `# zl-suspended ` 前缀),完事去掉;否则连到本地假服务器,假验证通过 |
| Zwift 官方网页登录入口 | `https://www.zwift.com/sign-in`;`/login` 会按 IP 地域跳转后 404 |
| 证书指纹 | 动态算 `sha1(pem 证书 DER)`,网上老教程的 54f7f293… 已过时 |
| hosts/cacert 修改 | 先备份;保留 CRLF 行尾;幂等(已配置则跳过) |
| 凭据加密 | AES-256-**CFB8**(pycryptodome `MODE_CFB` 默认),文件 = IV(16B)+密文,明文 `user\npass`,见 scripts/set_zwift_credentials.py |
| 下载 GitHub | 国内直连常断,前缀 `https://ghfast.top/` 或 `https://gh-proxy.com/`;pip 用 `-i https://pypi.tuna.tsinghua.edu.cn/simple` |

## 权限与安全

- 改 hosts / 导证书 / 绑 80/443 需要**管理员**(Windows)或 sudo(macOS):检测到没权限就明确告诉用户怎么以管理员运行,别硬试。
- 用户密码只在写入加密凭据时短暂使用,不要 echo、不要写进日志/临时文件。
- 一切系统修改都可逆:备份文件命名 `*.zoffline-launcher.bak`,还原 = 恢复备份。

## 骑行数据(不可触碰)

引擎目录下 `storage/`(玩家档案/活动/凭据)是用户核心数据:**任何操作不得删除**;升级引擎 = 保留 storage 只换代码。
