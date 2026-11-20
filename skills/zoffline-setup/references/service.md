# 服务启停与健康验证

## 启动

- 命令(Windows):管理员下 `py -3 standalone.py`,**工作目录 = 引擎目录**
- macOS:`sudo python3 standalone.py`(80/443 是特权端口)
- 启动后等几秒,日志应出现 `Server version ... is running`

## 验证(重要:探 80 不探 443)

- ✅ 正确:TCP 连 `127.0.0.1:80` 成功 = 服务健康(80 是纯 HTTP CDN,同进程)
- ❌ 错误:裸 TCP 连 443 后立刻断开 —— gevent 会对每个连入做 SSL 包装,立刻断开会抛
  `ssl.SSLEOFError: UNEXPECTED_EOF_WHILE_READING` 刷进服务日志吓用户
- 更完整:HTTP 客户端访问 `https://127.0.0.1/launcher`(忽略证书)应得 302

## 停止

- Windows:`taskkill /T /F /PID <pid>`(连子进程树)
- 找 pid:python 进程 + 命令行含 standalone.py

## 常见故障

| 现象 | 原因/处理 |
|---|---|
| 端口 bind 失败 | 80/443 被 IIS/其他 Web 服务占用(按 `references/detect.md` 查占用者) |
| Zwift 连不上离线服 | hosts 未生效/证书三件套不全 → 按 `references/configure.md` 检查 |
| 日志刷 SSLEOFError | 是健康探针连了 443 就断造成的,改探 80 即消;服务本身没事 |
| 要访问官方(验证账号/登录页) | 先临时注释 hosts 里 zwift 行(前缀 `# zl-suspended `),完事去掉,否则连到本地假服务器 |
