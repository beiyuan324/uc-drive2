<script setup lang="ts">
import { computed, onActivated, ref } from 'vue';
import {
  NButton,
  NIcon,
  NCard,
  NDescriptions,
  NDescriptionsItem,
  NTag,
  NSpin,
  NInput,
  NInputNumber,
  NAlert,
  NCheckbox,
  NRadioGroup,
  NRadioButton,
  useMessage,
  NTooltip,
} from 'naive-ui';
import {
  PhArrowClockwise as RefreshIcon,
  PhFloppyDisk as SaveIcon,
  PhFolderOpen as FolderOpenIcon,
  PhCopy as CopyIcon,
  PhSun as SunIcon,
  PhMoon as MoonIcon,
  PhMonitor as MonitorIcon,
} from '@phosphor-icons/vue';
import { useSettingsStore } from '@/stores/settings';
import { api, getBase } from '@/api';
import type { ThemeMode } from '@/types';
import AppUpdateSettings from '@/components/AppUpdateSettings.vue';
import DriveCredentialSettings from '@/components/DriveCredentialSettings.vue';

const message = useMessage();
const settings = useSettingsStore();
const credentials = ref<InstanceType<typeof DriveCredentialSettings> | null>(null);
const health = ref<{ ok: boolean; gopeed: boolean; version: string } | null>(null);
const base = ref('');
const loading = ref(true);

// 存储目录（本地编辑，保存后提交后端）
const storageDirInput = ref('');
const moveFiles = ref(true);
const savingStorage = ref(false);
const storageChanged = ref(false);
const movedFiles = ref(0);
const copiedPath = ref(false);

function normalizePath(p: string): string {
  return String(p || '')
    .replace(/\\/g, '/')
    .replace(/\/+$/, '');
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
    setTimeout(() => {
      copiedPath.value = false;
    }, 1800);
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
    api
      .health()
      .then((h) => (health.value = h))
      .catch(() => (health.value = null)),
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

onActivated(refreshAll);
</script>

<template>
  <div class="settings-page">
    <div class="page-head">
      <h1 class="page-title">设置</h1>
      <n-button size="small" secondary @click="refreshAll(); credentials?.refresh()">
        <template #icon><n-icon :component="RefreshIcon" /></template>
        刷新状态
      </n-button>
    </div>

    <n-spin :show="loading">
      <div class="cards">
        <n-card class="card" title="外观" :bordered="false">
          <n-radio-group
            :value="settings.themeMode"
            class="appearance-options"
            @update:value="(value: string) => settings.applyTheme(value as ThemeMode)"
          >
            <n-radio-button value="light"
              ><n-icon :component="SunIcon" size="17" /><span>浅色</span></n-radio-button
            >
            <n-radio-button value="dark"
              ><n-icon :component="MoonIcon" size="17" /><span>深色</span></n-radio-button
            >
            <n-radio-button value="auto"
              ><n-icon :component="MonitorIcon" size="17" /><span>自动</span></n-radio-button
            >
          </n-radio-group>
        </n-card>
        <!-- 存储目录 -->
        <n-card class="card" title="存储目录" :bordered="false">
          <div class="storage-edit">
            <div class="storage-input-row">
              <n-input
                v-model:value="storageDirInput"
                placeholder="选择文件保存位置"
                :input-props="{ 'aria-label': '存储目录' }"
                class="storage-input"
              />
              <n-button size="medium" secondary @click="browseStorageDir">
                <template #icon><n-icon :component="FolderOpenIcon" /></template>
                浏览…
              </n-button>
            </div>
            <div class="storage-options-row">
              <n-checkbox v-model:checked="moveFiles"
                >迁移现有文件</n-checkbox
              >
            </div>
            <div class="storage-actions-row">
              <n-button
                size="small"
                secondary
                :disabled="!isCustomStorageDir"
                @click="resetStorageDir"
              >
                恢复默认
              </n-button>
              <n-button
                size="small"
                type="primary"
                secondary
                :loading="savingStorage"
                aria-label="保存存储目录"
                @click="saveStorageDir"
              >
                <template #icon><n-icon :component="SaveIcon" /></template>
                保存
              </n-button>
            </div>
            <n-alert v-if="storageChanged" type="success" :show-icon="false" class="storage-alert">
              存储目录已生效{{ movedFiles ? `，已成功迁移 ${movedFiles} 个文件` : '' }}。
            </n-alert>
          </div>
          <n-descriptions
            :column="1"
            label-placement="left"
            size="small"
            class="settings-desc-table"
          >
            <n-descriptions-item label="当前生效目录">
              <div class="path-display-cell">
                <span class="mono">{{ settings.info?.storageDir || '—' }}</span>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button
                      size="tiny"
                      quaternary
                      circle
                      aria-label="复制存储路径"
                      @click="copyStoragePath(settings.info?.storageDir || '')"
                    >
                      <template #icon><n-icon :component="copiedPath ? CheckIcon : CopyIcon" /></template>
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
        <n-card class="card" title="服务状态" :bordered="false">
          <n-descriptions
            :column="1"
            label-placement="left"
            size="small"
            class="settings-desc-table"
          >
            <n-descriptions-item label="本地服务">
              <div class="status-cell">
                <n-tag :type="health?.ok ? 'success' : 'error'" size="small" :bordered="false">
                  {{ health?.ok ? '运行中' : '不可用' }}
                </n-tag>
                <span class="mono inline-mono">{{ base || '127.0.0.1:17210' }}</span>
              </div>
            </n-descriptions-item>
            <n-descriptions-item label="下载引擎">
              <div class="status-cell">
                <n-tag
                  :type="settings.info?.gopeed.running ? 'success' : 'warning'"
                  size="small"
                  :bordered="false"
                >
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
        <n-card class="card" title="下载偏好" :bordered="false">
          <div class="dl-row">
            <span class="dl-name">UC 连接数</span>
            <div class="dl-input-wrapper">
              <n-input-number
                v-model:value="dlUc"
                :min="1"
                :max="1000"
                :step="50"
                class="dl-input"
                :input-props="{ 'aria-label': 'UC 连接数' }"
              />
              <span class="unit-label">连接</span>
            </div>
          </div>
          <div class="dl-row">
            <span class="dl-name">直链连接数（0 为自动）</span>
            <div class="dl-input-wrapper">
              <n-input-number
                v-model:value="dlHttp"
                :input-props="{ 'aria-label': '直链连接数' }"
                :min="0"
                :max="1000"
                :step="20"
                class="dl-input"
              />
              <span class="unit-label">连接</span>
            </div>
          </div>
          <div class="dl-row">
            <span class="dl-name">同时下载</span>
            <div class="dl-input-wrapper">
              <n-input-number v-model:value="dlMaxRunning" :input-props="{ 'aria-label': '同时下载任务数' }" :min="1" :max="10" class="dl-input" />
              <span class="unit-label">个任务</span>
            </div>
          </div>
          <div class="actions">
            <n-button
              size="small"
              type="primary"
              secondary
              :loading="savingDl"
              aria-label="保存下载偏好"
              @click="saveDownloadParams"
            >
              <template #icon><n-icon :component="SaveIcon" /></template>
              保存
            </n-button>
          </div>
        </n-card>

        <n-card class="card" title="网盘凭据" :bordered="false">
          <drive-credential-settings ref="credentials" />
        </n-card>

        <n-card class="card" title="软件更新" :bordered="false">
          <app-update-settings />
        </n-card>

      </div>
    </n-spin>
  </div>
</template>

<style scoped>
.cards {
  display: flex;
  flex-direction: column;
}
.card {
  border-radius: 0;
  background: transparent;
}
.card + .card {
  border-top: 1px solid var(--line);
}
.card :deep(.n-card-header) {
  padding: 24px 0 16px;
  font-size: 15px;
  font-weight: 600;
}
.card :deep(.n-card-content) {
  padding: 0 0 24px;
}
.appearance-options {
  display: inline-flex;
}
.appearance-options :deep(.n-radio__label) {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 16px;
}
.mono {
  font-family: Consolas, monospace;
  font-size: 12px;
  color: var(--zinc-600);
  overflow-wrap: anywhere;
}
.inline-mono {
  margin-left: 8px;
}
.settings-desc-table :deep(td) {
  padding: 6px 0;
}
.path-display-cell,
.status-cell {
  display: flex;
  align-items: center;
  gap: 6px;
}
.status-cell {
  flex-wrap: wrap;
}
.dl-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 0;
}
.dl-name {
  font-size: 13px;
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
  color: var(--zinc-500);
  width: 40px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 14px;
}
.storage-edit {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-bottom: 16px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--line);
}
.storage-input-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.storage-input {
  font-family: Consolas, monospace;
  font-size: 13px;
  flex: 1;
  min-width: 0;
}
.storage-options-row {
  font-size: 13px;
}
.storage-actions-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.storage-alert {
  font-size: 13px;
}
@media (max-width: 680px) {
  .storage-input-row,
  .storage-actions-row {
    flex-wrap: wrap;
  }
  .storage-input {
    flex-basis: 100%;
  }
  .dl-row {
    align-items: flex-start;
    flex-direction: column;
    gap: 12px;
  }
}
</style>
