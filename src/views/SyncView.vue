<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { api, type StravaStatus, type ZwiftSyncStatus } from "../api";

// ---------- Strava ----------
const strava = ref<StravaStatus | null>(null);
const clientId = ref("");
const clientSecret = ref("");
const stravaBusy = ref(false);
// ---------- Zwift 官方 ----------
const zwift = ref<ZwiftSyncStatus | null>(null);
const zwiftUser = ref("");
const zwiftPass = ref("");
const zwiftVerify = ref(true);
const zwiftBusy = ref(false);
const loginWindowBusy = ref(false);

const message = ref("");
const error = ref("");

// 官方登录窗口捕获到的凭据(提交成功才采用)
let latestCred = { u: "", p: "" };
let uncred: (() => void) | null = null;
let unok: (() => void) | null = null;
let unfail: (() => void) | null = null;

async function refresh() {
  try {
    strava.value = await api.stravaStatus();
  } catch { /* ignore */ }
  try {
    zwift.value = await api.zwiftSyncStatus();
  } catch { /* ignore */ }
}

async function openOfficialLogin() {
  error.value = "";
  message.value = "正在打开 Zwift 官方登录窗口…请直接在官方页面里登录,成功后会自动保存。";
  loginWindowBusy.value = true;
  try {
    await api.zwiftLoginWindow();
  } catch (e) {
    error.value = String(e) + "(打开官方页面失败一般是因为 hosts 劫持需要管理员权限来临时绕开)";
  } finally {
    loginWindowBusy.value = false;
  }
}

async function authorize() {
  stravaBusy.value = true;
  error.value = "";
  message.value = "";
  try {
    message.value = await api.stravaAuthorize(clientId.value.trim(), clientSecret.value.trim());
  } catch (e) {
    error.value = String(e);
  } finally {
    stravaBusy.value = false;
    await refresh();
  }
}

async function saveZwift() {
  zwiftBusy.value = true;
  error.value = "";
  message.value = "";
  try {
    message.value = await api.zwiftSyncSave(zwiftUser.value.trim(), zwiftPass.value, zwiftVerify.value);
    zwiftPass.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    zwiftBusy.value = false;
    await refresh();
  }
}

async function clearZwift() {
  error.value = "";
  message.value = "";
  try {
    if (confirm("确定停止同步官方 Zwift 吗?(只是删除本地凭据,不影响已上传的数据)")) {
      message.value = await api.zwiftSyncClear();
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    await refresh();
  }
}

function fmtExpires(ts: number | null) {
  if (!ts) return "未知";
  return new Date(ts * 1000).toLocaleString("zh-CN");
}

onMounted(async () => {
  await refresh();
  // 登录窗口输入时持续上报,记录最新值
  uncred = await api.onZwiftCred((c) => (latestCred = c));
  // 官方页面登录成功(页面跳转)→ 自动保存(无需再验证,官方刚验证过)
  unok = await api.onZwiftLoginSuccess(async (c) => {
    const u = c.u || latestCred.u;
    const p = c.p || latestCred.p;
    if (!u || !p) {
      message.value = "检测到登录成功,但没能读取到账号密码(可能用了 Google/Apple 等第三方登录)。请展开下方手动填写。";
      await api.zwiftLoginCleanup();
      return;
    }
    zwiftBusy.value = true;
    message.value = "✅ 官方登录成功,正在保存并开启同步…";
    try {
      message.value = await api.zwiftSyncSave(u, p, false);
      await api.zwiftLoginCleanup();
    } catch (e) {
      error.value = String(e);
    } finally {
      zwiftBusy.value = false;
      await refresh();
    }
  });
  unfail = await api.onZwiftLoginFailed(() => {
    error.value = "官方页面提示登录失败(账号或密码有误),请在登录窗口里重试。";
  });
});
onUnmounted(() => {
  uncred?.();
  unok?.();
  unfail?.();
});
</script>

<template>
  <h1>云同步</h1>
  <p class="sub">骑行数据自动上云:官方 Zwift 和/或 Strava,可以同时开。</p>

  <!-- ===== Zwift 官方同步 ===== -->
  <div class="card">
    <h3>🚴 同步到官方 Zwift 服务器</h3>
    <div v-if="zwift">
      <div v-if="zwift.configured" style="line-height: 2; margin-bottom: 8px">
        <span class="pill ok"><i />已开启自动同步</span>
        <div style="color: var(--muted)">账号:{{ zwift.username }}(玩家档案 storage/{{ zwift.player_id }})</div>
        <div class="actions" style="margin-top: 6px">
          <button class="btn danger" @click="clearZwift">停止同步(删除本地凭据)</button>
        </div>
      </div>
      <div v-else>
        <p style="color: var(--muted); line-height: 1.9">
          开启后,每次骑行结束会自动把活动上传到官方 Zwift 服务器 —— 离线骑的里程也能计入官方账号、涨经验。
        </p>
        <div class="actions" style="margin-top: 4px">
          <button class="btn" :disabled="loginWindowBusy || zwiftBusy" @click="openOfficialLogin">
            {{ loginWindowBusy ? "正在打开…" : "🔑 打开 Zwift 官方登录窗口(推荐)" }}
          </button>
        </div>
        <p class="note blue" style="margin-top: 10px">
          弹出的窗口就是 <b>Zwift 官方登录页</b>(secure.zwift.com)。你在官方页面登录,启动器在登录成功后自动记住账号并开启同步,
          全程不用手动填表单。提示:打开窗口期间会暂时绕开 hosts 劫持,关闭窗口后自动恢复;使用 Google/Apple 第三方登录的账号请用下方手动填写。
        </p>

        <details style="margin-top: 12px">
          <summary style="cursor: pointer; color: var(--muted); font-size: 12.5px; user-select: none">
            手动填写账号密码(兜底方式,点开)
          </summary>
          <div style="margin-top: 10px">
            <div class="form-row">
              <label>账号(邮箱)</label>
              <input class="path" v-model="zwiftUser" placeholder="你在官方 Zwift 的登录邮箱" />
            </div>
            <div class="form-row">
              <label>密码</label>
              <input class="path" type="password" v-model="zwiftPass" placeholder="官方 Zwift 密码" />
            </div>
            <label style="display: flex; gap: 8px; align-items: center; color: var(--muted); cursor: pointer; margin: 6px 0 4px 140px">
              <input type="checkbox" v-model="zwiftVerify" />
              保存前先登录官方服务器验证账号(推荐,需要联网)
            </label>
            <div class="actions">
              <button class="btn" :disabled="zwiftBusy" @click="saveZwift">
                {{ zwiftBusy ? (zwiftVerify ? "验证并保存中…" : "保存中…") : "保存并开启同步" }}
              </button>
            </div>
          </div>
        </details>

        <p v-if="!zwift.player_id" class="note red" style="margin-top: 10px">
          还没找到玩家档案(storage 下没有含 profile.bin 的目录):先把服务跑起来、进一次游戏,再回来配置。
        </p>
      </div>
    </div>
    <p v-else style="color: var(--muted)">读取中…</p>
  </div>

  <!-- ===== Strava ===== -->
  <div class="card">
    <h3>⭐ 同步到 Strava</h3>
    <div class="note red" style="margin-bottom: 12px">
      ⚠️ 2026 年 6 月起,<b>Strava 公共 API 需要付费订阅(Subscription)</b>才能使用。没有订阅的话授权会失败,这是 Strava 官方限制。
    </div>

    <p style="color: var(--muted); line-height: 1.9; margin-bottom: 10px">
      ① 准备:打开 <a href="https://www.strava.com/settings/api" target="_blank" style="color: var(--accent)">strava.com/settings/api</a>
      创建应用(只需一次),「Authorization Callback Domain」填 <b style="color: var(--text)">localhost</b>,记下 Client ID 和 Client Secret;
      ② 填到下面点授权,浏览器里确认后自动完成。
    </p>

    <div class="form-row">
      <label>Client ID</label>
      <input class="path" v-model="clientId" placeholder="例如 123456" />
    </div>
    <div class="form-row">
      <label>Client Secret</label>
      <input class="path" v-model="clientSecret" placeholder="一长串字母数字" />
    </div>
    <div class="actions">
      <button class="btn" :disabled="stravaBusy" @click="authorize">
        {{ stravaBusy ? "等待授权回调…(请在浏览器里完成)" : "🔗 开始授权" }}
      </button>
    </div>

    <div v-if="strava" style="margin-top: 12px">
      <div v-if="strava.configured" style="line-height: 2">
        <span class="pill ok"><i />已授权</span>
        <div style="color: var(--muted)">令牌文件:{{ strava.token_file }}</div>
        <div style="color: var(--muted)">
          过期时间:{{ fmtExpires(strava.expires_at) }}{{ strava.expired ? "(已过期,上传时会自动刷新)" : "" }}
        </div>
      </div>
      <div v-else style="color: var(--muted); line-height: 2">
        未授权。
        <span v-if="!strava.player_id">
          <br />⚠️ 还没找到玩家档案:先跑一次服务、进游戏存档后再来授权。
        </span>
      </div>
    </div>
  </div>

  <div v-if="message" class="note blue" style="margin-bottom: 12px">{{ message }}</div>
  <div v-if="error" class="note red" style="margin-bottom: 12px">{{ error }}</div>
</template>
