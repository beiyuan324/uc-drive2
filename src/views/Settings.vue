<script setup lang="ts">
import { computed, onActivated, ref } from 'vue';
import { NButton, NIcon, NCard, NDescriptions, NDescriptionsItem, NTag, NSpin, NSpace, NInput, NInputNumber, NAlert, NCheckbox, useMessage } from 'naive-ui';
import { PhArrowClockwise as RefreshIcon, PhFloppyDisk as SaveIcon, PhTrash as TrashIcon, PhFolderOpen as FolderOpenIcon } from '@phosphor-icons/vue';
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

function normalizePath(p: string): string {
  return String(p || '').replace(/\\/g, '/').replace(/\/+$/, '');
}

const isCustomStorageDir = computed(() => {
  const info = settings.info;
  if (!info?.storageDir || !info.defaultStorageDir) return false;
  return normalizePath(info.storageDir) !== normalizePath(info.defaultStorageDir);
});

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
    <h2 class="page-title">设置</h2>

    <n-spin :show="loading">
      <div class="cards">
        <n-card title="存储" class="card">
          <div class="storage-edit">
            <div class="storage-label">
              <span class="storage-name">网盘存储目录</span>
              <span class="storage-desc">上传与下载完成的文件存放于此。默认为系统用户目录，可改为任意磁盘路径（如 D:\网盘）。</span>
            </div>
            <n-input
              v-model:value="storageDirInput"
              placeholder="选择或输入存储目录，例如 D:\网盘"
              class="storage-input"
            />
            <n-space class="storage-actions" :size="10" align="center">
              <n-button size="small" secondary @click="browseStorageDir">
                <template #icon><n-icon :component="FolderOpenIcon" /></template>
                浏览…
              </n-button>
              <n-checkbox v-model:checked="moveFiles">同时把现有文件移动到新目录（支持跨盘）</n-checkbox>
            </n-space>
            <n-space class="actions" justify="space-between" align="center">
              <n-button size="small" secondary :disabled="!isCustomStorageDir" @click="resetStorageDir">
                恢复默认目录
              </n-button>
              <n-button size="small" type="primary" secondary :loading="savingStorage" @click="saveStorageDir">
                <template #icon><n-icon :component="SaveIcon" /></template>
                保存存储目录
              </n-button>
            </n-space>
            <n-alert v-if="storageChanged" type="success" :show-icon="false" class="storage-alert">
              存储目录已生效{{ movedFiles ? `，已迁移 ${movedFiles} 个文件` : '' }}。
            </n-alert>
          </div>
          <n-descriptions :column="1" label-placement="left" size="small">
            <n-descriptions-item label="默认存储目录">
              <span class="mono">{{ settings.info?.defaultStorageDir || '—' }}</span>
            </n-descriptions-item>
            <n-descriptions-item label="数据目录（数据库/配置）">
              <span class="mono">{{ settings.info?.dataDir }}</span>
            </n-descriptions-item>
          </n-descriptions>
        </n-card>

        <n-card title="服务状态" class="card">
          <n-descriptions :column="1" label-placement="left" size="small">
            <n-descriptions-item label="后端服务">
              <n-tag :type="health?.ok ? 'success' : 'error'" :bordered="false">
                {{ health?.ok ? '运行中' : '不可用' }}
              </n-tag>
              <span class="mono inline-mono">{{ base }}</span>
            </n-descriptions-item>
            <n-descriptions-item label="下载引擎 gopeed">
              <n-tag :type="settings.info?.gopeed.running ? 'success' : 'warning'" :bordered="false">
                {{ settings.info?.gopeed.running ? '运行中' : '未运行' }}
              </n-tag>
              <span v-if="settings.info?.gopeed.port" class="mono inline-mono">
                127.0.0.1:{{ settings.info.gopeed.port }}
              </span>
            </n-descriptions-item>
          </n-descriptions>
          <n-space class="actions" justify="end">
            <n-button size="small" secondary @click="refreshAll">
              <template #icon><n-icon :component="RefreshIcon" /></template>
              刷新状态
            </n-button>
          </n-space>
        </n-card>

        <n-card title="下载参数" class="card">
          <div class="dl-row">
            <div class="dl-label">
              <span class="dl-name">UC 直链并发连接数</span>
              <span class="dl-desc">UC 直链每连接限速约 100KB/s，多连接可线性叠加。默认 300。</span>
            </div>
            <n-input-number
              v-model:value="dlUc"
              :min="1"
              :max="1000"
              :step="50"
              class="dl-input"
            />
          </div>
          <div class="dl-row">
            <div class="dl-label">
              <span class="dl-name">普通链接并发连接数</span>
              <span class="dl-desc">适用于普通 HTTP/HTTPS 下载任务；0 表示使用 gopeed 全局默认（500）。</span>
            </div>
            <n-input-number
              v-model:value="dlHttp"
              :min="0"
              :max="1000"
              :step="20"
              class="dl-input"
            />
          </div>
          <div class="dl-row">
            <div class="dl-label">
              <span class="dl-name">同时下载任务数</span>
              <span class="dl-desc">超过此数量的任务会排队等待，默认 3。</span>
            </div>
            <n-input-number
              v-model:value="dlMaxRunning"
              :min="1"
              :max="10"
              class="dl-input"
            />
          </div>
          <n-space class="actions" justify="end">
            <n-button size="small" type="primary" secondary :loading="savingDl" @click="saveDownloadParams">
              <template #icon><n-icon><save-icon /></n-icon></template>
              保存下载参数
            </n-button>
          </n-space>
        </n-card>

        <n-card title="UC 网盘" class="card">
          <n-alert type="info" :show-icon="false" class="cookie-alert">
            部分分享解析与直链刷新需要有效 Cookie。获取方式：登录 UC 网盘网页版 → F12 → Network → 复制请求头 Cookie。存储采用 AES-256 加密。
          </n-alert>
          <n-input
            v-model:value="cookieInput"
            type="textarea"
            :rows="3"
            placeholder="粘贴 UC 网盘 Cookie（保存时 AES-256 加密）"
            class="cookie-input"
          />
          <n-space class="actions" :size="10">
            <n-button size="small" type="primary" secondary :loading="savingCookie" @click="saveCookie">
              <template #icon><n-icon :component="SaveIcon" /></template>
              保存 Cookie
            </n-button>
            <n-button size="small" secondary :disabled="!hasCookie" @click="removeCookie">
              <template #icon><n-icon :component="TrashIcon" /></template>
              清除
            </n-button>
            <n-tag v-if="hasCookie" type="success" size="small" :bordered="false">已配置</n-tag>
            <n-tag v-else type="warning" size="small" :bordered="false">未配置</n-tag>
          </n-space>
        </n-card>

        <n-card title="关于" class="card">
          <n-descriptions :column="1" label-placement="left" size="small">
            <n-descriptions-item label="版本">
              <span class="mono">{{ health?.version || '1.1.0' }}</span>
            </n-descriptions-item>
          </n-descriptions>
        </n-card>
      </div>
    </n-spin>
  </div>
</template>

<style scoped>
.settings-page {
  max-width: 720px;
}
.page-title {
  margin: 0 0 20px;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.cards {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.card {
  border-radius: var(--radius-panel);
}
.mono {
  font-family: 'Cascadia Code', 'JetBrains Mono', Consolas, monospace;
  font-size: 12.5px;
  color: var(--zinc-500);
  word-break: break-all;
}
.inline-mono {
  margin-left: 10px;
}
.actions {
  margin-top: 12px;
}
.dl-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 0;
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
.dl-input {
  width: 130px;
  flex-shrink: 0;
}
.cookie-alert {
  margin-bottom: 12px;
  font-size: 13px;
}
.cookie-input {
  margin-bottom: 4px;
}
.storage-edit {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-bottom: 14px;
  margin-bottom: 14px;
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
.storage-input {
  font-family: 'Cascadia Code', 'JetBrains Mono', Consolas, monospace;
  font-size: 12.5px;
}
.storage-actions {
  flex-wrap: wrap;
}
.storage-alert {
  font-size: 13px;
}
[data-theme='dark'] .storage-edit {
  border-bottom-color: var(--zinc-800);
}
</style>
