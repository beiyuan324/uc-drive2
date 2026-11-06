<script setup lang="ts">
import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';
import { useMessage, useDialog, NButton, NIcon, NInput, NAlert, NEmpty, NSpin, NTable,
  NSpace, NBreadcrumb, NBreadcrumbItem, NCheckbox } from 'naive-ui';
import { PhPlay as ParseIcon, PhDownloadSimple as DownloadIcon,
  PhFolderOpen as FolderOpenIcon } from '@phosphor-icons/vue';
import { api, formatSize } from '@/api';
import { useSettingsStore } from '@/stores/settings';
import { driveNames } from '@/types';
import type { DriveFile, DriveParseResult, DriveShare } from '@/types';
import FileIcon from '@/components/FileIcon.vue';

const message = useMessage();
const dialog = useDialog();
const router = useRouter();
const settings = useSettingsStore();
const shareText = ref('');
const parsing = ref(false);
const parsed = ref<DriveParseResult[]>([]);
const activeIdx = ref(-1);
const crumbStack = ref<{ name: string; pdirFid: string | null }[]>([]);
const currentFiles = ref<DriveFile[]>([]);
const folderLoading = ref(false);
const pendingFids = ref(new Set<string>());
const batchBusy = ref<'' | 'selected' | 'all'>('');
const selectedFids = ref(new Set<string>());
const hasResult = computed(() => parsed.value.length > 0);
const active = computed(() => parsed.value[activeIdx.value]);
const currentFileItems = computed(() => currentFiles.value.filter((f) => f.file));
const isAllSelected = computed(() => currentFileItems.value.length > 0
  && currentFileItems.value.every((f) => selectedFids.value.has(f.fid)));
const isIndeterminate = computed(() => {
  const count = currentFileItems.value.filter((f) => selectedFids.value.has(f.fid)).length;
  return count > 0 && count < currentFileItems.value.length;
});

function pendingKey(share: DriveShare | undefined, fid: string) {
  return `${share?.platform}:${share?.shareId}:${fid}`;
}

function setPending(share: DriveShare, files: DriveFile[], pending: boolean) {
  const next = new Set(pendingFids.value);
  for (const file of files) {
    const key = pendingKey(share, file.fid);
    if (pending) next.add(key);
    else next.delete(key);
  }
  pendingFids.value = next;
}

function toggleSelect(fid: string) {
  const next = new Set(selectedFids.value);
  if (next.has(fid)) next.delete(fid);
  else next.add(fid);
  selectedFids.value = next;
}

function toggleSelectAll(checked: boolean) {
  selectedFids.value = checked ? new Set(currentFileItems.value.map((f) => f.fid)) : new Set();
}

async function parseAll() {
  const links = shareText.value.split('\n').map((s) => s.trim()).filter(Boolean);
  if (!links.length) return message.warning('请输入分享链接');
  parsing.value = true;
  parsed.value = [];
  activeIdx.value = -1;
  currentFiles.value = [];
  crumbStack.value = [];
  selectedFids.value = new Set();
  try {
    for (const link of links) {
      try {
        parsed.value.push(await api.parseShare(link));
      } catch (e) {
        message.error((e as Error).message);
      }
    }
    if (parsed.value.length) {
      message.success(`已解析 ${parsed.value.length}/${links.length} 个分享`);
      await switchLink(0);
    }
  } finally {
    parsing.value = false;
  }
}

async function switchLink(idx: number) {
  activeIdx.value = idx;
  crumbStack.value = [{ name: '分享根目录', pdirFid: null }];
  selectedFids.value = new Set();
  currentFiles.value = parsed.value[idx].files;
}

async function loadFolder(share: DriveParseResult, parent: string | null) {
  folderLoading.value = true;
  try {
    const result = await api.listShareFolder(share, parent);
    if (result.session) share.session = result.session;
    if (active.value === share) {
      currentFiles.value = result.files;
      selectedFids.value = new Set();
    }
    return true;
  } catch (e) {
    message.error((e as Error).message);
    return false;
  } finally {
    folderLoading.value = false;
  }
}

async function enterFolder(file: DriveFile) {
  const share = active.value;
  if (share && await loadFolder(share, file.fid) && active.value === share) {
    crumbStack.value.push({ name: file.name, pdirFid: file.fid });
  }
}

async function goCrumb(idx: number) {
  const share = active.value;
  const target = crumbStack.value[idx];
  if (share && await loadFolder(share, target.pdirFid) && active.value === share) {
    crumbStack.value = crumbStack.value.slice(0, idx + 1);
  }
}

function handleDownloadError(e: unknown, share: DriveShare, name = '') {
  const err = e as Error & { kind?: string };
  if (err.kind === 'cookie_expired' || err.kind === 'credential_expired') {
    message.warning(`${driveNames[share.platform]}凭据或校验令牌已失效，请到设置中更新后重新解析`, { duration: 8000 });
    return true;
  }
  message.error(`${name ? name + '：' : ''}${err.message || '下载失败'}`);
  return false;
}

async function downloadOne(file: DriveFile) {
  const share = active.value;
  if (!share || pendingFids.value.has(pendingKey(share, file.fid))) return;
  setPending(share, [file], true);
  try {
    await api.downloadShare(share, file, settings.downloadConfig.ucConnections);
    message.success(`已创建任务：${file.name}`);
  } catch (e) {
    handleDownloadError(e, share, file.name);
  } finally {
    setPending(share, [file], false);
  }
}

async function flattenFiles(share: DriveParseResult, parent: string | null, visited = new Set<string>()): Promise<DriveFile[]> {
  const key = parent ?? '';
  if (visited.has(key)) throw new Error('分享目录出现循环，已停止展开');
  visited.add(key);
  const result = await api.listShareFolder(share, parent);
  if (result.session) share.session = result.session;
  const files: DriveFile[] = [];
  for (const file of result.files) {
    if (file.file) files.push(file);
    else files.push(...await flattenFiles(share, file.fid, visited));
  }
  return files;
}

async function downloadBatch(kind: 'selected' | 'all') {
  const share = active.value;
  if (!share || batchBusy.value) return;
  batchBusy.value = kind;
  let targets: DriveFile[] = [];
  try {
    if (kind === 'all') {
      const confirmed = await new Promise<boolean>((resolve) => {
        dialog.warning({
          title: '全部下载确认',
          content: '将为当前目录及子文件夹中的文件创建 Gopeed 下载任务。',
          positiveText: '开始下载', negativeText: '取消',
          onPositiveClick: () => resolve(true), onNegativeClick: () => resolve(false),
          onClose: () => resolve(false), onMaskClick: () => resolve(false),
        });
      });
      if (!confirmed) return;
      const parent = crumbStack.value.at(-1)?.pdirFid ?? null;
      targets = await flattenFiles(share, parent);
    } else {
      targets = currentFiles.value.filter((f) => f.file && selectedFids.value.has(f.fid));
    }
    targets = targets.filter((f) => !pendingFids.value.has(pendingKey(share, f.fid)));
    if (!targets.length) return message.info('没有可创建的下载任务');
    setPending(share, targets, true);
    let created = 0;
    for (const file of targets) {
      try {
        await api.downloadShare(share, file, settings.downloadConfig.ucConnections);
        created++;
      } catch (e) {
        if (handleDownloadError(e, share, file.name)) break;
      }
    }
    message.info(`已创建 ${created}/${targets.length} 个下载任务`);
    if (active.value === share) selectedFids.value = new Set();
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    setPending(share, targets, false);
    batchBusy.value = '';
  }
}

const downloadSelected = () => downloadBatch('selected');
const downloadAll = () => downloadBatch('all');

function clearAll() {
  shareText.value = '';
  parsed.value = [];
  activeIdx.value = -1;
  currentFiles.value = [];
  crumbStack.value = [];
  selectedFids.value = new Set();
}
</script>

<template>
  <div class="parse-view">
    <div class="page-head">
      <h1 class="page-title">链接解析</h1>
    </div>

    <div class="panel">
      <n-input
        v-model:value="shareText"
        type="textarea"
        :rows="3"
        placeholder="UC / 夸克 / 百度 / 迅雷分享链接，每行一个"
        :input-props="{ 'aria-label': '分享链接和提取码' }"
      />
      <n-space class="parse-actions" :size="10">
        <n-button type="primary" :loading="parsing" @click="parseAll">
          <template #icon
            ><n-icon><parse-icon /></n-icon
          ></template>
          开始解析
        </n-button>
        <n-button secondary @click="clearAll">清空</n-button>
      </n-space>
      <n-alert
        v-if="!active?.cookieUsed && parsed.length"
        type="warning"
        :show-icon="false"
        class="cookie-tip"
      >
        <div class="cookie-tip-content">
          <span>此分享未使用登录凭据</span>
          <n-button text type="warning" @click="router.push('/settings')"
            >设置凭据</n-button
          >
        </div>
      </n-alert>
    </div>

    <n-alert v-if="active?.platform === 'xunlei'" type="info" :show-icon="false">迅雷分享可能先转存到你的云盘，转存文件会保留。</n-alert>

    <div v-if="parsed.length > 1" class="links-bar" role="group" aria-label="切换分享">
      <button
        v-for="(p, i) in parsed"
        :key="p.shareId + i"
        class="link-tab"
        :class="{ active: i === activeIdx }"
        :aria-pressed="i === activeIdx"
        :title="p.shareLink"
        @click="switchLink(i)"
      >
        {{ driveNames[p.platform] }} · {{ i + 1 }}
      </button>
    </div>

    <div v-if="hasResult" class="panel">
      <div class="results-header">
        <n-breadcrumb>
          <n-breadcrumb-item v-for="(c, i) in crumbStack" :key="i" @click="goCrumb(i)">
            <a v-if="i < crumbStack.length - 1" class="crumb-link">{{ c.name }}</a>
            <span v-else class="crumb-current">{{ c.name }}</span>
          </n-breadcrumb-item>
        </n-breadcrumb>
        <n-space size="small" align="center">
          <n-button
            v-if="selectedFids.size > 0"
            size="small"
            type="primary"
            :loading="batchBusy === 'selected'"
            @click="downloadSelected"
          >
            <template #icon
              ><n-icon><download-icon /></n-icon
            ></template>
            下载所选 ({{ selectedFids.size }})
          </n-button>
          <n-button size="small" secondary :loading="batchBusy === 'all'" @click="downloadAll">
            <template #icon
              ><n-icon><download-icon /></n-icon
            ></template>
            全部下载
          </n-button>
        </n-space>
      </div>

      <div v-if="folderLoading" class="loading-box">
        <n-spin size="small" />
      </div>
      <n-empty v-else-if="currentFiles.length === 0" description="此目录为空" class="empty-box" />
      <n-table v-else size="small" :bordered="false" class="uc-table">
        <thead>
          <tr>
            <th style="width: 44px; text-align: center">
              <n-checkbox
                aria-label="选择全部文件"
                :checked="isAllSelected"
                :indeterminate="isIndeterminate"
                :disabled="currentFileItems.length === 0"
                @update:checked="toggleSelectAll"
              />
            </th>
            <th style="width: 52%">名称</th>
            <th style="width: 18%; text-align: right">大小</th>
            <th style="width: 24%; text-align: right">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="f in currentFiles"
            :key="f.fid"
            :class="{ 'row-checked': f.file && selectedFids.has(f.fid) }"
          >
            <td style="text-align: center">
              <n-checkbox
                v-if="f.file"
                :aria-label="`选择 ${f.name}`"
                :checked="selectedFids.has(f.fid)"
                @update:checked="toggleSelect(f.fid)"
              />
            </td>
            <td>
              <button
                class="file-cell"
                :aria-pressed="f.file ? selectedFids.has(f.fid) : undefined"
                @click="f.file ? toggleSelect(f.fid) : enterFolder(f)"
              >
                <file-icon
                  :name="f.name"
                  :is-dir="!f.file"
                  :mime="f.file ? '' : 'folder'"
                  :size="19"
                />
                <span class="file-name" :title="f.name">{{ f.name }}</span>
              </button>
            </td>
            <td class="muted tabular-nums" style="text-align: right">
              {{ f.file ? formatSize(f.size) : '—' }}
            </td>
            <td style="text-align: right">
              <n-space size="small" justify="end" align="center">
                <n-button
                  v-if="f.file"
                  size="tiny"
                  type="primary"
                  secondary
                  :loading="pendingFids.has(pendingKey(active, f.fid))"
                  @click="downloadOne(f)"
                >
                  <template #icon><n-icon :component="DownloadIcon" /></template>
                  下载
                </n-button>
                <n-button v-else size="tiny" secondary @click="enterFolder(f)">
                  <template #icon><n-icon :component="FolderOpenIcon" /></template>
                  打开
                </n-button>
              </n-space>
            </td>
          </tr>
        </tbody>
      </n-table>
    </div>
  </div>
</template>

<style scoped>
.parse-view {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.page-head {
  margin-bottom: 0;
}
.panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
  background: transparent;
}
.uc-table {
  background: transparent !important;
}
.uc-table :deep(th) {
  background: transparent !important;
  border-bottom: 1px solid var(--line) !important;
  color: var(--zinc-500) !important;
  font-weight: 550 !important;
  padding: 10px 12px !important;
}
.uc-table :deep(tr) {
  transition: background 0.15s ease;
}
.uc-table :deep(tr:not(.row-checked):hover td) {
  background: var(--hover) !important;
}
.parse-actions {
  margin-top: 2px;
}
.cookie-tip {
  font-size: 13px;
}
.cookie-tip-content {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.links-bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}
.link-tab {
  padding: 6px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--zinc-600);
  font-size: 13px;
  cursor: pointer;
}
.link-tab:hover {
  background: var(--hover);
}
.link-tab.active {
  background: var(--selection);
  color: var(--accent);
}
.results-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}
.crumb-link {
  color: var(--accent);
  cursor: pointer;
}
.crumb-current {
  color: var(--zinc-500);
  font-weight: 500;
}
.loading-box {
  display: flex;
  justify-content: center;
  padding: 30px 0;
}
.file-cell {
  width: 100%;
  border: 0;
  padding: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.file-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}
.muted {
  color: var(--zinc-500);
  font-size: 13px;
}
.uc-table :deep(tr.row-checked td) {
  background: color-mix(in srgb, var(--accent) 6%, var(--zinc-50));
}
.uc-table :deep(td) {
  padding: 8px 12px;
}
@media (max-width: 680px) {
  .uc-table :deep(th:first-child) {
    width: 32px !important;
  }
  .uc-table :deep(th:nth-child(2)) {
    width: auto !important;
  }
  .uc-table :deep(th:nth-child(3)),
  .uc-table :deep(td:nth-child(3)) {
    display: none;
  }
  .uc-table :deep(th:last-child) {
    width: 80px !important;
  }
  .uc-table :deep(td) {
    padding: 8px 4px;
  }
}
</style>
