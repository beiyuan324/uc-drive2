<script setup lang="ts">
import { computed, onActivated, ref } from 'vue';
import { useMessage, useDialog } from 'naive-ui';
import { NButton, NIcon, NEmpty, NSpin, NTable, NSpace, NTag, NTooltip, NInput } from 'naive-ui';
import {
  PhTrash as TrashIcon,
  PhArrowClockwise as RedownloadIcon,
  PhCopy as CopyIcon,
  PhCheck as CheckIcon,
  PhMagnifyingGlass as SearchIcon,
} from '@phosphor-icons/vue';
import { api } from '@/api';
import FileIcon from '@/components/FileIcon.vue';
import type { DrivePlatform, TaskItem } from '@/types';
import { driveNames } from '@/types';

const message = useMessage();
const dialog = useDialog();

const history = ref<TaskItem[]>([]);
const loading = ref(false);
const searchQuery = ref('');
const copiedId = ref<number | null>(null);

const statusMeta: Record<
  string,
  { label: string; type: 'success' | 'error' | 'warning' | 'info' | 'default' }
> = {
  done: { label: '已完成', type: 'success' },
  error: { label: '失败', type: 'error' },
  cookie_expired: { label: 'Cookie 失效', type: 'warning' },
  replaced: { label: '已替换', type: 'info' },
};

const filteredHistory = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return history.value;
  return history.value.filter((t) => {
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
    if (t.source === 'quark' || t.source === 'baidu' || t.source === 'xunlei') {
      await api.redownloadShare(t.id);
      message.success('已重新解析官方直链并创建任务');
    } else if (t.source === 'uc') {
      const uc = (t.metadata?.uc || {}) as Record<string, string>;
      if (!uc.shareLink) return message.error('缺少原始分享链接，无法重新下载');
      const r = await api.ucParse(uc.shareLink);
      const f = r.files.find((x) => x.fid === uc.fid);
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
  const link =
    (t.metadata?.cloud as Record<string, string> | undefined)?.shareLink
      || (t.metadata?.uc as Record<string, string> | undefined)?.shareLink
      || t.source_url;
  try {
    await navigator.clipboard.writeText(link);
    copiedId.value = t.id;
    setTimeout(() => {
      if (copiedId.value === t.id) copiedId.value = null;
    }, 1800);
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
  const uc = (t.metadata?.uc || t.metadata?.cloud || {}) as Record<string, string>;
  return (
    uc.filename || t.source_url?.split('?')[0]?.split('/').filter(Boolean).pop() || `任务 ${t.id}`
  );
}
</script>

<template>
  <div class="history-view">
    <div class="page-head">
      <h1 class="page-title">历史记录</h1>
    </div>

    <div class="panel">
      <div class="panel-toolbar">
        <n-input
          v-model:value="searchQuery"
          placeholder="搜索文件名或来源"
          :input-props="{ 'aria-label': '搜索历史记录' }"
          clearable
          size="small"
          class="history-search"
        >
          <template #prefix><n-icon :component="SearchIcon" /></template>
        </n-input>
        <n-button size="small" type="error" secondary :disabled="!history.length" @click="clearAll">
          <template #icon><n-icon :component="TrashIcon" /></template>
          清空历史
        </n-button>
      </div>

      <div v-if="loading" class="loading-box"><n-spin size="small" /></div>
      <n-empty v-else-if="!history.length" description="暂无历史记录" class="empty-box" />
      <n-empty
        v-else-if="!filteredHistory.length"
        description="没有找到匹配的历史记录"
        class="empty-box"
      />

      <n-table v-else size="small" :bordered="false" class="history-table">
        <thead>
          <tr>
            <th>文件名</th>
            <th style="width: 100px">来源</th>
            <th style="width: 110px">状态</th>
            <th style="width: 150px; text-align: right">时间</th>
            <th style="width: 140px; text-align: right">操作</th>
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
              <span class="source-tag">{{
                driveNames[t.source as DrivePlatform] || t.source.toUpperCase()
              }}</span>
            </td>
            <td>
              <n-tag size="small" :type="statusMeta[t.status]?.type || 'default'" :bordered="false">
                {{ statusMeta[t.status]?.label || t.status }}
              </n-tag>
              <div v-if="t.error" class="err muted" :title="t.error">{{ t.error }}</div>
            </td>
            <td class="muted tabular-nums" style="text-align: right">
              {{ fmtTime(t.finished_at || t.updated_at) }}
            </td>
            <td style="text-align: right">
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
                    <n-button
                      size="tiny"
                      quaternary
                      circle
                      @click="copyLink(t)"
                      aria-label="复制链接"
                    >
                      <template #icon>
                        <n-icon
                          :component="copiedId === t.id ? CheckIcon : CopyIcon"
                          :color="copiedId === t.id ? 'var(--accent)' : undefined"
                        />
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
.panel-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
}
.history-table {
  background: transparent !important;
}
.history-table :deep(th) {
  background: transparent !important;
  border-bottom: 1px solid var(--line) !important;
  color: var(--zinc-500) !important;
  font-weight: 550 !important;
  padding: 10px 12px !important;
}
.history-table :deep(tr) {
  transition: background 0.15s ease;
}
.history-table :deep(tr:hover td) {
  background: var(--hover) !important;
}
.history-search {
  max-width: 320px;
}
.loading-box {
  display: flex;
  justify-content: center;
  padding: 36px 0;
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
@media (max-width: 680px) {
  .history-table :deep(th:nth-child(2)),
  .history-table :deep(td:nth-child(2)),
  .history-table :deep(th:nth-child(4)),
  .history-table :deep(td:nth-child(4)) {
    display: none;
  }
  .history-table :deep(th:nth-child(3)) {
    width: 76px !important;
  }
  .history-table :deep(th:last-child) {
    width: 100px !important;
  }
  .history-table :deep(td) {
    padding-left: 4px;
    padding-right: 4px;
  }
  .history-table :deep(.n-space) {
    gap: 4px !important;
  }
  .history-table :deep(.n-button) {
    font-size: 11px;
  }
  .history-table :deep(.n-button:not([aria-label]) .n-button__icon) {
    display: none;
  }
}
</style>
