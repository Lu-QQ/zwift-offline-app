<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { api, type CheckItem, type Settings, type StepResult } from "../api";

const settings = ref<Settings>({ engine_path: null, engine_exe: null, zwift_dir: null, strava_port: 8000 });
const items = ref<CheckItem[]>([]);
const scanning = ref(false);
const configuring = ref(false);
const installing = ref(false);
const configSteps = ref<StepResult[]>([]);
const message = ref("");
const error = ref("");
let unlisten: (() => void) | null = null;

async function load() {
  settings.value = await api.getSettings();
}

async function scan() {
  scanning.value = true;
  error.value = "";
  try {
    await save();
    items.value = await api.detect();
  } catch (e) {
    error.value = String(e);
  } finally {
    scanning.value = false;
  }
}

async function save() {
  await api.saveSettings({ ...settings.value, strava_port: Number(settings.value.strava_port) || 8000 });
}

async function configure() {
  configuring.value = true;
  configSteps.value = [];
  error.value = "";
  message.value = "";
  try {
    await save();
    const results = await api.configApply();
    configSteps.value = results;
    if (results.some((r) => r.status === "error")) {
      error.value = "有步骤失败,请按提示处理(常见原因:未以管理员身份运行)";
    } else {
      message.value = "配置完成!可以到「服务控制」页启动 zoffline 了。";
    }
    items.value = await api.detect();
  } catch (e) {
    error.value = String(e);
  } finally {
    configuring.value = false;
  }
}

async function installDeps() {
  installing.value = true;
  error.value = "";
  message.value = "";
  configSteps.value = [];
  try {
    await installDepsInner();
  } finally {
    installing.value = false;
  }
}

async function installDepsInner() {
  return new Promise<void>((resolve) => {
    // install_deps 会持续推送 config:progress 事件,结束后 promise 完成
    api.installDeps().then(
      (msg) => {
        message.value = msg;
        items.value = [];
        api.detect().then((r) => (items.value = r));
        resolve();
      },
      (e) => {
        error.value = String(e);
        resolve();
      }
    );
  });
}

async function pickEnginePath() {
  const dir = await api.pickDir();
  if (dir) settings.value.engine_path = dir;
}
async function pickEngineExe() {
  const dir = await api.pickDir();
  if (dir) settings.value.engine_exe = dir ? dir + "/zoffline.exe" : null;
  // exe 选择器:目录方式简单一些,用户选 zoffline.exe 所在目录
}
async function pickZwiftDir() {
  const dir = await api.pickDir();
  if (dir) settings.value.zwift_dir = dir;
}

const installingPython = ref(false);
const pythonMissing = computed(() =>
  items.value.some((i) => i.id === "python" && i.status === "fail")
);

async function autoInstallPython() {
  installingPython.value = true;
  error.value = "";
  message.value = "";
  try {
    message.value = await api.installPython();
    items.value = await api.detect();
  } catch (e) {
    error.value = String(e);
  } finally {
    installingPython.value = false;
  }
}

function openPythonDownload(which: "official" | "mirror") {
  api.openUrl(
    which === "official"
      ? "https://www.python.org/downloads/windows/"
      : "https://mirrors.huaweicloud.com/python/3.12.9/python-3.12.9-amd64.exe"
  );
}

const badgeText: Record<string, string> = { ok: "✓", warn: "!", fail: "✕", info: "i" };

// 只展示有问题(warn/fail)的项,全绿时给一条横幅;想看细节可展开
const showAll = ref(false);
const problems = computed(() => items.value.filter((i) => i.status !== "ok"));
const okCount = computed(() => items.value.length - problems.value.length);

onMounted(async () => {
  unlisten = await api.onConfigProgress((r) => {
    if (r.status === "log") {
      const last = configSteps.value[configSteps.value.length - 1];
      if (last && last.status === "log") last.detail += "\n" + r.detail;
      else configSteps.value.push(r);
    } else {
      configSteps.value.push(r);
    }
  });
  await load();
  await scan();
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <h1>环境检测</h1>
  <p class="sub">第一次使用?先点「重新检测」看看缺什么,再点「一键配置 Zwift」。</p>

  <details class="card" style="padding: 0">
    <summary style="cursor: pointer; padding: 14px 18px; font-weight: 600; user-select: none">
      ⚙️ 路径设置<span style="color: var(--muted); font-weight: 400; font-size: 12.5px">(一般自动识别,识别不了再展开手动选)</span>
    </summary>
    <div style="padding: 0 18px 16px">
      <div class="form-row" style="margin-top: 8px">
        <label>zoffline 源码目录</label>
        <input class="path" v-model="settings.engine_path" placeholder="含 standalone.py 的目录,如 ..\zwift-offline" />
        <button class="btn ghost" @click="pickEnginePath">选择…</button>
      </div>
      <div class="form-row">
        <label>zoffline.exe 目录</label>
        <input class="path" v-model="settings.engine_exe" placeholder="可选,填了就优先用 exe(不需要 Python)" />
        <button class="btn ghost" @click="pickEngineExe">选择…</button>
      </div>
      <div class="form-row">
        <label>Zwift 安装目录</label>
        <input class="path" v-model="settings.zwift_dir" placeholder="留空自动检测(含 data 子目录)" />
        <button class="btn ghost" @click="pickZwiftDir">选择…</button>
      </div>
      <div class="form-row">
        <label>Strava 回调端口</label>
        <input class="path" v-model="settings.strava_port" style="width: 120px; flex: none" type="number" />
        <span style="color: var(--muted); font-size: 12px">默认 8000,Strava 应用回调域填 localhost</span>
      </div>
    </div>
  </details>

  <div class="card">
    <div style="display: flex; align-items: center; justify-content: space-between">
      <h3 style="margin: 0">🩺 检测结果</h3>
      <div style="display: flex; gap: 10px">
        <button class="btn ghost" :disabled="scanning" @click="scan">
          {{ scanning ? "检测中…" : "重新检测" }}
        </button>
        <button class="btn" :disabled="configuring" @click="configure">
          {{ configuring ? "配置中…" : "一键配置 Zwift" }}
        </button>
      </div>
    </div>
    <div v-if="items.length" style="margin-top: 12px">
      <div v-if="!problems.length" class="note" style="background: rgba(52,192,122,.1); border-left: 3px solid var(--ok); color: var(--ok)">
        ✅ 全部 {{ items.length }} 项检查通过,环境就绪!直接去「服务控制」页启动吧。
      </div>
      <template v-else>
        <p style="color: var(--muted); font-size: 12.5px; margin-bottom: 8px">
          {{ problems.length }} 项需要处理(其余 {{ okCount }} 项正常)
        </p>
        <div class="check-list">
          <div v-for="(item, i) in showAll ? items : problems" :key="i" class="check-item">
            <div class="badge" :class="item.status">{{ badgeText[item.status] || "?" }}</div>
            <div class="check-body">
              <div class="name">{{ item.name }}</div>
              <div class="detail">{{ item.detail }}</div>
              <div v-if="item.hint" class="hint">{{ item.hint }}</div>
            </div>
          </div>
        </div>
        <a v-if="okCount" style="color: var(--accent); font-size: 12.5px; cursor: pointer; display: inline-block; margin-top: 8px" @click="showAll = !showAll">
          {{ showAll ? "▲ 收起正常项" : `▼ 另有 ${okCount} 项正常,点开查看` }}
        </a>
      </template>
    </div>
    <p v-else style="color: var(--muted); margin-top: 10px">尚未检测</p>

    <div v-if="pythonMissing" class="note red" style="margin-top: 14px">
      <b>缺 Python 3</b> —— 三种解决办法(任选其一):
      <div class="actions" style="margin-top: 10px">
        <button class="btn" :disabled="installingPython" @click="autoInstallPython">
          {{ installingPython ? "下载安装中,请耐心等待…" : "⬇ 自动安装 Python(推荐)" }}
        </button>
        <button class="btn ghost" @click="openPythonDownload('official')">打开官方下载页</button>
        <button class="btn ghost" @click="openPythonDownload('mirror')">国内镜像直链(3.12.9)</button>
      </div>
      <p style="margin-top: 8px; color: var(--muted)">
        自动安装会从国内镜像下载(约 27MB)并静默装好、自动加 PATH,装完自动重新检测;
        手动安装时记得勾选「Add python.exe to PATH」。
        不想装 Python?上方填 zoffline.exe 目录即可,exe 模式完全不需要 Python。
      </p>
    </div>

    <div v-if="configSteps.length" style="margin-top: 14px">
      <div v-for="(s, i) in configSteps" :key="i" class="step" :class="s.status">
        <span class="tag">
          {{ s.status === "done" ? "已配置" : s.status === "skipped" ? "已满足" : s.status === "error" ? "失败" : "" }}
        </span>
        <span style="white-space: pre-wrap">{{ s.name + (s.detail ? ":" + s.detail : "") }}</span>
      </div>
    </div>

    <div class="actions">
      <button class="btn ghost" :disabled="installing" @click="installDeps">
        {{ installing ? "安装中,请耐心等待…" : "一键安装 Python 依赖" }}
      </button>
    </div>
  </div>

  <div v-if="message" class="note blue" style="margin-bottom: 12px">{{ message }}</div>
  <div v-if="error" class="note red" style="margin-bottom: 12px">{{ error }}</div>
</template>
