<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useMessage, useDialog } from 'naive-ui';
import { NButton, NIcon, NEmpty, NSpin, NTable, NSpace, NTag, NTooltip } from 'naive-ui';
import {
  PhClock as ClockIcon, PhTrash as TrashIcon, PhArrowClockwise as RedownloadIcon,
  PhCopy as CopyIcon, PhCheck as CheckIcon, PhLink as LinkIcon,
} from '@phosphor-icons/vue';
import { api, formatSize } from '@/api';
import type { TaskItem } from '@/types';

const message = useMessage();
const dialog = useDialog();

const history = ref<TaskItem[]>([]);
const loading = ref(false);

const statusMeta: Record<string, { label: string; type: 'success' | 'error' | 'warning' | 'info' | 'default' }> = {
  done: { label: '已完成', type: 'success' },
  error: { label: '失败', type: 'error' },
  cookie_expired: { label: 'Cookie 失效', type: 'warning' },
  replaced: { label: '已替换', type: 'info' },
};

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

onMounted(load);

async function reDownload(t: TaskItem) {
  try {
    if (t.source === 'uc') {
      const uc = (t.metadata?.uc || {}) as Record<string, string>;
      if (!uc.shareLink) return message.error('缺少原始分享链接，无法重新下载');
      // 重新解析并下载同名文件
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
    message.success('链接已复制');
  } catch {
    message.warning('复制失败，请手动复制');
  }
}

async function clearAll() {
  const ok = await dialog.warning({
    title: '清空历史',
    content: '确定清空全部历史记录吗？此操作不可恢复。',
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
  return uc.filename || t.source_url?.split('/').pop() || `任务 ${t.id}`;
}
</script>

<template>
  <div class="history-view">
    <div class="panel">
      <div class="panel-header">
        <div class="panel-title">
          <n-icon :component="ClockIcon" size="18" />
          <span>历史记录</span>
        </div>
        <n-button size="small" type="error" secondary :disabled="!history.length" @click="clearAll">
          <template #icon><n-icon><trash-icon /></n-icon></template>
          清空历史
        </n-button>
      </div>

      <div v-if="loading" class="loading-box"><n-spin size="small" /></div>
      <n-empty v-else-if="!history.length" description="暂无历史记录" />

      <n-table v-else size="small" :bordered="false">
        <thead>
          <tr>
            <th>文件名</th>
            <th style="width: 110px">来源</th>
            <th style="width: 100px">状态</th>
            <th style="width: 140px">时间</th>
            <th style="width: 150px">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="t in history" :key="t.id">
            <td>
              <div class="name-cell">
                <span class="name">{{ fileName(t) }}</span>
              </div>
            </td>
            <td class="muted">{{ t.source === 'uc' ? 'UC 网盘' : t.source.toUpperCase() }}</td>
            <td>
              <n-tag size="small" :type="statusMeta[t.status]?.type || 'default'">
                {{ statusMeta[t.status]?.label || t.status }}
              </n-tag>
              <div v-if="t.error" class="err muted">{{ t.error }}</div>
            </td>
            <td class="muted">{{ fmtTime(t.finished_at || t.updated_at) }}</td>
            <td>
              <n-space :size="6">
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" secondary @click="reDownload(t)">
                      <template #icon><n-icon><redownload-icon /></n-icon></template>
                      重新下载
                    </n-button>
                  </template>
                  重新下载该文件
                </n-tooltip>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary @click="copyLink(t)">
                      <template #icon><n-icon><copy-icon /></n-icon></template>
                    </n-button>
                  </template>
                  复制链接
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
  max-width: 980px;
}
.panel {
  background: var(--zinc-50);
  border: 1px solid var(--zinc-200);
  border-radius: var(--radius-panel);
  padding: 20px;
}
.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}
.panel-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 15px;
}
.loading-box {
  display: flex;
  justify-content: center;
  padding: 24px 0;
}
.name-cell {
  max-width: 420px;
}
.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  display: block;
}
.muted {
  color: var(--zinc-500);
  font-size: 12px;
}
.err {
  margin-top: 2px;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
