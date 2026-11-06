<script setup lang="ts">
import { computed } from 'vue';
import {
  NAlert, NButton, NDescriptions, NDescriptionsItem, NProgress, NSpace, NSwitch, useDialog, useMessage,
} from 'naive-ui';
import { formatSize } from '@/api';
import { useUpdaterStore } from '@/stores/updater';

const updates = useUpdaterStore();
const dialog = useDialog();
const message = useMessage();
const statusText = computed(() => ({
  idle: '尚未检查', checking: '正在检查', upToDate: '已是最新版本', available: '有新版本',
  downloading: '正在下载并校验', downloaded: '已下载，等待安装', installing: '正在安装，即将重启', error: '检查失败',
})[updates.phase]);

async function check() {
  if (await updates.check()) {
    message.info(updates.update ? `发现新版本 ${updates.update.version}` : '当前已是最新版本');
  }
}
function install() {
  dialog.warning({
    title: '安装更新并重启',
    content: '安装将关闭应用，完成后重新启动。请先完成或暂停传输任务。',
    positiveText: '安装并重启',
    negativeText: '取消',
    onPositiveClick: () => { void updates.install(); },
  });
}
</script>

<template>
  <div class="update-settings">
    <div class="auto-check-row">
      <strong id="auto-update-label" title="启动时及每 6 小时检查一次">自动检查更新</strong>
      <n-switch aria-labelledby="auto-update-label" :value="updates.autoCheck" :disabled="!updates.desktop" @update:value="updates.setAutoCheck" />
    </div>
    <n-descriptions :column="1" label-placement="left" size="small">
      <n-descriptions-item label="当前版本">{{ updates.currentVersion }}</n-descriptions-item>
      <n-descriptions-item label="更新状态">{{ statusText }}</n-descriptions-item>
      <n-descriptions-item v-if="updates.update" label="最新版本">{{ updates.update.version }}</n-descriptions-item>
      <n-descriptions-item v-if="updates.lastCheckedAt" label="上次检查">{{ updates.lastCheckedAt }}</n-descriptions-item>
    </n-descriptions>
    <n-alert v-if="!updates.canInstall" type="info" :show-icon="false">
      {{ updates.desktop ? '开发模式仅支持检查更新' : '请在桌面版中更新' }}
    </n-alert>
    <div v-if="updates.update?.notes" class="release-notes">
      <strong>更新内容</strong>
      <p>{{ updates.update.notes }}</p>
    </div>
    <div v-if="updates.phase === 'downloading'" class="download-progress">
      <n-progress type="line" :percentage="updates.progress" :processing="true" :show-indicator="!!updates.totalBytes" />
      <span>{{ formatSize(updates.downloadedBytes) }}{{ updates.totalBytes ? ' / ' + formatSize(updates.totalBytes) : ' · 正在下载并校验签名' }}</span>
    </div>
    <n-alert v-if="updates.blockedReason" type="warning" :show-icon="false">{{ updates.blockedReason }}</n-alert>
    <n-alert v-if="updates.error" type="error" :show-icon="false">{{ updates.error }}</n-alert>
    <n-space>
      <n-button size="small" secondary :loading="updates.phase === 'checking'"
        :disabled="!updates.desktop || updates.busy || updates.phase === 'downloaded'" @click="check">检查更新</n-button>
      <n-button v-if="updates.update && updates.phase !== 'downloaded' && updates.phase !== 'installing'"
        size="small" type="primary" :loading="updates.phase === 'downloading'"
        :disabled="!updates.canInstall || updates.busy" @click="updates.promptForDownload">
        {{ updates.error ? '重试下载' : '下载更新' }}
      </n-button>
      <n-button v-if="updates.phase === 'downloaded' || updates.phase === 'installing'" size="small" type="primary"
        :loading="updates.phase === 'installing'" :disabled="!updates.canInstall || updates.busy" @click="install">安装并重启</n-button>
    </n-space>
  </div>
</template>

<style scoped>
.update-settings { display: flex; flex-direction: column; gap: 14px; }
.auto-check-row { display: flex; justify-content: space-between; align-items: center; gap: 16px; }
.auto-check-row strong, .release-notes strong { font-size: 13px; font-weight: 500; }
.release-notes p { margin: 8px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 220px; overflow: auto; font-size: 13px; }
.download-progress span { color: var(--zinc-500); font-size: 12px; }
</style>
