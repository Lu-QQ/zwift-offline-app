<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { api, type CheckItem, type StepResult } from "../api";

const emit = defineEmits<{ finish: [] }>();

const step = ref(0); // 0 欢迎 1 检测 2 配置 3 启动 4 完成
const STEP_NAMES = ["欢迎", "检测环境", "配置 Zwift", "启动服务", "完成"];

// 检测
const items = ref<CheckItem[]>([]);
const scanning = ref(false);
// 配置
const configSteps = ref<StepResult[]>([]);
const configuring = ref(false);
const configRan = ref(false);
// 服务
const status = ref({ running: false, pid: null as number | null, healthy: false });
const starting = ref(false);
let statusTimer: number | undefined;

const detectHasFail = computed(() => items.value.some((i) => i.status === "fail"));
const problems = computed(() => items.value.filter((i) => i.status !== "ok"));
const configHasError = computed(() => configSteps.value.some((s) => s.status === "error"));

async function scan() {
  scanning.value = true;
  try {
    items.value = await api.detect();
  } finally {
    scanning.value = false;
  }
}

async function configure() {
  configuring.value = true;
  configSteps.value = [];
  try {
    configSteps.value = await api.configApply();
    configRan.value = true;
  } finally {
    configuring.value = false;
  }
}

async function startService() {
  starting.value = true;
  try {
    await api.serviceStart();
  } catch {
    /* 日志页可看详情 */
  } finally {
    starting.value = false;
  }
}

function next() {
  if (step.value < 4) step.value += 1;
}
function prev() {
  if (step.value > 0) step.value -= 1;
}
function finish() {
  localStorage.setItem("wizard_done", "1");
  emit("finish");
}
function skip() {
  localStorage.setItem("wizard_done", "1");
  emit("finish");
}

async function refreshStatus() {
  try {
    status.value = await api.serviceStatus();
  } catch { /* ignore */ }
}

const onKeydown = (e: KeyboardEvent) => {
  if (e.key === "Escape") skip();
};

onMounted(async () => {
  await scan();
  await refreshStatus();
  statusTimer = window.setInterval(refreshStatus, 2000);
  window.addEventListener("keydown", onKeydown);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
  if (statusTimer) window.clearInterval(statusTimer);
});
</script>

<template>
  <div class="wizard">
    <!-- 顶部步骤条 -->
    <div class="wiz-header">
      <div class="wiz-title">zoffline 启动器 · 首次引导</div>
      <div class="wiz-steps">
        <div v-for="(name, i) in STEP_NAMES" :key="i" class="wiz-step" :class="{ active: i === step, done: i < step }">
          <span class="wiz-step-dot">{{ i < step ? "✓" : i + 1 }}</span>
          <span class="wiz-step-name">{{ name }}</span>
        </div>
      </div>
      <a class="wiz-skip" @click="skip">跳过引导,直接使用 →</a>
    </div>

    <div class="wiz-body">
      <!-- 0 欢迎 -->
      <div v-if="step === 0" class="wiz-card">
        <h2>👋 欢迎使用 zoffline</h2>
        <p class="wiz-lead">
          这个小工具帮你把 Zwift 骑行切换到离线服务器(或局域网自建服务器),<br />
          不用敲命令行、不用手改系统文件,跟着下面几步点按钮就行。
        </p>
        <div class="wiz-features">
          <div class="wiz-feature"><b>🩺 自动体检</b><span>检测 Zwift / Python / 端口 / 证书,缺什么告诉你</span></div>
          <div class="wiz-feature"><b>🔧 一键配置</b><span>自动改 hosts、导证书,配置前有备份、随时可还原</span></div>
          <div class="wiz-feature"><b>⭐ 自动同步</b><span>可选:骑行结束自动上传 Strava</span></div>
        </div>
        <div class="note blue">
          前提:本机已安装 <b>Zwift</b>,并且有 zoffline 引擎(源码目录或 zoffline.exe)。<br />
          引导过程中遇到问题,随时可以跳过,到主界面的「环境检测」页慢慢排查。
        </div>
        <div class="wiz-actions">
          <button class="btn" @click="next">开始引导(约 2 分钟)</button>
          <button class="btn ghost" @click="skip">跳过</button>
        </div>
      </div>

      <!-- 1 检测 -->
      <div v-else-if="step === 1" class="wiz-card">
        <h2>🩺 第一步:检查环境</h2>
        <p class="wiz-lead">已经在自动检测了:</p>
        <div v-if="items.length">
          <div v-if="!problems.length" class="note" style="background: rgba(52,192,122,.1); border-left: 3px solid var(--ok); color: var(--ok)">
            ✅ 全部 {{ items.length }} 项检查通过,环境完美!直接下一步。
          </div>
          <template v-else>
            <div class="check-list">
              <div v-for="(item, i) in problems" :key="i" class="check-item">
                <div class="badge" :class="item.status">{{ item.status === "fail" ? "✕" : "!" }}</div>
                <div class="check-body">
                  <div class="name">{{ item.name }}</div>
                  <div class="detail">{{ item.detail }}</div>
                  <div v-if="item.hint" class="hint">{{ item.hint }}</div>
                </div>
              </div>
            </div>
            <p style="color: var(--muted); font-size: 12.5px; margin-top: 8px">
              其余 {{ items.length - problems.length }} 项全部正常。
            </p>
          </template>
        </div>
        <p v-else style="color: var(--muted)">检测中…</p>
        <div class="wiz-actions">
          <button class="btn ghost" @click="prev">上一步</button>
          <button class="btn ghost" :disabled="scanning" @click="scan">{{ scanning ? "检测中…" : "重新检测" }}</button>
          <button class="btn" @click="next">{{ detectHasFail ? "有问题,仍要继续" : "下一步" }}</button>
        </div>
        <p v-if="detectHasFail" class="note red" style="margin-top: 10px">
          有红色项时建议先按提示解决(例如设置引擎路径),不然后面的步骤可能失败。
        </p>
      </div>

      <!-- 2 配置 -->
      <div v-else-if="step === 2" class="wiz-card">
        <h2>🔧 第二步:配置 Zwift</h2>
        <p class="wiz-lead">
          这一步会:① 把 zwift.com 四个域名指向本机(hosts)<br />
          ② 把离线服务器的证书导入系统信任区 ③ 给 Zwift 补上证书链。<br />
          每一步都会先备份,以后在「还原官方」页可以一键撤销。
        </p>
        <div class="wiz-actions" style="margin-top: 6px">
          <button class="btn ghost" @click="prev">上一步</button>
          <button class="btn" :disabled="configuring" @click="configure">
            {{ configuring ? "配置中…" : configRan ? "重新执行" : "一键配置" }}
          </button>
          <button class="btn" :disabled="!configRan || configuring" @click="next">下一步</button>
        </div>
        <div v-if="configSteps.length" style="margin-top: 14px">
          <div v-for="(s, i) in configSteps" :key="i" class="step" :class="s.status">
            <span class="tag">{{ s.status === "done" ? "已配置" : s.status === "skipped" ? "已满足" : "失败" }}</span>
            <span>{{ s.name + (s.detail ? ":" + s.detail : "") }}</span>
          </div>
        </div>
        <p v-if="configHasError" class="note red" style="margin-top: 10px">
          有步骤失败:最常见原因是没用管理员身份运行本程序。请右键启动器 → 以管理员身份运行,再试一次。
        </p>
      </div>

      <!-- 3 启动 -->
      <div v-else-if="step === 3" class="wiz-card">
        <h2>🚴 第三步:启动离线服务</h2>
        <p class="wiz-lead">启动 zoffline 服务器(占用 80/443/3024/3025 端口),几秒就好。</p>
        <div style="display: flex; align-items: center; gap: 12px; margin: 10px 0">
          <span class="pill" :class="status.running ? (status.healthy ? 'ok' : 'warn') : 'off'">
            <i />
            {{ !status.running ? "未运行" : status.healthy ? "运行中 · 443 正常" : "启动中 · 端口未就绪" }}
          </span>
        </div>
        <div class="wiz-actions">
          <button class="btn ghost" @click="prev">上一步</button>
          <button class="btn ok" :disabled="starting || status.running" @click="startService">
            {{ starting ? "启动中…" : status.running ? "已启动" : "▶ 启动服务" }}
          </button>
          <button class="btn" @click="next">{{ status.running ? "下一步" : "跳过,以后再启" }}</button>
        </div>
        <p class="note blue" style="margin-top: 10px">
          服务启动后,以后每次骑车:打开本启动器 → 启动服务 → 点「启动 Zwift」进游戏。骑行数据在引擎目录 storage 里,升级不丢。
        </p>
      </div>

      <!-- 4 完成 -->
      <div v-else class="wiz-card">
        <h2>🎉 引导完成!</h2>
        <p class="wiz-lead">
          你的状态:服务 {{ status.running ? "运行中 ✅" : "未启动(可随时在「服务控制」页启动)" }} · Strava 授权可在「Strava
          授权」页随时配置(需要 Strava 付费订阅)。
        </p>
        <ul class="wiz-tips">
          <li>每次骑行前:启动服务 → 启动 Zwift</li>
          <li>想回官方服务器:「还原官方」页一键撤销全部修改</li>
          <li>遇到问题:先去「环境检测」页重新体检</li>
        </ul>
        <div class="wiz-actions">
          <button class="btn ghost" @click="prev">上一步</button>
          <button class="btn" @click="finish">开始使用 🚀</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wizard {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100vh;
  background:
    radial-gradient(1200px 500px at 20% -10%, rgba(79, 124, 255, 0.14), transparent),
    var(--bg);
}
.wiz-header {
  display: flex;
  align-items: center;
  gap: 24px;
  padding: 16px 28px;
  border-bottom: 1px solid var(--border);
  background: rgba(23, 30, 46, 0.6);
}
.wiz-title { font-weight: 700; font-size: 15px; }
.wiz-steps { display: flex; gap: 18px; flex: 1; }
.wiz-step { display: flex; align-items: center; gap: 7px; color: var(--muted); font-size: 12.5px; }
.wiz-step-dot {
  width: 22px; height: 22px; border-radius: 50%;
  display: flex; align-items: center; justify-content: center;
  background: var(--panel-2); border: 1px solid var(--border);
  font-size: 11px; font-weight: 700;
}
.wiz-step.active { color: var(--text); }
.wiz-step.active .wiz-step-dot { background: var(--accent); border-color: var(--accent); color: #fff; }
.wiz-step.done .wiz-step-dot { background: var(--ok); border-color: var(--ok); color: #fff; }
.wiz-skip { color: var(--muted); font-size: 12px; cursor: pointer; }
.wiz-skip:hover { color: var(--accent); }

.wiz-body { flex: 1; overflow-y: auto; display: flex; justify-content: center; padding: 34px 20px; }
.wiz-card {
  width: 640px;
  max-width: 100%;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 30px 34px;
  height: fit-content;
}
.wiz-card h2 { font-size: 20px; margin-bottom: 10px; }
.wiz-lead { color: var(--muted); line-height: 1.8; margin-bottom: 14px; }
.wiz-features { display: flex; flex-direction: column; gap: 10px; margin-bottom: 16px; }
.wiz-feature {
  display: flex; flex-direction: column; gap: 3px;
  background: var(--panel-2); border-radius: 8px; padding: 12px 14px;
}
.wiz-feature b { font-size: 13.5px; }
.wiz-feature span { color: var(--muted); font-size: 12.5px; }
.wiz-actions { display: flex; gap: 10px; margin-top: 18px; }
.wiz-tips { color: var(--muted); line-height: 2; padding-left: 20px; margin-bottom: 4px; }
.check-list { display: flex; flex-direction: column; gap: 8px; }
</style>
