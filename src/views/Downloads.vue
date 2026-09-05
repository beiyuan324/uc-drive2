<script setup lang="ts">
import { onActivated, onDeactivated, ref, computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useMessage, useDialog } from 'naive-ui';
import {
  NButton, NIcon, NInput, NModal, NProgress, NTag, NEmpty, NSpin, NRadioGroup, NRadioButton,
  NSpace, NTooltip, NTabs, NTab,
} from 'naive-ui';
import {
  PhLink as LinkIcon, PhMagnet as MagnetIcon, PhMagnetStraight as TorrentIcon,
  PhPlay as PlayIcon, PhPause as PauseIcon, PhTrash as TrashIcon, PhPlus as PlusIcon,
  PhCheckCircle as CheckIcon, PhWarningCircle as WarnIcon, PhClockClockwise as ClockIcon,
  PhKey as KeyIcon, PhHourglass as HourglassIcon, PhArrowDown as ArrowDownIcon,
  PhCheck as CheckMarkIcon,
} from '@phosphor-icons/vue';
import { useTasksStore } from '@/stores/tasks';
import { useSettingsStore } from '@/stores/settings';
import { api, formatSize, formatSpeed } from '@/api';
import type { TaskItem, TaskSource } from '@/types';

const router = useRouter();
const tasks = useTasksStore();
const settings = useSettingsStore();
const message = useMessage();
const dialog = useDialog();

type FilterTab = 'all' | 'running' | 'done' | 'error';
const currentTab = ref<FilterTab>('all');

const runningTasks = computed(() => tasks.tasks.filter(t => t.status === 'running' || t.status === 'queued'));
const doneTasks = computed(() => tasks.tasks.filter(t => t.status === 'done'));
const errorTasks = computed(() => tasks.tasks.filter(t => t.status === 'error' || t.status === 'cookie_expired'));

const filteredTasks = computed(() => {
  switch (currentTab.value) {
    case 'running': return runningTasks.value;
    case 'done': return doneTasks.value;
    case 'error': return errorTasks.value;
    default: return tasks.tasks;
  }
});

function goToSettings() {
  router.push('/settings');
}

async function clearCompleted() {
  const list = doneTasks.value;
  if (!list.length) return;
  dialog.warning({
    title: '清空已完成记录',
    content: `将从列表中清除 ${list.length} 个已完成的任务记录（文件依然保留在网盘中）。确定继续？`,
    positiveText: '清空',
    negativeText: '取消',
    onPositiveClick: async () => {
      for (const t of list) {
        await tasks.remove(t.id, false);
      }
      message.success('已清空完成任务');
    },
  });
}

// 下载完成通知（WebView 系统通知）
const notifiedIds = new Set<number>();
const notifiedCookieIds = new Set<number>();
function requestNotifyPermission() {
  try {
    if (typeof Notification !== 'undefined' && Notification.permission === 'default') {
      Notification.requestPermission();
    }
  } catch { /* 非浏览器环境忽略 */ }
}
function notify(title: string, body: string) {
  try {
    if (typeof Notification !== 'undefined' && Notification.permission === 'granted') {
      new Notification(title, { body, silent: true });
    }
  } catch { /* 通知失败不影响功能 */ }
}
function checkCompleted() {
  for (const t of tasks.tasks) {
    const uc = (t.metadata?.uc || {}) as Record<string, unknown>;
    const name = (uc.filename as string) || t.source_url?.split('/').pop() || `任务 ${t.id}`;
    if (t.status === 'done' && !notifiedIds.has(t.id)) {
      notifiedIds.add(t.id);
      notify('下载完成', name);
    }
    if (t.status === 'cookie_expired' && !notifiedCookieIds.has(t.id)) {
      notifiedCookieIds.add(t.id);
      notify('UC Cookie 已失效', name);
      message.warning(`「${name}」下载失败：UC Cookie 已失效，请到设置中更新后重新下载`, { duration: 8000 });
    }
  }
}

const showNew = ref(false);
const source = ref<TaskSource>('url');
const url = ref('');
const torrentFile = ref<File | null>(null);
const creating = ref(false);

async function confirmCreate() {
  let file: File | null = null;
  if (source.value === 'url' || source.value === 'magnet') {
    if (!url.value.trim()) return message.warning('请输入下载链接');
  } else {
    file = torrentFile.value;
  }
  if (!file && source.value === 'torrent') return message.warning('请选择 torrent 文件');
  creating.value = true;
  try {
    if (source.value === 'torrent' && file) {
      const tmp = await api.uploadTmp(file);
      await tasks.create({ source: 'torrent', torrentName: tmp.name });
    } else {
      const connections = source.value === 'url' ? settings.downloadConfig.httpConnections : undefined;
      await tasks.create({ source: source.value, url: url.value.trim(), connections });
    }
    message.success('任务已创建');
    showNew.value = false;
    url.value = '';
    torrentFile.value = null;
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    creating.value = false;
  }
}

async function togglePause(t: TaskItem) {
  if (t.status === 'running' || t.status === 'queued') await tasks.pause(t.id);
  else if (t.status === 'paused') await tasks.resume(t.id);
}

function confirmDelete(t: TaskItem) {
  const done = t.status === 'done';
  dialog.warning({
    title: '删除任务',
    content: done ? '任务已完成，文件已登记到网盘。删除任务记录？' : '删除任务将停止下载（已下载部分保留在临时目录）。',
    positiveText: '删除',
    negativeText: '取消',
    onPositiveClick: async () => {
      await tasks.remove(t.id, false);
      message.success('已删除');
    },
  });
}

const statusMeta: Record<string, { label: string; type: 'info' | 'success' | 'warning' | 'error' | 'default'; icon: any }> = {
  queued: { label: '排队中', type: 'default', icon: ClockIcon },
  running: { label: '下载中', type: 'info', icon: ArrowDownIcon },
  paused: { label: '已暂停', type: 'warning', icon: PauseIcon },
  done: { label: '已完成', type: 'success', icon: CheckIcon },
  error: { label: '失败', type: 'error', icon: WarnIcon },
  cookie_expired: { label: 'Cookie 失效', type: 'warning', icon: KeyIcon },
};

function taskTitle(t: TaskItem): string {
  const uc = (t.metadata?.uc || {}) as Record<string, string>;
  if (uc.filename) return uc.filename;
  if (t.source === 'magnet') return t.source_url?.startsWith('magnet:') ? '磁力任务' : (t.source_url || `任务 ${t.id}`);
  if (t.source === 'torrent') return (t.metadata as Record<string, { name?: string }>)?.torrentInfo?.name || '种子任务';
  const clean = t.source_url?.split('?')[0]?.split('/').filter(Boolean).pop();
  return clean || `任务 ${t.id}`;
}

function taskLink(t: TaskItem): string | null {
  if (t.source === 'uc') return 'UC 直链解析下载';
  if (t.source === 'magnet') return t.source_url?.slice(0, 60) || null;
  if (t.source === 'torrent') return null;
  return t.source_url || null;
}

function taskTotal(t: TaskItem): number {
  const uc = (t.metadata?.uc || {}) as Record<string, unknown>;
  if (typeof uc.size === 'number' && uc.size > 0) return uc.size;
  if (typeof t.metadata.total === 'number' && t.metadata.total > 0) return t.metadata.total;
  return 0;
}

function downloadedBytes(t: TaskItem): number {
  const total = taskTotal(t);
  if (!total) return 0;
  return total * (t.progress / 100);
}

function isFinishing(t: TaskItem): boolean {
  return t.status === 'running' && t.progress >= 98;
}

function taskRemaining(t: TaskItem): string {
  const total = taskTotal(t);
  if (!total || t.status !== 'running') return '';
  const dl = total * t.progress / 100;
  const mb = Math.max(0, (total - dl) / 1048576);
  if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(0, Math.round(mb * 1024))} KB`;
}

function taskEta(t: TaskItem): string | null {
  const total = taskTotal(t);
  if (!total || t.speed <= 0) return null;
  const dl = total * t.progress / 100;
  const sec = Math.max(0, total - dl) / t.speed;
  if (!Number.isFinite(sec) || sec <= 0 || sec > 3600 * 24) return null;
  if (sec < 60) return `${Math.ceil(sec)} 秒`;
  if (sec < 3600) return `${Math.floor(sec / 60)} 分 ${Math.ceil(sec % 60)} 秒`;
  return `${Math.floor(sec / 3600)} 时 ${Math.floor((sec % 3600) / 60)} 分`;
}

onActivated(() => {
  tasks.startPolling(2000);
  requestNotifyPermission();
});
onDeactivated(() => tasks.stopPolling());
watch(() => tasks.tasks, checkCompleted, { deep: true });
</script>

<template>
  <div class="downloads-page">
    <div class="page-head">
      <div>
        <h2 class="page-title">离线下载</h2>
        <p class="page-desc">基于 gopeed 引擎的高速下载与 UC 网盘直链解析任务</p>
      </div>
      <n-button type="primary" @click="showNew = true">
        <template #icon><n-icon :component="PlusIcon" /></template>
        新建任务
      </n-button>
    </div>

    <!-- 任务过滤分段栏与快捷操作 -->
    <div class="filter-bar">
      <div class="filter-tabs">
        <button
          class="filter-tab"
          :class="{ active: currentTab === 'all' }"
          @click="currentTab = 'all'"
        >
          全部 <span class="tab-count tabular-nums">{{ tasks.tasks.length }}</span>
        </button>
        <button
          class="filter-tab"
          :class="{ active: currentTab === 'running' }"
          @click="currentTab = 'running'"
        >
          下载中 <span class="tab-count tabular-nums">{{ runningTasks.length }}</span>
        </button>
        <button
          class="filter-tab"
          :class="{ active: currentTab === 'done' }"
          @click="currentTab = 'done'"
        >
          已完成 <span class="tab-count tabular-nums">{{ doneTasks.length }}</span>
        </button>
        <button
          class="filter-tab"
          :class="{ active: currentTab === 'error' }"
          @click="currentTab = 'error'"
        >
          异常/失败 <span class="tab-count tabular-nums">{{ errorTasks.length }}</span>
        </button>
      </div>
      <div class="filter-actions">
        <n-button
          v-if="doneTasks.length > 0"
          size="small"
          quaternary
          @click="clearCompleted"
        >
          <template #icon><n-icon :component="TrashIcon" /></template>
          清空已完成 ({{ doneTasks.length }})
        </n-button>
      </div>
    </div>

    <n-spin :show="tasks.loading">
      <div v-if="filteredTasks.length === 0" class="empty-box">
        <n-empty :description="currentTab === 'all' ? '暂无任何下载任务' : currentTab === 'running' ? '暂无正在下载的任务' : currentTab === 'done' ? '暂无已完成的下载任务' : '暂无异常任务'">
          <template #extra v-if="currentTab === 'all'">
            <n-button type="primary" secondary size="small" @click="showNew = true">
              <template #icon><n-icon :component="PlusIcon" /></template>
              创建第一个任务
            </n-button>
          </template>
        </n-empty>
      </div>

      <div v-else class="task-list">
        <div v-for="t in filteredTasks" :key="t.id" class="task-item hover-lift">
          <div class="task-icon" :class="`icon-${t.status}`">
            <n-icon
              :component="statusMeta[t.status]?.icon || ClockIcon"
              size="20"
              :color="t.status === 'done' ? 'var(--accent)' : t.status === 'error' ? '#ef4444' : t.status === 'cookie_expired' ? '#f59e0b' : undefined"
            />
          </div>
          <div class="task-main">
            <div class="task-header-row">
              <div class="task-name" :title="t.source_url || `任务 #${t.id}`">{{ taskTitle(t) }}</div>
              <span class="task-source-badge">{{ t.source === 'torrent' ? 'torrent' : t.source }}</span>
            </div>
            <div v-if="taskLink(t)" class="task-link" :title="t.source_url || ''">{{ taskLink(t) }}</div>

            <!-- 详细状态与速度信息 -->
            <div class="task-sub">
              <n-tag size="small" :type="statusMeta[t.status]?.type" :bordered="false">
                {{ statusMeta[t.status]?.label || t.status }}
              </n-tag>
              <span v-if="t.status === 'running' && isFinishing(t)" class="task-finishing">
                <n-icon :component="HourglassIcon" size="13" />
                正在合并收尾 · 剩余约 {{ taskRemaining(t) }}
              </span>
              <span v-else-if="t.status === 'running'" class="task-speed-box">
                <span class="pulse-dot" style="margin-right: 4px;"></span>
                <span class="task-speed tabular-nums">{{ formatSpeed(t.speed) }}</span>
                <span v-if="taskEta(t)" class="task-eta tabular-nums">· 约剩 {{ taskEta(t) }}</span>
              </span>
              <span v-else-if="t.status === 'queued'" class="task-queue-hint">
                队列中等待，最多同时并发 {{ settings.downloadConfig.maxRunning }} 个
              </span>
            </div>

            <!-- Cookie 失效特别引导条 -->
            <div v-if="t.status === 'cookie_expired'" class="cookie-expired-banner">
              <div class="cookie-banner-text">
                <n-icon :component="KeyIcon" size="15" color="#d97706" />
                <span>UC Cookie 已失效，无法自动换取直链</span>
              </div>
              <n-button size="tiny" type="warning" secondary @click="goToSettings">前往设置更新</n-button>
            </div>

            <!-- 进度条与数值 -->
            <template v-if="t.status !== 'done'">
              <div class="progress-info-row">
                <span class="progress-pct tabular-nums">{{ t.progress.toFixed(1) }}%</span>
                <span v-if="taskTotal(t) > 0" class="progress-bytes tabular-nums">
                  {{ formatSize(downloadedBytes(t)) }} / {{ formatSize(taskTotal(t)) }}
                </span>
              </div>
              <n-progress
                :percentage="t.progress"
                :show-indicator="false"
                :height="5"
                :border-radius="3"
                class="task-progress"
                :status="t.status === 'error' ? 'error' : t.status === 'cookie_expired' ? 'warning' : 'default'"
              />
            </template>
            <div v-else class="task-done-hint">
              <n-icon :component="CheckMarkIcon" size="14" />
              <span>已完整下载并登记至网盘根目录</span>
              <span v-if="taskTotal(t) > 0" class="done-size tabular-nums">（{{ formatSize(taskTotal(t)) }}）</span>
            </div>
          </div>

          <div class="task-actions">
            <n-tooltip trigger="hover">
              <template #trigger>
                <n-button
                  size="small"
                  quaternary
                  circle
                  :disabled="t.status === 'done' || t.status === 'error' || t.status === 'cookie_expired'"
                  @click="togglePause(t)"
                  :aria-label="t.status === 'paused' ? '继续下载' : '暂停下载'"
                >
                  <template #icon>
                    <n-icon :component="t.status === 'paused' ? PlayIcon : PauseIcon" />
                  </template>
                </n-button>
              </template>
              {{ t.status === 'paused' ? '继续下载' : '暂停下载' }}
            </n-tooltip>
            <n-tooltip trigger="hover">
              <template #trigger>
                <n-button size="small" quaternary circle type="error" @click="confirmDelete(t)" aria-label="删除任务">
                  <template #icon><n-icon :component="TrashIcon" /></template>
                </n-button>
              </template>
              删除任务记录
            </n-tooltip>
          </div>
        </div>
      </div>
    </n-spin>

    <!-- 新建任务弹窗 -->
    <n-modal v-model:show="showNew" preset="card" title="新建离线下载任务" :style="{ width: '520px' }">
      <div class="source-segmented">
        <n-radio-group v-model:value="source" size="medium" class="source-group">
          <n-radio-button value="url">
            <n-space size="small" align="center">
              <n-icon :component="LinkIcon" />
              <span>普通链接</span>
            </n-space>
          </n-radio-button>
          <n-radio-button value="magnet">
            <n-space size="small" align="center">
              <n-icon :component="MagnetIcon" />
              <span>磁力链接</span>
            </n-space>
          </n-radio-button>
          <n-radio-button value="torrent">
            <n-space size="small" align="center">
              <n-icon :component="TorrentIcon" />
              <span>种子文件</span>
            </n-space>
          </n-radio-button>
        </n-radio-group>
      </div>

      <template v-if="source === 'url' || source === 'magnet'">
        <n-input
          v-model:value="url"
          :placeholder="source === 'url' ? '输入或粘贴下载链接，如 https://example.com/file.zip' : '输入或粘贴 magnet:?xt=urn:btih: 磁力链接'"
          class="source-input"
          autofocus
          @keydown.enter="confirmCreate"
        />
        <p class="source-tip">
          {{ source === 'url' ? '支持标准 HTTP / HTTPS 协议直链下载' : '磁力下载依赖可用 Peer 与 Tracker 节点网络质量' }}
        </p>
      </template>

      <template v-else>
        <label class="torrent-picker" :class="{ active: torrentFile }">
          <input
            type="file"
            accept=".torrent"
            class="torrent-input"
            @change="(e: Event) => torrentFile = (e.target as HTMLInputElement).files?.[0] || null"
          />
          <template v-if="torrentFile">
            <div class="torrent-selected">
              <n-icon :component="TorrentIcon" size="32" color="var(--accent)" />
              <span class="torrent-name">{{ torrentFile.name }}</span>
              <span class="torrent-size tabular-nums">{{ formatSize(torrentFile.size) }}</span>
            </div>
          </template>
          <template v-else>
            <n-icon :component="TorrentIcon" size="32" />
            <span class="torrent-hint">点击选择或将 .torrent 种子文件拖拽至此</span>
          </template>
        </label>
      </template>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showNew = false">取消</n-button>
          <n-button type="primary" :loading="creating" @click="confirmCreate">创建下载</n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.downloads-page {
  max-width: 900px;
}
.page-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 18px;
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
.filter-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}
.filter-tabs {
  display: inline-flex;
  align-items: center;
  background: var(--zinc-100);
  padding: 3px;
  border-radius: var(--radius-control);
  gap: 2px;
  border: 1px solid var(--zinc-200);
}
.filter-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  border: none;
  font-size: 13px;
  font-weight: 500;
  color: var(--zinc-600);
  cursor: pointer;
  padding: 5px 12px;
  border-radius: 6px;
  transition: all 0.12s ease;
}
.filter-tab:hover {
  color: var(--zinc-900);
}
.filter-tab.active {
  background: var(--zinc-50);
  color: var(--zinc-900);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
}
.tab-count {
  font-size: 11px;
  color: var(--zinc-400);
  background: var(--zinc-200);
  padding: 1px 5px;
  border-radius: 10px;
  line-height: 1.2;
}
.filter-tab.active .tab-count {
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
}
.empty-box {
  padding: 70px 0;
}
.task-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.task-item {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  padding: 16px 18px;
  border: 1px solid var(--zinc-200);
  border-radius: var(--radius-panel);
  background: var(--zinc-50);
  transition: all 0.15s ease;
}
.task-item:hover {
  background: var(--zinc-100);
  border-color: var(--zinc-300);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.04);
}
.task-icon {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: 10px;
  background: var(--zinc-100);
  color: var(--zinc-500);
  margin-top: 2px;
  border: 1px solid var(--zinc-200);
}
.task-icon.icon-running {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: color-mix(in srgb, var(--accent) 25%, transparent);
  color: var(--accent);
}
.task-icon.icon-done {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: color-mix(in srgb, var(--accent) 25%, transparent);
}
.task-icon.icon-error {
  background: rgba(239, 68, 68, 0.1);
  border-color: rgba(239, 68, 68, 0.2);
}
.task-icon.icon-cookie_expired {
  background: rgba(245, 158, 11, 0.12);
  border-color: rgba(245, 158, 11, 0.25);
}
.task-main {
  flex: 1;
  min-width: 0;
}
.task-header-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.task-name {
  font-size: 14px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.task-source-badge {
  font-size: 10.5px;
  font-weight: 600;
  text-transform: uppercase;
  color: var(--zinc-500);
  background: var(--zinc-200);
  padding: 1px 6px;
  border-radius: 4px;
  letter-spacing: 0.03em;
  flex-shrink: 0;
}
.task-link {
  margin-top: 3px;
  font-size: 12px;
  color: var(--zinc-400);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 580px;
}
.task-sub {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
  font-size: 12px;
  color: var(--zinc-500);
  flex-wrap: wrap;
}
.task-speed-box {
  display: inline-flex;
  align-items: center;
  color: var(--accent);
  font-weight: 500;
}
.task-speed {
  font-weight: 600;
}
.task-eta {
  color: var(--zinc-500);
  font-weight: 400;
  margin-left: 4px;
}
.task-finishing {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--accent);
  font-weight: 500;
}
.task-queue-hint {
  color: var(--zinc-400);
}
.cookie-expired-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-top: 8px;
  padding: 8px 12px;
  background: color-mix(in srgb, #f59e0b 10%, transparent);
  border: 1px solid color-mix(in srgb, #f59e0b 25%, transparent);
  border-radius: var(--radius-control);
  font-size: 12.5px;
  color: #d97706;
}
.cookie-banner-text {
  display: flex;
  align-items: center;
  gap: 6px;
}
.progress-info-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 10px;
  font-size: 12px;
}
.progress-pct {
  font-weight: 600;
  color: var(--accent);
}
.progress-bytes {
  color: var(--zinc-500);
}
.task-progress {
  margin-top: 4px;
}
.task-done-hint {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-top: 8px;
  font-size: 12px;
  color: var(--accent);
  font-weight: 500;
}
.done-size {
  color: var(--zinc-400);
  font-weight: 400;
}
.task-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
  align-items: center;
}
.source-segmented {
  margin-bottom: 16px;
}
.source-group {
  width: 100%;
  display: flex;
}
.source-group :deep(.n-radio-button) {
  flex: 1;
  text-align: center;
}
.source-input {
  margin-bottom: 8px;
}
.source-tip {
  margin: 0;
  font-size: 12px;
  color: var(--zinc-500);
}
.torrent-picker {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 32px 20px;
  border: 2px dashed var(--zinc-300);
  border-radius: var(--radius-panel);
  background: var(--zinc-100);
  color: var(--zinc-500);
  cursor: pointer;
  transition: all 0.15s ease;
}
.torrent-picker:hover,
.torrent-picker.active {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 5%, var(--zinc-100));
  color: var(--accent);
}
.torrent-selected {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}
.torrent-name {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--zinc-900);
}
.torrent-size {
  font-size: 12px;
  color: var(--zinc-500);
}
.torrent-hint {
  font-size: 13px;
}
.torrent-input {
  display: none;
}
</style>
