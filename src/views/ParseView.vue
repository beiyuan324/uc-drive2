<script setup lang="ts">
import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';
import { useMessage, useDialog } from 'naive-ui';
import {
  NButton, NIcon, NInput, NAlert, NEmpty, NSpin, NTable, NSpace,
  NBreadcrumb, NBreadcrumbItem, NTooltip, NTag,
} from 'naive-ui';
import {
  PhLink as LinkIcon, PhPlay as ParseIcon, PhDownloadSimple as DownloadIcon,
  PhFolder as FolderGlyph, PhFile as FileGlyph, PhArrowClockwise as RefreshIcon,
  PhInfo as InfoIcon, PhArrowsClockwise as RetryIcon,
} from '@phosphor-icons/vue';
import { api, formatSize } from '@/api';
import { useSettingsStore } from '@/stores/settings';
import type { UcFile, UcSession } from '@/types';

const message = useMessage();
const dialog = useDialog();
const router = useRouter();
const settings = useSettingsStore();

const shareText = ref('');
const parsing = ref(false);
const parsed = ref<{
  shareId: string;
  shareLink: string;
  session: UcSession;
  cookieUsed: boolean;
}[]>([]);

/** 当前正在浏览的链接索引 + 目录栈 */
const activeIdx = ref(-1);
const crumbStack = ref<{ name: string; pdirFid: string | null }[]>([]);
const currentFiles = ref<UcFile[]>([]);
const folderLoading = ref(false);
const downloading = ref(false);

const hasResult = computed(() => parsed.value.length > 0);
const active = computed(() => parsed.value[activeIdx.value]);

async function parseAll() {
  const links = shareText.value.split('\n').map(s => s.trim()).filter(Boolean);
  if (!links.length) return message.warning('请输入分享链接');
  parsing.value = true;
  parsed.value = [];
  activeIdx.value = -1;
  currentFiles.value = [];
  crumbStack.value = [];
  try {
    for (const link of links) {
      const r = await api.ucParse(link);
      parsed.value.push({ shareId: r.shareId, shareLink: r.shareLink, session: r.session, cookieUsed: r.cookieUsed });
    }
    message.success(`解析成功：${parsed.value.length} 个分享链接`);
    // 进入第一个链接的根目录
    activeIdx.value = 0;
    currentFiles.value = parsed.value[0] ? await enterRoot(parsed.value[0]) : [];
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    parsing.value = false;
  }
}

async function enterRoot(p: { shareId: string; session: UcSession }) {
  crumbStack.value = [{ name: '分享根目录', pdirFid: null }];
  return loadFolder(p, null);
}

async function switchLink(idx: number) {
  if (idx === activeIdx.value) return;
  activeIdx.value = idx;
  const p = parsed.value[idx];
  folderLoading.value = true;
  try {
    currentFiles.value = await enterRoot(p);
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    folderLoading.value = false;
  }
}

async function loadFolder(p: { shareId: string; session: UcSession }, pdirFid: string | null) {
  folderLoading.value = true;
  try {
    const r = await api.ucListFolder(p.shareId, pdirFid, p.session);
    currentFiles.value = r.files;
    return r.files;
  } catch (e) {
    message.error((e as Error).message);
    return [];
  } finally {
    folderLoading.value = false;
  }
}

async function enterFolder(f: UcFile) {
  if (!active.value) return;
  crumbStack.value.push({ name: f.name, pdirFid: f.fid });
  await loadFolder(active.value, f.fid);
}

function goCrumb(idx: number) {
  if (!active.value) return;
  const target = crumbStack.value[idx];
  crumbStack.value = crumbStack.value.slice(0, idx + 1);
  loadFolder(active.value, target.pdirFid);
}

/** 下载创建失败：Cookie 失效要给出明确引导（否则用户只看到笼统「下载失败」） */
function handleDownloadError(e: unknown, name = '') {
  const err = e as Error & { kind?: string };
  if (err.kind === 'cookie_expired') {
    message.warning(
      `${name ? `「${name}」` : ''}UC Cookie 已失效，请到设置中更新后重新下载`,
      { duration: 8000 },
    );
    setTimeout(() => router.push('/settings'), 500);
    return;
  }
  message.error(err.message || '下载失败');
}

async function downloadOne(f: UcFile) {
  if (!active.value) return;
  downloading.value = true;
  try {
    await api.ucDownload({
      shareId: active.value.shareId,
      stoken: active.value.session.stoken,
      fid: f.fid,
      shareFidToken: f.share_fid_token,
      filename: f.name,
      size: f.size,
      ctoken: active.value.session.ctoken,
      cookies: active.value.session.cookies,
      shareLink: active.value.shareLink,
      connections: settings.downloadConfig.ucConnections,
    });
    message.success(`已创建任务：${f.name}`);
  } catch (e) {
    handleDownloadError(e, f.name);
  } finally {
    downloading.value = false;
  }
}

/** 递归展开当前目录全部文件（含子文件夹） */
async function flattenFiles(p: { shareId: string; session: UcSession }, pdirFid: string | null): Promise<UcFile[]> {
  const r = await api.ucListFolder(p.shareId, pdirFid, p.session);
  const out: UcFile[] = [];
  for (const f of r.files) {
    if (f.file) out.push(f);
    else out.push(...await flattenFiles(p, f.fid));
  }
  return out;
}

async function downloadAll() {
  if (!active.value) return;
  const ok = await dialog.warning({
    title: '全部下载',
    content: '将下载当前目录（含子文件夹）中的所有文件，共需创建多个任务。继续？',
    positiveText: '继续',
    negativeText: '取消',
  });
  if (!ok) return;
  downloading.value = true;
  try {
    const all = await flattenFiles(active.value, crumbStack.value[crumbStack.value.length - 1]?.pdirFid ?? null);
    if (!all.length) return message.info('当前目录没有文件');
    let created = 0;
    for (const f of all) {
      try {
        await api.ucDownload({
          shareId: active.value.shareId,
          stoken: active.value.session.stoken,
          fid: f.fid,
          shareFidToken: f.share_fid_token,
          filename: f.name,
          size: f.size,
          ctoken: active.value.session.ctoken,
          cookies: active.value.session.cookies,
          shareLink: active.value.shareLink,
          connections: settings.downloadConfig.ucConnections,
        });
        created += 1;
      } catch (e) {
        handleDownloadError(e, f.name);
      }
    }
    message.success(`已创建 ${created}/${all.length} 个下载任务`);
  } finally {
    downloading.value = false;
  }
}

function clearAll() {
  shareText.value = '';
  parsed.value = [];
  activeIdx.value = -1;
  currentFiles.value = [];
  crumbStack.value = [];
}
</script>

<template>
  <div class="parse-view">
    <div class="panel">
      <div class="panel-title">
        <n-icon :component="LinkIcon" size="18" />
        <span>UC 网盘解析</span>
      </div>
      <n-input
        v-model:value="shareText"
        type="textarea"
        :rows="3"
        placeholder="粘贴 UC 网盘分享链接，每行一个，支持批量解析&#10;例：https://drive.uc.cn/s/xxxxxxxx?public=1"
      />
      <n-space class="parse-actions" :size="10">
        <n-button type="primary" :loading="parsing" @click="parseAll">
          <template #icon><n-icon><parse-icon /></n-icon></template>
          开始解析
        </n-button>
        <n-button secondary @click="clearAll">清空</n-button>
      </n-space>
      <n-alert v-if="!active?.cookieUsed && parsed.length" type="warning" :show-icon="false" class="cookie-tip">
        部分分享需要有效 Cookie 才能解析。可到「设置 → UC 网盘 Cookie」中配置。
      </n-alert>
    </div>

    <div v-if="parsed.length > 1" class="links-bar">
      <span class="links-bar-label">已解析</span>
      <n-tag
        v-for="(p, i) in parsed"
        :key="p.shareId + i"
        :type="i === activeIdx ? 'primary' : 'default'"
        size="small"
        class="link-tag"
        :title="p.shareLink"
        @click="switchLink(i)"
      >
        分享 {{ i + 1 }}
      </n-tag>
    </div>

    <div v-if="hasResult" class="panel">
      <div class="results-header">
        <n-breadcrumb>
          <n-breadcrumb-item
            v-for="(c, i) in crumbStack"
            :key="i"
            @click="goCrumb(i)"
          >
            <a v-if="i < crumbStack.length - 1" class="crumb-link">{{ c.name }}</a>
            <span v-else class="crumb-current">{{ c.name }}</span>
          </n-breadcrumb-item>
        </n-breadcrumb>
        <n-button size="small" type="primary" secondary :loading="downloading" @click="downloadAll">
          <template #icon><n-icon><download-icon /></n-icon></template>
          全部下载
        </n-button>
      </div>

      <div v-if="folderLoading" class="loading-box">
        <n-spin size="small" />
      </div>
      <n-empty v-else-if="currentFiles.length === 0" description="此目录为空" />
      <n-table v-else size="small" :bordered="false" class="uc-table">
        <thead>
          <tr>
            <th style="width: 48%">名称</th>
            <th style="width: 18%">大小</th>
            <th style="width: 34%">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="f in currentFiles" :key="f.fid">
            <td>
              <div class="file-cell" @click="f.file ? undefined : enterFolder(f)">
                <n-icon :component="f.file ? FileGlyph : FolderGlyph" size="17" :color="f.file ? undefined : 'var(--accent)'" />
                <span class="file-name">{{ f.name }}</span>
              </div>
            </td>
            <td class="muted">{{ f.file ? formatSize(f.size) : '—' }}</td>
            <td>
              <n-space :size="8">
                <template v-if="f.file">
                  <n-tooltip trigger="hover">
                    <template #trigger>
                      <n-button size="tiny" type="primary" secondary :loading="downloading" @click="downloadOne(f)">
                        <template #icon><n-icon><download-icon /></n-icon></template>
                        下载
                      </n-button>
                    </template>
                    创建下载任务
                  </n-tooltip>
                </template>
                <n-tooltip v-else trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" secondary @click="enterFolder(f)">
                      <template #icon><n-icon><folder-glyph /></n-icon></template>
                      进入
                    </n-button>
                  </template>
                  打开文件夹
                </n-tooltip>
              </n-space>
            </td>
          </tr>
        </tbody>
      </n-table>
      <div class="results-foot muted">
        <n-icon :component="InfoIcon" size="14" />
        <span>直链有时效，大文件下载中断时会自动刷新重试；Cookie 失效时请在设置中更新。</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.parse-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 980px;
}
.panel {
  background: var(--zinc-50);
  border: 1px solid var(--zinc-200);
  border-radius: var(--radius-panel);
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.panel-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 15px;
}
.parse-actions {
  margin-top: 2px;
}
.cookie-tip {
  font-size: 13px;
}
.links-bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}
.links-bar-label {
  font-size: 12px;
  color: var(--zinc-400);
  margin-right: 2px;
}
.link-tag {
  cursor: pointer;
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.results-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
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
  padding: 24px 0;
}
.file-cell {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.file-cell:has(.uc-table) {
  cursor: default;
}
.file-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.muted {
  color: var(--zinc-500);
  font-size: 13px;
}
.uc-table :deep(td) {
  padding: 8px 12px;
}
.results-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}
</style>
