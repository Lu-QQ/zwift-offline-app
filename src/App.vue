<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { api } from "./api";
import WizardView from "./views/WizardView.vue";
import SetupView from "./views/SetupView.vue";
import ServiceView from "./views/ServiceView.vue";
import SyncView from "./views/SyncView.vue";
import RestoreView from "./views/RestoreView.vue";

// 首次启动(未完成引导)时显示引导页
const wizardMode = ref(localStorage.getItem("wizard_done") !== "1");
const tab = ref<"setup" | "service" | "sync" | "restore">("setup");
const running = ref(false);
let unlistenExit: (() => void) | null = null;
let timer: number | undefined;

async function refresh() {
  try {
    const s = await api.serviceStatus();
    running.value = s.running;
  } catch {
    running.value = false;
  }
}

onMounted(async () => {
  await refresh();
  timer = window.setInterval(refresh, 2500);
  unlistenExit = await api.onServiceExited(refresh);
});
onUnmounted(() => {
  unlistenExit?.();
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <!-- 首启引导 / 主界面 -->
  <WizardView v-if="wizardMode" @finish="wizardMode = false" />
  <template v-else>
    <aside class="sidebar">
    <div class="brand">
      zoffline 启动器
      <small>Zwift 离线小助手</small>
    </div>
    <div class="nav-item" :class="{ active: tab === 'setup' }" @click="tab = 'setup'">
      🩺 环境检测
    </div>
    <div class="nav-item" :class="{ active: tab === 'service' }" @click="tab = 'service'">
      🚴 服务控制
    </div>
    <div class="nav-item" :class="{ active: tab === 'sync' }" @click="tab = 'sync'">
      ☁️ 云同步
    </div>
    <div class="nav-item" :class="{ active: tab === 'restore' }" @click="tab = 'restore'">
      ↩️ 还原官方
    </div>
    <div class="spacer" />
    <div class="nav-item" style="font-size: 12px" @click="wizardMode = true">
      📖 重新运行引导
    </div>
    <div class="dot" :class="{ on: running }">
      <i />
      {{ running ? "服务运行中" : "服务未运行" }}
    </div>
  </aside>
  <main class="main">
    <SetupView v-if="tab === 'setup'" />
    <ServiceView v-else-if="tab === 'service'" />
    <SyncView v-else-if="tab === 'sync'" />
    <RestoreView v-else />
  </main>
  </template>
</template>
