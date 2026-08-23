<script setup lang="ts">
import { onMounted, onUnmounted, ref, computed, watch } from 'vue';
import { useMessage, useDialog } from 'naive-ui';
import {
  NButton, NIcon, NInput, NModal, NProgress, NTag, NEmpty, NSpin, NRadioGroup, NRadioButton, NSpace,
} from 'naive-ui';
import {
  PhLink as LinkIcon, PhMagnet as MagnetIcon, PhMagnetStraight as TorrentIcon,
  PhPlay as PlayIcon, PhPause as PauseIcon, PhTrash as TrashIcon, PhPlus as PlusIcon,
  PhCheckCircle as CheckIcon, PhWarningCircle as WarnIcon, PhClockClockwise as ClockIcon,
  PhKey as KeyIcon,
} from '@phosphor-icons/vue';
import { useTasksStore } from '@/stores/tasks';
import { useSettingsStore } from '@/stores/settings';
import { api, formatSize, formatSpeed } from '@/api';
import type { TaskItem, TaskSource } from '@/types';

const tasks = useTasksStore();
const settings = useSettingsStore();
const message = useMessage();
const dialog = useDialog();

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
    // UC Cookie 失效：前台主动提醒 + 引导去设置（不只靠任务列表状态）
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
  if (!file) return message.warning('请选择 torrent 文件');
  creating.value = true;
  try {
    if (source.value === 'torrent') {
      // 临时上传 torrent（不入文件树，任务创建后由后端清理）
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
  running: { label: '下载中', type: 'info', icon: LinkIcon },
  paused: { label: '已暂停', type: 'warning', icon: PauseIcon },
  done: { label: '已完成', type: 'success', icon: CheckIcon },
  error: { label: '失败', type: 'error', icon: WarnIcon },
  cookie_expired: { label: 'Cookie 失效', type: 'warning', icon: KeyIcon },
};

onMounted(() => {
  tasks.startPolling(2000);
  requestNotifyPermission();
});
onUnmounted(() => tasks.stopPolling());
watch(() => tasks.tasks, checkCompleted, { deep: true });
</script>

<template>
  <div class="downloads-page">
    <div class="page-head">
      <h2 class="page-title">离线下载</h2>
      <n-button type="primary" @click="showNew = true">
        <template #icon><n-icon :component="PlusIcon" /></template>
        新建任务
      </n-button>
    </div>

    <n-spin :show="tasks.loading">
      <div v-if="tasks.tasks.length === 0" class="empty-box">
        <n-empty description="暂无下载任务" />
      </div>

      <div v-else class="task-list">
        <div v-for="t in tasks.tasks" :key="t.id" class="task-item">
          <div class="task-icon">
            <n-icon :component="statusMeta[t.status]?.icon || ClockIcon" size="22" :color="t.status === 'done' ? 'var(--accent)' : undefined" />
          </div>
          <div class="task-main">
            <div class="task-name" :title="t.source_url">{{ t.source_url || `任务 #${t.id}` }}</div>
            <div class="task-sub">
              <n-tag size="small" :type="statusMeta[t.status]?.type" :bordered="false">
                {{ statusMeta[t.status]?.label || t.status }}
              </n-tag>
              <span v-if="t.status === 'running'" class="task-speed">{{ formatSpeed(t.speed) }}</span>
              <span v-else-if="t.status === 'queued'" class="task-queue-hint">
                排队中，同时最多 {{ settings.downloadConfig.maxRunning }} 个任务
              </span>
              <span v-if="t.status === 'cookie_expired'" class="task-cookie-hint">
                <n-icon :component="KeyIcon" size="13" />
                到「设置」更新 UC Cookie 后可重新下载
              </span>
              <span class="task-source">{{ t.source === 'torrent' ? 'torrent' : t.source }}</span>
            </div>
            <n-progress
              v-if="t.status !== 'done'"
              :percentage="t.progress"
              :show-indicator="false"
              :height="6"
              :border-radius="3"
              class="task-progress"
            />
            <div v-else class="task-done-hint">已登记到网盘根目录</div>
          </div>
          <div class="task-actions">
            <n-button
              size="small"
              quaternary
              :disabled="t.status === 'done' || t.status === 'error'"
              @click="togglePause(t)"
            >
              <template #icon>
                <n-icon :component="t.status === 'paused' ? PlayIcon : PauseIcon" />
              </template>
              {{ t.status === 'paused' ? '继续' : '暂停' }}
            </n-button>
            <n-button size="small" quaternary type="error" @click="confirmDelete(t)">
              <template #icon><n-icon :component="TrashIcon" /></template>
              删除
            </n-button>
          </div>
        </div>
      </div>
    </n-spin>

    <!-- 新建任务 -->
    <n-modal v-model:show="showNew" preset="card" title="新建下载任务" :style="{ width: '520px' }">
      <n-radio-group v-model:value="source" class="source-group">
        <n-radio-button value="url">链接</n-radio-button>
        <n-radio-button value="magnet">磁力</n-radio-button>
        <n-radio-button value="torrent">torrent</n-radio-button>
      </n-radio-group>

      <template v-if="source === 'url' || source === 'magnet'">
        <n-input
          v-model:value="url"
          :placeholder="source === 'url' ? 'https://example.com/file.zip' : 'magnet:?xt=urn:btih:…'"
          class="source-input"
          @keydown.enter="confirmCreate"
        />
        <p class="source-tip">支持 HTTP / HTTPS 链接{{ source === 'magnet' ? '，BT 任务需要网络可达的 Tracker' : '' }}</p>
      </template>

      <template v-else>
        <label class="torrent-picker" :class="{ active: torrentFile }">
          <input type="file" accept=".torrent" class="torrent-input" @change="(e: Event) => torrentFile = (e.target as HTMLInputElement).files?.[0] || null" />
          <template v-if="torrentFile">
            <n-icon :component="TorrentIcon" size="28" />
            <span>{{ torrentFile.name }}</span>
          </template>
          <template v-else>
            <n-icon :component="TorrentIcon" size="28" />
            <span>点击选择 .torrent 文件</span>
          </template>
        </label>
      </template>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showNew = false">取消</n-button>
          <n-button type="primary" :loading="creating" @click="confirmCreate">创建</n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.downloads-page {
  max-width: 860px;
}
.page-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 20px;
}
.page-title {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.empty-box {
  padding: 60px 0;
}
.task-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.task-item {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
  border: 1px solid var(--zinc-200);
  border-radius: var(--radius-panel);
  background: var(--zinc-50);
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
}
.task-main {
  flex: 1;
  min-width: 0;
}
.task-name {
  font-size: 13.5px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.task-sub {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 4px;
  font-size: 12px;
  color: var(--zinc-500);
}
.task-speed {
  color: var(--accent);
  font-weight: 500;
}
.task-queue-hint {
  color: var(--zinc-400);
}
.task-cookie-hint {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: #d97706;
  font-weight: 500;
}
.task-source {
  text-transform: uppercase;
  font-size: 11px;
  letter-spacing: 0.04em;
}
.task-progress {
  margin-top: 8px;
}
.task-done-hint {
  margin-top: 6px;
  font-size: 12px;
  color: var(--accent);
}
.task-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}
.source-group {
  margin-bottom: 14px;
}
.source-input {
  margin-bottom: 6px;
}
.source-tip {
  margin: 0;
  font-size: 12px;
  color: var(--zinc-500);
}
.torrent-picker {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 28px;
  border: 1.5px dashed var(--zinc-200);
  border-radius: var(--radius-panel);
  color: var(--zinc-500);
  cursor: pointer;
  transition: border-color 0.12s ease, color 0.12s ease;
}
.torrent-picker:hover,
.torrent-picker.active {
  border-color: var(--accent);
  color: var(--accent);
}
.torrent-input {
  display: none;
}
</style>
