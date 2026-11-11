<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { api } from "./api";
import WizardView from "./views/WizardView.vue";
import SetupView from "./views/SetupView.vue";
import ServiceView from "./views/ServiceView.vue";
import SyncView from "./views/SyncView.vue";
import RestoreView from "./views/RestoreView.vue";
import ChangelogView from "./views/ChangelogView.vue";

// 首次启动(未完成引导)时显示引导页
const wizardMode = ref(localStorage.getItem("wizard_done") !== "1");
const tab = ref<"setup" | "service" | "sync" | "restore" | "update">("setup");
const running = ref(false);
const updateTip = ref("");
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
  // 启动静默检查更新,有新版在侧栏顶部提示
  api.checkUpdate().then(
    (u) => {
      if (u) updateTip.value = `🔄 有新版 v${u.version},到「检查更新」页安装`;
    },
    () => {}
  );
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
    <div class="nav-item" :class="{ active: tab === 'update' }" @click="tab = 'update'">
      🔄 检查更新
    </div>
    <div class="spacer" />
    <div v-if="updateTip" style="color: var(--warn); font-size: 11.5px; padding: 0 10px 6px; line-height: 1.5">
      {{ updateTip }}
    </div>
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
    <ChangelogView v-else-if="tab === 'update'" />
    <RestoreView v-else />
  </main>
  </template>
</template>
