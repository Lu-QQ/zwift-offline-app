<script setup lang="ts">
import { ref } from "vue";
import changelogRaw from "../../CHANGELOG.md?raw";
import { api } from "../api";

const checking = ref(false);
const installing = ref(false);
const message = ref("");
const error = ref("");
const updateAvailable = ref<{ version: string; notes: string } | null>(null);
const progress = ref("");

async function checkUpdate() {
  checking.value = true;
  error.value = "";
  message.value = "";
  updateAvailable.value = null;
  progress.value = "";
  try {
    const update = await api.checkUpdate();
    if (update) {
      updateAvailable.value = { version: update.version, notes: update.body || "(详见发布页)" };
    } else {
      message.value = "✅ 已是最新版本";
    }
  } catch (e) {
    error.value = `检查更新失败:${String(e)}(更新源走国内加速镜像,若都失败请检查网络或代理)`;
  } finally {
    checking.value = false;
  }
}

async function install() {
  installing.value = true;
  error.value = "";
  progress.value = "下载中…";
  try {
    const update = await api.checkUpdate();
    if (update) {
      await api.installUpdate(update, (pct) => {
        progress.value = pct >= 100 ? "安装中,即将重启…" : `下载中 ${pct}%`;
      });
    }
  } catch (e) {
    error.value = `安装失败:${String(e)}`;
    installing.value = false;
  }
}

// 简易 markdown 渲染:版本标题加粗分隔,列表项保留
function fmt(md: string): string {
  return md.trim();
}
</script>

<template>
  <h1>更新</h1>
  <p class="sub">检查新版本、查看更新日志。更新包带签名校验,装完自动重启。</p>

  <div class="card">
    <div style="display: flex; align-items: center; gap: 12px; flex-wrap: wrap">
      <button class="btn" :disabled="checking || installing" @click="checkUpdate">
        {{ checking ? "检查中…" : "🔄 检查更新" }}
      </button>
      <button v-if="updateAvailable" class="btn ok" :disabled="installing" @click="install">
        {{ installing ? progress || "安装中…" : `⬇ 下载并安装 v${updateAvailable.version}` }}
      </button>
    </div>
    <div v-if="updateAvailable" class="note blue" style="margin-top: 12px">
      发现新版本 <b>v{{ updateAvailable.version }}</b>:
      <div style="white-space: pre-wrap; margin-top: 6px; color: var(--muted)">{{ updateAvailable.notes }}</div>
    </div>
    <div v-if="message" class="note" style="background: rgba(52,192,122,.1); border-left: 3px solid var(--ok); color: var(--ok); margin-top: 12px">{{ message }}</div>
    <div v-if="error" class="note red" style="margin-top: 12px">{{ error }}</div>
  </div>

  <div class="card">
    <h3>📜 更新日志</h3>
    <div class="changelog" style="white-space: pre-wrap; line-height: 1.8; color: var(--text)">{{ fmt(changelogRaw) }}</div>
  </div>
</template>
