# 云同步(官方 Zwift / Strava)

活动上传由引擎在**每次骑行保存时自动执行**,你只需要把凭据文件放对。

## 前提

- 玩家档案已存在:引擎 `storage/` 下有含 `profile.bin` 的数字目录(先进一次游戏生成)
- 访问官方必须联网,且**先临时注释 hosts 里的 zwift 行**(见 `references/service.md`),否则连的是本地假服务器

## 同步官方 Zwift(zwift_credentials.bin)

- 格式:AES-256-**CFB8** 密文(= pycryptodome `AES.new(key, AES.MODE_CFB, iv)` 默认参数),
  文件 = IV(16字节) + 密文,明文 = `用户名\n密码`(UTF-8)
- key = 引擎 `storage/credentials-key.bin`(32 字节;不存在就生成随机 32 字节写入,引擎会直接复用)
- **用现成脚本,别手写加密**。将 Skill 目录下的 `scripts/set_zwift_credentials.py` 解析为绝对路径，填入命令中的脚本路径。引擎目录仍填写实际运行目录。
  ```
  py -3 "<脚本绝对路径>" --engine "<引擎运行目录>" --username <邮箱> --password <密码> [--verify]
  ```
  `--verify` 会临时挂起 hosts 后调官方接口真实验证(端点:
  `https://secure.zwift.com/auth/realms/zwift/tokens/access/codes`,
  form: client_id=Zwift_Mobile_Link, grant_type=password)。验证通过才保存。
- 想让用户自己改密码/看官方页面:浏览器打开 `https://www.zwift.com/sign-in`(`/login` 会 404)

## 同步 Strava(strava_token.txt)

- 前提:用户在 strava.com/settings/api 自建应用(callback domain 填 `localhost`),
  **2026-06 起 Strava API 需付费订阅**,没有就直接告知用户,别白折腾
- 文件:`storage/<玩家>/strava_token.txt`,五行:
  ```
  client_id
  client_secret
  access_token
  refresh_token
  expires_at(unix 秒)
  ```
- 获取 token:引擎自带 `scripts/strava_auth.py --client-id <id> --client-secret <secret>`,
  跑起来后浏览器开 `http://localhost:8000/` 授权,生成的 strava_token.txt 挪到玩家目录
- token 过期不用管:引擎上传时自动刷新

## 取消同步

删对应凭据文件即可(zwift_credentials.bin / strava_token.txt),已上传的数据不受影响。
