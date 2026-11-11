<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import { api } from "../api";

const status = ref({ running: false, pid: null as number | null, healthy: false });
const logs = ref<string[]>([]);
const starting = ref(false);
const stopping = ref(false);
const message = ref("");
const error = ref("");
const logBox = ref<HTMLElement | null>(null);
let unlog: (() => void) | null = null;
let unexit: (() => void) | null = null;
let timer: number | undefined;

async function refresh() {
  try {
    status.value = await api.serviceStatus();
  } catch { /* ignore */ }
}

async function start() {
  starting.value = true;
  error.value = "";
  message.value = "";
  try {
    message.value = await api.serviceStart();
    logs.value = await api.serviceLogs();
  } catch (e) {
    error.value = String(e);
  } finally {
    starting.value = false;
    await refresh();
  }
}

async function stop() {
  stopping.value = true;
  error.value = "";
  try {
    message.value = await api.serviceStop();
  } catch (e) {
    error.value = String(e);
  } finally {
    stopping.value = false;
    await refresh();
  }
}

async function launch() {
  error.value = "";
  try {
    message.value = await api.launchZwift();
  } catch (e) {
    error.value = String(e);
  }
}

async function scrollBottom() {
  await nextTick();
  if (logBox.value) logBox.value.scrollTop = logBox.value.scrollHeight;
}

onMounted(async () => {
  logs.value = await api.serviceLogs();
  unlog = await api.onServiceLog(async (line) => {
    logs.value.push(line);
    if (logs.value.length > 2000) logs.value.splice(0, logs.value.length - 2000);
    await scrollBottom();
  });
  unexit = await api.onServiceExited(refresh);
  timer = window.setInterval(refresh, 2500);
  await refresh();
});
onUnmounted(() => {
  unlog?.();
  unexit?.();
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <h1>服务控制</h1>
  <p class="sub">启动 zoffline 离线服务,看到日志滚动后,再点「启动 Zwift」进游戏。</p>

  <div class="card">
    <div style="display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px">
      <div style="display: flex; align-items: center; gap: 12px">
        <span class="pill" :class="status.running ? (status.healthy ? 'ok' : 'warn') : 'off'">
          <i />
          {{ !status.running ? "未运行" : status.healthy ? "运行中 · 端口就绪" : "运行中 · 端口未就绪" }}
        </span>
        <span v-if="status.pid" style="color: var(--muted); font-size: 12px">PID {{ status.pid }}</span>
      </div>
      <div style="display: flex; gap: 10px">
        <button v-if="!status.running" class="btn ok" :disabled="starting" @click="start">
          {{ starting ? "启动中…" : "▶ 启动服务" }}
        </button>
        <button v-else class="btn danger" :disabled="stopping" @click="stop">
          {{ stopping ? "停止中…" : "■ 停止服务" }}
        </button>
        <button class="btn" @click="launch">🚴 启动 Zwift</button>
      </div>
    </div>
  </div>

  <div class="card">
    <h3>📜 服务日志(实时)</h3>
    <div class="log-view" ref="logBox">
      <div v-if="!logs.length" style="color: #5a6a8c">还没有日志。点上方「启动服务」试试。</div>
      <div v-for="(line, i) in logs" :key="i">{{ line }}</div>
    </div>
    <p style="color: var(--muted); font-size: 12px; margin-top: 8px">
      提示:启动需要几秒钟,等日志出现端口监听信息后再打开 Zwift。
    </p>
  </div>

  <div v-if="message" class="note blue" style="margin-bottom: 12px">{{ message }}</div>
  <div v-if="error" class="note red" style="margin-bottom: 12px">{{ error }}</div>
</template>
