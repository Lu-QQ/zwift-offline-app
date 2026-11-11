# zoffline 启动器

给 [zoffline](https://github.com/zoffline/zwift-offline)(Zwift 离线服务器)做的中文小白向导桌面应用,基于 **Tauri 2 + Rust + Vue 3**。

> **下载**:到 [Releases](../../releases) 页拿最新的 `zoffline-launcher_x64-setup.exe`(约 2MB),右键"以管理员身份运行"安装。
>
> **授权**:本项目继承上游 zoffline 的 **AGPL-3.0**(不限制商用/非商用,要求衍生作品同样开源);Zwift、Strava、zoffline 的商标与代码归各自所有,本项目仅供个人学习研究。

把 zoffline 最劝退的四件事变成点按钮:

| 以前要手动做 | 现在 |
|---|---|
| 手动改 hosts、导入证书、改 Zwift 的 cacert.pem | 「环境检测」页 → **一键配置 Zwift** |
| 命令行跑 `standalone.py`(还要管理员) | 「服务控制」页 → **▶ 启动服务**,日志实时看 |
| 跑 `strava_auth.py` 再手动挪 token 文件 | 「Strava 授权」页 → 浏览器里点一下授权,token 自动就位 |
| 想回官方服务器要逐项手工撤销 | 「还原官方」页 → **一键还原**(全部可逆,有备份) |

## 功能

- **环境检测**:Zwift 安装(含注册表探测/手动选目录)、Python 3、zoffline 引擎(源码目录或 zoffline.exe)、Python 依赖缺失、80/443/3024/3025 端口占用(能区分"被自己服务占用"和"被别人占用")、系统根证书、hosts 状态、管理员权限 —— 每项缺什么都有中文提示
- **缺 Python 一键解决**:检测到没装 Python 时,提供「自动安装」(从华为云镜像下载 3.12.9 → 静默安装并加 PATH → 自动复检)、官方下载页、国内镜像直链三个入口;Python **不打进安装包**(体积会从 2MB 涨到 28MB+,且 pip 依赖本就需要联网);不想装 Python 可直接用 zoffline.exe 模式
- **一键配置客户端**:写 hosts(先备份)→ 证书导入系统信任区(Windows `certutil` / macOS `security`)→ 追加证书到 Zwift 的 `cacert.pem`(先备份),幂等可重复执行
- **服务控制**:拉起/停止 `standalone.py` 或 `zoffline.exe`,实时日志,443 健康检查,顺手启动 Zwift 客户端
- **云同步**:
  - **官方 Zwift**:填官方账号密码 → (可选)先登录官方验证 → 写入引擎同款加密的 `zwift_credentials.bin`(AES-256-CFB8,与 pycryptodome 逐字节兼容,有跨端单元测试),之后每次骑行结束引擎自动上传到官方服务器;一键停止同步
  - **Strava**:内置 OAuth 回调服务器 + 自动换取/写入 `storage/<玩家>/strava_token.txt`,token 过期由 zoffline 服务端自动刷新(2026-06 起需 Strava 付费订阅)
- **一键还原**:撤销上述全部修改,骑行存档(`storage/`)不受影响
- **一键装依赖**:源码模式下自动执行 `pip install -r requirements.txt`

## 从源码跑(开发)

前置:Node.js 18+、Rust(msvc 工具链)、Windows 需要 WebView2(Win10/11 一般自带)。

```bash
npm install
npm run tauri dev     # 开发模式(不提权,配置操作会提示需要管理员)
npm run tauri build   # 打包:Windows 出 NSIS 安装包,macOS 出 dmg
```

## 发新版(维护者指南)

1. 改版本号三处:`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`,并在 `CHANGELOG.md` 顶部加一段
2. 带签名构建(更新包必须 minisign 签名,私钥在 `~/.tauri/zoffline-launcher.key`,**务必备份**):
   ```bash
   export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/zoffline-launcher.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
   npm run tauri build
   ```
3. 生成 `latest.json`:`version`、`notes`、`platforms.windows-x86_64.signature`(取 `.exe.sig` 内容)、`url`(指向新 release 的 exe,建议用 ghfast 加速前缀)
4. 推送 main + tag,发 GitHub Release,上传三个资产:**exe、exe.sig、latest.json**
5. 验证:`curl -sL https://gh-proxy.com/https://github.com/Lu-QQ/zwift-offline-app/releases/latest/download/latest.json` 应返回新版本号

> 国内用户更新走 ghfast/gh-proxy 镜像(编译期固化在 tauri.conf.json 的 endpoints,换源需发新版)。

## 卸载说明

卸载启动器**不会**自动还原系统修改(hosts 重定向、根证书、Zwift cacert.pem 备份仍在)。卸载前请先在「↩️ 还原官方」页执行一键还原。

## 使用说明(小白版)

1. **安装**:Windows 下载安装包,右键"以管理员身份运行"(安装包本身也会请求管理员);macOS 拖进应用程序
2. **设置路径**:首次打开会自动探测;识别不到就手动点"选择…"指定 zoffline 源码目录(或 zoffline.exe 所在目录)和 Zwift 目录
3. **环境检测页**:点「重新检测」,红的黄的一项项看提示;都绿了点「一键配置 Zwift」
4. **服务控制页**:点「▶ 启动服务」,等日志滚起来,再点「启动 Zwift」进游戏
5. 想同步 Strava:「Strava 授权」页照着三步走(注意:**2026-06 起 Strava API 需要付费订阅**)
6. 想回官方:「还原官方」页,勾确认后一键还原

## 技术说明

- Windows 打包产物通过 [app.manifest](src-tauri/app.manifest) 声明 `requireAdministrator`(仅 release,debug 开发免 UAC):改 hosts / 导证书 / 绑 80/443 都需要管理员
- macOS 端启动 `standalone.py` 需要 sudo(80/443 特权端口),当前版本请在「服务控制」失败时改用终端 `sudo python3 standalone.py`,或使用 443 代理方案(计划中)
- 引擎检测顺序:`zoffline.exe`(填了优先)→ 源码目录(`standalone.py` + `zwift_offline.py` 都在才算有效)
- 上游 zoffline 已声明弃维,活跃维护在 [fatsbrown/zwift-offline](https://github.com/fatsbrown/zwift-offline),本启动器对两者通用
- 所有系统修改都有备份(`*.zoffline-launcher.bak`),还原页可完整撤销

## 目录结构

```
zoffline-launcher/
├── src-tauri/src/
│   ├── lib.rs           # 入口、IPC 注册
│   ├── detector.rs      # 环境检测
│   ├── configurator.rs  # 证书/hosts/cacert 配置与还原
│   ├── service.rs       # 进程管理、日志流、健康检查
│   ├── strava.rs        # 内嵌 OAuth 回调 + token 写入
│   ├── settings.rs      # 路径设置持久化
│   └── util.rs          # 系统探测工具函数
└── src/                 # Vue 3 前端(四个页面)
```
