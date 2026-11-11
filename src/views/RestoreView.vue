<script setup lang="ts">
import { ref } from "vue";
import { api, type StepResult } from "../api";

const confirmed = ref(false);
const busy = ref(false);
const steps = ref<StepResult[]>([]);
const message = ref("");
const error = ref("");

async function restore() {
  busy.value = true;
  steps.value = [];
  error.value = "";
  message.value = "";
  try {
    const results = await api.restoreRun();
    steps.value = results;
    if (results.some((r) => r.status === "error")) {
      error.value = "有步骤失败:常见原因是没用管理员身份运行启动器。";
    } else {
      message.value = "已还原到 Zwift 官方服务器。现在打开 Zwift 连的就是官方了。";
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <h1>还原官方服务器</h1>
  <p class="sub">想回到 Zwift 官方服务器?这里会撤销启动器做过的所有修改。</p>

  <div class="card">
    <h3>将撤销以下修改(都是可逆的,放心)</h3>
    <div class="check-list">
      <div class="check-item">
        <div class="badge warn">1</div>
        <div class="check-body">
          <div class="name">hosts 重定向</div>
          <div class="detail">移除把 zwift.com 四个域名指向本机的行</div>
        </div>
      </div>
      <div class="check-item">
        <div class="badge warn">2</div>
        <div class="check-body">
          <div class="name">系统根证书</div>
          <div class="detail">从 Windows 根证书库 / macOS 钥匙串删除 *.zwift.com 证书</div>
        </div>
      </div>
      <div class="check-item">
        <div class="badge warn">3</div>
        <div class="check-body">
          <div class="name">Zwift 内置证书链 cacert.pem</div>
          <div class="detail">从自动备份恢复(或移除追加段)</div>
        </div>
      </div>
    </div>
    <div class="note blue" style="margin-top: 12px">
      你的骑行数据不受影响:zoffline 的存档都在引擎目录的 storage 文件夹里,不会被删除。
    </div>
    <div class="actions" style="align-items: center">
      <label style="display: flex; gap: 8px; align-items: center; color: var(--muted); cursor: pointer">
        <input type="checkbox" v-model="confirmed" />
        我确认要还原到官方服务器
      </label>
      <button class="btn danger" :disabled="!confirmed || busy" @click="restore">
        {{ busy ? "还原中…" : "↩️ 一键还原" }}
      </button>
    </div>
  </div>

  <div v-if="steps.length" class="card">
    <h3>执行结果</h3>
    <div v-for="(s, i) in steps" :key="i" class="step" :class="s.status">
      <span class="tag">
        {{ s.status === "done" ? "已还原" : s.status === "skipped" ? "无需处理" : "失败" }}
      </span>
      <span>{{ s.name + (s.detail ? ":" + s.detail : "") }}</span>
    </div>
  </div>

  <div v-if="message" class="note blue" style="margin-bottom: 12px">{{ message }}</div>
  <div v-if="error" class="note red" style="margin-bottom: 12px">{{ error }}</div>
</template>
