<script setup lang="ts">
import { computed, onActivated, ref } from 'vue';
import { NButton, NIcon, NCard, NDescriptions, NDescriptionsItem, NTag, NSpin, NSpace, NInput, NInputNumber, NAlert, NCheckbox, useMessage, NTooltip } from 'naive-ui';
import {
  PhArrowClockwise as RefreshIcon, PhFloppyDisk as SaveIcon, PhTrash as TrashIcon,
  PhFolderOpen as FolderOpenIcon, PhFolderSimple as StorageIcon, PhPulse as PulseIcon,
  PhSliders as SlidersIcon, PhShieldCheck as ShieldIcon, PhInfo as InfoIcon,
  PhCopy as CopyIcon, PhCheck as CheckIcon,
} from '@phosphor-icons/vue';
import { useSettingsStore } from '@/stores/settings';
import { api, getBase } from '@/api';

const message = useMessage();
const settings = useSettingsStore();
const health = ref<{ ok: boolean; gopeed: boolean; version: string } | null>(null);
const base = ref('');
const loading = ref(true);

// UC Cookie
const hasCookie = ref(false);
const cookieInput = ref('');
const savingCookie = ref(false);

// 存储目录（本地编辑，保存后提交后端）
const storageDirInput = ref('');
const moveFiles = ref(true);
const savingStorage = ref(false);
const storageChanged = ref(false);
const movedFiles = ref(0);
const copiedPath = ref(false);

function normalizePath(p: string): string {
  return String(p || '').replace(/\\/g, '/').replace(/\/+$/, '');
}

const isCustomStorageDir = computed(() => {
  const info = settings.info;
  if (!info?.storageDir || !info.defaultStorageDir) return false;
  return normalizePath(info.storageDir) !== normalizePath(info.defaultStorageDir);
});

async function copyStoragePath(pathText: string) {
  if (!pathText) return;
  try {
    await navigator.clipboard.writeText(pathText);
    copiedPath.value = true;
    setTimeout(() => { copiedPath.value = false; }, 1800);
    message.success('路径已复制');
  } catch {
    message.warning('复制失败，请手动复制');
  }
}

async function browseStorageDir() {
  try {
    // 系统目录选择器（Tauri 环境可用）；浏览器开发模式降级为手动输入
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ directory: true, multiple: false, title: '选择网盘存储目录' });
    if (typeof picked === 'string' && picked) storageDirInput.value = picked;
  } catch {
    message.info('当前环境不支持目录选择器，请直接输入路径');
  }
}

async function saveStorageDir() {
  const dir = storageDirInput.value.trim();
  if (!dir) return message.warning('请输入存储目录');
  savingStorage.value = true;
  storageChanged.value = false;
  try {
    const info = await settings.saveStorageDir(dir, moveFiles.value);
    movedFiles.value = info?.movedFiles ?? 0;
    storageChanged.value = true;
    message.success(info?.changed ? '存储目录已切换，文件已就位' : '当前已是该目录');
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    savingStorage.value = false;
  }
}

function resetStorageDir() {
  storageDirInput.value = settings.info?.defaultStorageDir || '';
}

// 下载参数（本地编辑，保存后提交后端）
const dlUc = ref(300);
const dlHttp = ref(0);
const dlMaxRunning = ref(3);
const savingDl = ref(false);

async function refreshAll() {
  loading.value = true;
  await Promise.all([
    settings.load(),
    api.health().then(h => (health.value = h)).catch(() => (health.value = null)),
    api.cookieStatus().then(r => (hasCookie.value = r.hasCookie)).catch(() => {}),
  ]);
  dlUc.value = settings.downloadConfig.ucConnections;
  dlHttp.value = settings.downloadConfig.httpConnections;
  dlMaxRunning.value = settings.downloadConfig.maxRunning;
  storageDirInput.value = settings.info?.storageDir || '';
  base.value = await getBase().catch(() => '');
  loading.value = false;
}

async function saveDownloadParams() {
  savingDl.value = true;
  try {
    await settings.saveDownloadConfig({
      ucConnections: dlUc.value,
      httpConnections: dlHttp.value,
      maxRunning: dlMaxRunning.value,
    });
    message.success('下载参数已保存');
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    savingDl.value = false;
  }
}

async function saveCookie() {
  const v = cookieInput.value.trim();
  if (!v) return message.warning('请输入 Cookie');
  savingCookie.value = true;
  try {
    await api.saveCookie(v);
    hasCookie.value = true;
    cookieInput.value = '';
    message.success('Cookie 已加密保存');
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    savingCookie.value = false;
  }
}

async function removeCookie() {
  try {
    await api.clearCookie();
    hasCookie.value = false;
    cookieInput.value = '';
    message.success('Cookie 已清除');
  } catch (e) {
    message.error((e as Error).message);
  }
}

onActivated(refreshAll);
</script>

<template>
  <div class="settings-page">
    <div class="page-head">
      <div>
        <h2 class="page-title">系统设置</h2>
        <p class="page-desc">网盘存储位置、服务引擎状态、并发连接调优与 UC 网盘凭据管理</p>
      </div>
      <n-button size="small" secondary @click="refreshAll">
        <template #icon><n-icon :component="RefreshIcon" /></template>
        刷新状态
      </n-button>
    </div>

    <n-spin :show="loading">
      <div class="cards">
        <!-- 存储目录 -->
        <n-card class="card" :bordered="true">
          <template #header>
            <div class="card-title-row">
              <div class="card-title-icon"><n-icon :component="StorageIcon" size="18" color="var(--accent)" /></div>
              <span>存储目录</span>
            </div>
          </template>
          <div class="storage-edit">
            <div class="storage-label">
              <span class="storage-name">本地网盘文件根目录</span>
              <span class="storage-desc">上传、下载完成及离线转存的文件存放于此。支持任意磁盘路径（如 D:\网盘）。</span>
            </div>
            <div class="storage-input-row">
              <n-input
                v-model:value="storageDirInput"
                placeholder="输入或浏览选择网盘存储目录，例如 D:\网盘"
                class="storage-input"
              />
              <n-button size="medium" secondary @click="browseStorageDir">
                <template #icon><n-icon :component="FolderOpenIcon" /></template>
                浏览…
              </n-button>
            </div>
            <div class="storage-options-row">
              <n-checkbox v-model:checked="moveFiles">保存时同时将现有文件迁移到新目录（支持跨分区）</n-checkbox>
            </div>
            <div class="storage-actions-row">
              <n-button size="small" secondary :disabled="!isCustomStorageDir" @click="resetStorageDir">
                恢复默认存储路径
              </n-button>
              <n-button size="small" type="primary" secondary :loading="savingStorage" @click="saveStorageDir">
                <template #icon><n-icon :component="SaveIcon" /></template>
                保存并应用存储目录
              </n-button>
            </div>
            <n-alert v-if="storageChanged" type="success" :show-icon="false" class="storage-alert">
              存储目录已生效{{ movedFiles ? `，已成功迁移 ${movedFiles} 个文件` : '' }}。
            </n-alert>
          </div>
          <n-descriptions :column="1" label-placement="left" size="small" class="settings-desc-table">
            <n-descriptions-item label="当前生效目录">
              <div class="path-display-cell">
                <span class="mono">{{ settings.info?.storageDir || '—' }}</span>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary circle @click="copyStoragePath(settings.info?.storageDir || '')">
                      <template #icon><n-icon :component="CopyIcon" /></template>
                    </n-button>
                  </template>
                  复制路径
                </n-tooltip>
              </div>
            </n-descriptions-item>
            <n-descriptions-item label="默认系统目录">
              <span class="mono">{{ settings.info?.defaultStorageDir || '—' }}</span>
            </n-descriptions-item>
            <n-descriptions-item label="数据库与日志目录">
              <span class="mono">{{ settings.info?.dataDir }}</span>
            </n-descriptions-item>
          </n-descriptions>
        </n-card>

        <!-- 服务引擎状态 -->
        <n-card class="card" :bordered="true">
          <template #header>
            <div class="card-title-row">
              <div class="card-title-icon"><n-icon :component="PulseIcon" size="18" color="var(--accent)" /></div>
              <span>服务引擎运行状态</span>
            </div>
          </template>
          <n-descriptions :column="1" label-placement="left" size="small" class="settings-desc-table">
            <n-descriptions-item label="内置后端服务 (Rust / axum)">
              <div class="status-cell">
                <span v-if="health?.ok" class="pulse-dot"></span>
                <n-tag :type="health?.ok ? 'success' : 'error'" size="small" :bordered="false">
                  {{ health?.ok ? '运行中' : '不可用' }}
                </n-tag>
                <span class="mono inline-mono">{{ base || '127.0.0.1:17210' }}</span>
              </div>
            </n-descriptions-item>
            <n-descriptions-item label="下载引擎 (gopeed-web)">
              <div class="status-cell">
                <span v-if="settings.info?.gopeed.running" class="pulse-dot"></span>
                <n-tag :type="settings.info?.gopeed.running ? 'success' : 'warning'" size="small" :bordered="false">
                  {{ settings.info?.gopeed.running ? '运行中' : '未启动' }}
                </n-tag>
                <span v-if="settings.info?.gopeed.port" class="mono inline-mono">
                  127.0.0.1:{{ settings.info.gopeed.port }}
                </span>
              </div>
            </n-descriptions-item>
          </n-descriptions>
        </n-card>

        <!-- 下载并发调优 -->
        <n-card class="card" :bordered="true">
          <template #header>
            <div class="card-title-row">
              <div class="card-title-icon"><n-icon :component="SlidersIcon" size="18" color="var(--accent)" /></div>
              <span>下载并发参数调优</span>
            </div>
          </template>
          <div class="dl-row">
            <div class="dl-label">
              <span class="dl-name">UC 直链并发连接数</span>
              <span class="dl-desc">UC 直链每连接限速约 100KB/s，多连接可线性叠加提速。推荐 200~500。</span>
            </div>
            <div class="dl-input-wrapper">
              <n-input-number
                v-model:value="dlUc"
                :min="1"
                :max="1000"
                :step="50"
                class="dl-input"
              />
              <span class="unit-label">连接</span>
            </div>
          </div>
          <div class="dl-row">
            <div class="dl-label">
              <span class="dl-name">普通 HTTP / HTTPS 任务连接数</span>
              <span class="dl-desc">适用于普通直链下载；填 0 表示遵循 gopeed 全局引擎默认设置（500）。</span>
            </div>
            <div class="dl-input-wrapper">
              <n-input-number
                v-model:value="dlHttp"
                :min="0"
                :max="1000"
                :step="20"
                class="dl-input"
              />
              <span class="unit-label">连接</span>
            </div>
          </div>
          <div class="dl-row">
            <div class="dl-label">
              <span class="dl-name">同时并发下载任务数</span>
              <span class="dl-desc">超过此数量的任务将自动在队列中排队等待调度。默认 3。</span>
            </div>
            <div class="dl-input-wrapper">
              <n-input-number
                v-model:value="dlMaxRunning"
                :min="1"
                :max="10"
                class="dl-input"
              />
              <span class="unit-label">个任务</span>
            </div>
          </div>
          <div class="actions" style="display: flex; justify-content: flex-end;">
            <n-button size="small" type="primary" secondary :loading="savingDl" @click="saveDownloadParams">
              <template #icon><n-icon :component="SaveIcon" /></template>
              保存并发参数
            </n-button>
          </div>
        </n-card>

        <!-- UC 网盘凭据 -->
        <n-card class="card" :bordered="true">
          <template #header>
            <div class="card-title-row">
              <div class="card-title-icon"><n-icon :component="ShieldIcon" size="18" color="var(--accent)" /></div>
              <span>UC 网盘 Cookie 凭证</span>
            </div>
          </template>
          <n-alert type="info" :show-icon="false" class="cookie-alert">
            部分私密分享解析与大文件直链超时刷新需要有效 Cookie。存储采用本地 <strong>AES-256-GCM</strong> 高强度加密，确保凭据安全。
          </n-alert>
          <n-input
            v-model:value="cookieInput"
            type="textarea"
            :rows="3"
            placeholder="粘贴 UC 网盘网页版请求头中的 Cookie（保存时将本地加密存储）"
            class="cookie-input"
          />
          <div class="cookie-actions-row">
            <n-space size="small" align="center">
              <n-button size="small" type="primary" secondary :loading="savingCookie" @click="saveCookie">
                <template #icon><n-icon :component="SaveIcon" /></template>
                加密保存 Cookie
              </n-button>
              <n-button size="small" secondary :disabled="!hasCookie" @click="removeCookie">
                <template #icon><n-icon :component="TrashIcon" /></template>
                清除凭据
              </n-button>
            </n-space>
            <n-tag v-if="hasCookie" type="success" size="small" :bordered="false">已配置有效 Cookie</n-tag>
            <n-tag v-else type="warning" size="small" :bordered="false">未配置 Cookie</n-tag>
          </div>
        </n-card>

        <!-- 关于与技术栈 -->
        <n-card class="card" :bordered="true">
          <template #header>
            <div class="card-title-row">
              <div class="card-title-icon"><n-icon :component="InfoIcon" size="18" color="var(--zinc-500)" /></div>
              <span>关于 uc-drive2</span>
            </div>
          </template>
          <n-descriptions :column="1" label-placement="left" size="small" class="settings-desc-table">
            <n-descriptions-item label="客户端版本">
              <span class="mono">{{ health?.version || '2.0.0' }}</span>
            </n-descriptions-item>
            <n-descriptions-item label="架构技术栈">
              <span class="stack-text">Tauri v2 + Vue3 / Naive UI + Rust (axum) + SQLite (WAL) + gopeed</span>
            </n-descriptions-item>
          </n-descriptions>
        </n-card>
      </div>
    </n-spin>
  </div>
</template>

<style scoped>
.settings-page {
  max-width: 780px;
}
.page-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 20px;
}
.page-title {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.page-desc {
  margin: 4px 0 0;
  font-size: 13px;
  color: var(--zinc-500);
}
.cards {
  display: flex;
  flex-direction: column;
  gap: 18px;
}
.card {
  border-radius: var(--radius-panel);
  background: var(--zinc-50);
}
.card-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
}
.card-title-icon {
  display: flex;
  align-items: center;
}
.mono {
  font-family: 'Cascadia Code', 'JetBrains Mono', Consolas, monospace;
  font-size: 12.5px;
  color: var(--zinc-600);
  word-break: break-all;
}
.inline-mono {
  margin-left: 8px;
}
.settings-desc-table :deep(td) {
  padding: 6px 0;
}
.path-display-cell {
  display: flex;
  align-items: center;
  gap: 6px;
}
.status-cell {
  display: flex;
  align-items: center;
  gap: 6px;
}
.dl-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 0;
  border-bottom: 1px solid var(--zinc-100);
}
.dl-row:last-of-type {
  border-bottom: none;
}
.dl-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.dl-name {
  font-size: 13.5px;
  font-weight: 500;
}
.dl-desc {
  font-size: 12px;
  color: var(--zinc-500);
}
.dl-input-wrapper {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
.dl-input {
  width: 120px;
}
.unit-label {
  font-size: 12px;
  color: var(--zinc-400);
  width: 40px;
}
.actions {
  margin-top: 14px;
}
.storage-edit {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-bottom: 16px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--zinc-100);
}
.storage-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.storage-name {
  font-size: 13.5px;
  font-weight: 500;
}
.storage-desc {
  font-size: 12px;
  color: var(--zinc-500);
}
.storage-input-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.storage-input {
  font-family: 'Cascadia Code', 'JetBrains Mono', Consolas, monospace;
  font-size: 12.5px;
  flex: 1;
}
.storage-options-row {
  font-size: 13px;
}
.storage-actions-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 4px;
}
.storage-alert {
  font-size: 13px;
  margin-top: 4px;
}
.cookie-alert {
  margin-bottom: 12px;
  font-size: 13px;
}
.cookie-input {
  margin-bottom: 10px;
  font-family: 'Cascadia Code', 'JetBrains Mono', Consolas, monospace;
  font-size: 12px;
}
.cookie-actions-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.stack-text {
  font-size: 12.5px;
  color: var(--zinc-500);
}
[data-theme='dark'] .storage-edit,
[data-theme='dark'] .dl-row {
  border-bottom-color: var(--zinc-800);
}
</style>
