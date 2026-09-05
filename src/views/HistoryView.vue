<script setup lang="ts">
import { computed, onActivated, ref } from 'vue';
import { useMessage, useDialog } from 'naive-ui';
import { NButton, NIcon, NEmpty, NSpin, NTable, NSpace, NTag, NTooltip, NInput } from 'naive-ui';
import {
  PhClock as ClockIcon, PhTrash as TrashIcon, PhArrowClockwise as RedownloadIcon,
  PhCopy as CopyIcon, PhCheck as CheckIcon, PhLink as LinkIcon,
  PhMagnifyingGlass as SearchIcon, PhCheckCircle as SuccessIcon,
} from '@phosphor-icons/vue';
import { api, formatSize } from '@/api';
import FileIcon from '@/components/FileIcon.vue';
import type { TaskItem } from '@/types';

const message = useMessage();
const dialog = useDialog();

const history = ref<TaskItem[]>([]);
const loading = ref(false);
const searchQuery = ref('');
const copiedId = ref<number | null>(null);

const statusMeta: Record<string, { label: string; type: 'success' | 'error' | 'warning' | 'info' | 'default' }> = {
  done: { label: '已完成', type: 'success' },
  error: { label: '失败', type: 'error' },
  cookie_expired: { label: 'Cookie 失效', type: 'warning' },
  replaced: { label: '已替换', type: 'info' },
};

const doneCount = computed(() => history.value.filter(t => t.status === 'done').length);
const failCount = computed(() => history.value.filter(t => t.status === 'error' || t.status === 'cookie_expired').length);

const filteredHistory = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return history.value;
  return history.value.filter(t => {
    const name = fileName(t).toLowerCase();
    const source = (t.source || '').toLowerCase();
    return name.includes(q) || source.includes(q);
  });
});

async function load() {
  loading.value = true;
  try {
    history.value = await api.history();
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    loading.value = false;
  }
}

onActivated(load);

async function reDownload(t: TaskItem) {
  try {
    if (t.source === 'uc') {
      const uc = (t.metadata?.uc || {}) as Record<string, string>;
      if (!uc.shareLink) return message.error('缺少原始分享链接，无法重新下载');
      const r = await api.ucParse(uc.shareLink);
      const f = r.files.find(x => x.fid === uc.fid);
      if (!f) return message.error('分享中已找不到该文件');
      await api.ucDownload({
        shareId: r.shareId,
        stoken: r.session.stoken,
        fid: f.fid,
        shareFidToken: f.share_fid_token,
        filename: f.name,
        size: f.size,
        ctoken: r.session.ctoken,
        cookies: r.session.cookies,
        shareLink: r.shareLink,
      });
      message.success('已创建重新下载任务');
    } else {
      await api.createTask({ source: t.source, url: t.source_url });
      message.success('已创建重新下载任务');
    }
  } catch (e) {
    message.error((e as Error).message);
  }
}

async function copyLink(t: TaskItem) {
  const link = t.source === 'uc'
    ? (t.metadata?.uc as Record<string, string>)?.shareLink || t.source_url
    : t.source_url;
  try {
    await navigator.clipboard.writeText(link);
    copiedId.value = t.id;
    setTimeout(() => { if (copiedId.value === t.id) copiedId.value = null; }, 1800);
    message.success('链接已复制到剪贴板');
  } catch {
    message.warning('复制失败，请手动复制');
  }
}

async function clearAll() {
  const ok = await dialog.warning({
    title: '清空历史确认',
    content: `确定清空全部 ${history.value.length} 条历史记录吗？此操作不可恢复。`,
    positiveText: '清空',
    negativeText: '取消',
  });
  if (!ok) return;
  try {
    const r = await api.clearHistory();
    history.value = [];
    message.success(`已清空 ${r.deleted} 条记录`);
  } catch (e) {
    message.error((e as Error).message);
  }
}

function fmtTime(iso: string | null) {
  if (!iso) return '—';
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fileName(t: TaskItem): string {
  const uc = (t.metadata?.uc || {}) as Record<string, string>;
  return uc.filename || t.source_url?.split('?')[0]?.split('/').filter(Boolean).pop() || `任务 ${t.id}`;
}
</script>

<template>
  <div class="history-view">
    <div class="page-head">
      <div>
        <h2 class="page-title">历史记录</h2>
        <p class="page-desc">下载完成与失败任务的本地归档，支持一键快速重新下载或复制链接</p>
      </div>
      <div v-if="history.length > 0" class="stats-row">
        <span class="stat-pill">共 <strong class="tabular-nums">{{ history.length }}</strong> 条</span>
        <span v-if="doneCount > 0" class="stat-pill success">完成 <strong class="tabular-nums">{{ doneCount }}</strong></span>
        <span v-if="failCount > 0" class="stat-pill error">异常 <strong class="tabular-nums">{{ failCount }}</strong></span>
      </div>
    </div>

    <div class="panel">
      <div class="panel-toolbar">
        <n-input
          v-model:value="searchQuery"
          placeholder="搜索文件名或来源…"
          clearable
          size="small"
          class="history-search"
        >
          <template #prefix><n-icon :component="SearchIcon" /></template>
        </n-input>
        <n-button
          size="small"
          type="error"
          secondary
          :disabled="!history.length"
          @click="clearAll"
        >
          <template #icon><n-icon :component="TrashIcon" /></template>
          清空历史
        </n-button>
      </div>

      <div v-if="loading" class="loading-box"><n-spin size="small" /></div>
      <n-empty v-else-if="!history.length" description="暂无历史记录" class="empty-box" />
      <n-empty v-else-if="!filteredHistory.length" description="没有找到匹配的历史记录" class="empty-box" />

      <n-table v-else size="small" :bordered="false" class="history-table">
        <thead>
          <tr>
            <th>文件名</th>
            <th style="width: 100px">来源</th>
            <th style="width: 110px">状态</th>
            <th style="width: 150px; text-align: right;">完成/更新时间</th>
            <th style="width: 140px; text-align: right;">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="t in filteredHistory" :key="t.id">
            <td>
              <div class="name-cell">
                <file-icon :name="fileName(t)" :is-dir="false" mime="" :size="18" />
                <span class="name" :title="fileName(t)">{{ fileName(t) }}</span>
              </div>
            </td>
            <td>
              <span class="source-tag">{{ t.source === 'uc' ? 'UC 网盘' : t.source.toUpperCase() }}</span>
            </td>
            <td>
              <n-tag size="small" :type="statusMeta[t.status]?.type || 'default'" :bordered="false">
                {{ statusMeta[t.status]?.label || t.status }}
              </n-tag>
              <div v-if="t.error" class="err muted" :title="t.error">{{ t.error }}</div>
            </td>
            <td class="muted tabular-nums" style="text-align: right;">{{ fmtTime(t.finished_at || t.updated_at) }}</td>
            <td style="text-align: right;">
              <n-space size="small" justify="end" align="center">
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" secondary @click="reDownload(t)">
                      <template #icon><n-icon :component="RedownloadIcon" /></template>
                      重新下载
                    </n-button>
                  </template>
                  再次创建下载任务
                </n-tooltip>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary circle @click="copyLink(t)" aria-label="复制链接">
                      <template #icon>
                        <n-icon :component="copiedId === t.id ? CheckIcon : CopyIcon" :color="copiedId === t.id ? 'var(--accent)' : undefined" />
                      </template>
                    </n-button>
                  </template>
                  {{ copiedId === t.id ? '已复制！' : '复制下载链接' }}
                </n-tooltip>
              </n-space>
            </td>
          </tr>
        </tbody>
      </n-table>
    </div>
  </div>
</template>

<style scoped>
.history-view {
  max-width: 960px;
}
.page-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 18px;
  flex-wrap: wrap;
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
.stats-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.stat-pill {
  font-size: 12px;
  color: var(--zinc-600);
  background: var(--zinc-100);
  border: 1px solid var(--zinc-200);
  padding: 3px 9px;
  border-radius: 12px;
}
.stat-pill strong {
  font-weight: 600;
  color: var(--zinc-900);
}
.stat-pill.success {
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border-color: color-mix(in srgb, var(--accent) 20%, transparent);
  color: var(--accent);
}
.stat-pill.success strong {
  color: var(--accent);
}
.stat-pill.error {
  background: rgba(239, 68, 68, 0.1);
  border-color: rgba(239, 68, 68, 0.2);
  color: #ef4444;
}
.stat-pill.error strong {
  color: #ef4444;
}
.panel {
  background: var(--zinc-50);
  border: 1px solid var(--zinc-200);
  border-radius: var(--radius-panel);
  padding: 18px 20px;
}
.panel-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 14px;
}
.history-search {
  max-width: 320px;
}
.loading-box {
  display: flex;
  justify-content: center;
  padding: 36px 0;
}
.empty-box {
  padding: 50px 0;
}
.history-table :deep(td) {
  padding: 10px 12px;
}
.name-cell {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 380px;
}
.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}
.source-tag {
  font-size: 11.5px;
  font-weight: 500;
  color: var(--zinc-500);
  background: var(--zinc-100);
  padding: 2px 6px;
  border-radius: 4px;
  border: 1px solid var(--zinc-200);
}
.muted {
  color: var(--zinc-500);
  font-size: 12.5px;
}
.err {
  margin-top: 2px;
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11.5px;
  color: #ef4444;
}
</style>
