<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue';
import { useMessage, useDialog } from 'naive-ui';
import {
  NButton,
  NIcon,
  NEmpty,
  NAlert,
  NInput,
  NModal,
  NTreeSelect,
  NSpin,
  NDropdown,
  NSpace,
  NTooltip,
  NTag,
  NTable,
} from 'naive-ui';
import {
  PhArrowLeft as ArrowLeftIcon,
  PhTrash as TrashIcon,
  PhUploadSimple as UploadIcon,
  PhGridFour as GridIcon,
  PhList as ListIcon,
  PhImage as ImageGlyph,
  PhDotsThreeVertical as MoreIcon,
  PhFolderPlus as FolderPlusIcon,
  PhHouse as HouseIcon,
  PhPencilSimple as PencilIcon,
  PhFolderOpen as FolderOpenIcon,
  PhEye as EyeIcon,
  PhArrowSquareOut as RevealIcon,
  PhArrowLineRight as MoveIcon,
  PhArrowClockwise as RefreshIcon,
  PhCaretRight as CaretRightIcon,
  PhCloudArrowUp as CloudArrowUpIcon,
  PhSortAscending as SortIcon,
  PhCaretDown as CaretDownIcon,
  PhFolderSimple as FolderIcon,
} from '@phosphor-icons/vue';
import { useFilesStore } from '@/stores/files';
import { api, formatSize, revealInFolder } from '@/api';
import FileIcon from '@/components/FileIcon.vue';
import FileThumbnail from '@/components/FileThumbnail.vue';
import type { FileNode, ViewMode } from '@/types';

const files = useFilesStore();
const message = useMessage();
const dialog = useDialog();

const viewMode = ref<ViewMode>((localStorage.getItem('ucd2-view') as ViewMode) || 'grid');
watch(viewMode, (v) => localStorage.setItem('ucd2-view', v));

const selectedId = ref<number | null>(null);
const category = ref('all');
const sortBy = ref('name');
const categories = [
  { key: 'all', label: '全部文件' },
  { key: 'folder', label: '文件夹' },
  { key: 'image', label: '图片' },
  { key: 'video', label: '视频' },
  { key: 'document', label: '文档' },
  { key: 'other', label: '其他' },
];
const sortOptions = [
  { label: '按名称排序', key: 'name' },
  { label: '最近修改优先', key: 'modified' },
  { label: '按大小排序', key: 'size' },
];
function fileCategory(node: FileNode) {
  if (node.is_dir) return 'folder';
  const ext = node.name.split('.').pop()?.toLowerCase() || '';
  if (node.mime.startsWith('image/') || /^(png|jpe?g|gif|webp|svg|heic|avif|bmp)$/.test(ext))
    return 'image';
  if (node.mime.startsWith('video/') || /^(mp4|mov|mkv|avi|webm)$/.test(ext)) return 'video';
  if (node.mime.startsWith('text/') || /^(pdf|docx?|xlsx?|pptx?|txt|md|csv)$/.test(ext))
    return 'document';
  return 'other';
}
const visibleItems = computed(() =>
  files.items
    .filter((node) => category.value === 'all' || fileCategory(node) === category.value)
    .slice()
    .sort((a, b) => {
      if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
      if (sortBy.value === 'modified')
        return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
      if (sortBy.value === 'size') return b.size - a.size;
      return a.name.localeCompare(b.name, 'zh-CN', { numeric: true });
    }),
);
const currentTitle = computed(() => files.breadcrumbs.at(-1)?.name || '我的文件');
const totalSize = computed(() =>
  visibleItems.value.reduce((total, node) => total + (node.is_dir ? 0 : node.size), 0),
);
watch(
  () => files.currentDir,
  () => {
    selectedId.value = null;
    category.value = 'all';
  },
);
watch(
  () => files.items,
  (items) => {
    if (selectedId.value !== null && !items.some((node) => node.id === selectedId.value))
      selectedId.value = null;
  },
);

function selectNode(node: FileNode) {
  selectedId.value = node.id;
}

// 新建目录
const showNewDir = ref(false);
const newDirName = ref('');
async function confirmNewDir() {
  const name = newDirName.value.trim();
  if (!name) return message.warning('请输入目录名');
  await files.createDir(name);
  showNewDir.value = false;
  newDirName.value = '';
}

// 重命名
const showRename = ref(false);
const renameTarget = ref<FileNode | null>(null);
const renameValue = ref('');
function openRename(node: FileNode) {
  renameTarget.value = node;
  renameValue.value = node.name;
  showRename.value = true;
}
async function confirmRename() {
  const name = renameValue.value.trim();
  if (!name || !renameTarget.value) return;
  await files.rename(renameTarget.value.id, name);
  showRename.value = false;
}

// 移动
const showMove = ref(false);
const moveTarget = ref<FileNode | null>(null);
const moveParent = ref<number | null>(null);
const treeOptions = ref<{ label: string; key: number | 'root'; children: any[] }[]>([]);
async function openMove(node: FileNode) {
  moveTarget.value = node;
  moveParent.value = null;
  const tree = await api.tree();
  const prune = (nodes: any[]): any[] =>
    nodes
      .filter((n) => n.id !== node.id)
      .map((n) => ({ label: n.name, key: n.id, children: prune(n.children) }));
  treeOptions.value = [{ label: '（网盘根目录）', key: 'root', children: prune(tree) }];
  showMove.value = true;
}
async function confirmMove() {
  if (!moveTarget.value) return;
  await files.move(moveTarget.value.id, moveParent.value);
  showMove.value = false;
}

// 删除
function confirmDelete(node: FileNode) {
  dialog.warning({
    title: '删除确认',
    content: node.is_dir
      ? `将删除目录「${node.name}」及其全部内容，可到系统回收站恢复。`
      : `将删除文件「${node.name}」，可到系统回收站恢复。`,
    positiveText: '删除',
    negativeText: '取消',
    onPositiveClick: async () => {
      try {
        await files.remove(node.id);
        message.success('已删除（可到回收站恢复）');
      } catch (e) {
        message.error((e as Error).message || '删除失败');
      }
    },
  });
}

// 预览
const previewNode = ref<FileNode | null>(null);
const previewUrl = ref('');
const previewOpen = computed({
  get: () => previewNode.value != null,
  set: (v: boolean) => {
    if (!v) closePreview();
  },
});
async function openPreview(node: FileNode) {
  previewNode.value = node;
  previewUrl.value = await api.downloadUrl(node.id, true);
}
function closePreview() {
  previewNode.value = null;
  previewUrl.value = '';
}

/** 在系统文件管理器中定位/打开 */
async function reveal(node: FileNode) {
  const ok = await revealInFolder(node.path);
  if (!ok) message.info('当前环境不支持打开系统文件管理器，请手动访问目录');
}

function openNode(node: FileNode) {
  if (node.is_dir) {
    selectedId.value = null;
    files.enterDir(node.id);
  } else {
    openPreview(node);
  }
}

function goRoot() {
  selectedId.value = null;
  files.enterDir(null);
}

// 页面全局拖拽上传
const dragOver = ref(false);
function onDrop(e: DragEvent) {
  dragOver.value = false;
  const list = e.dataTransfer?.files;
  if (list && list.length) {
    files.upload(Array.from(list)).then(() => message.success(`已上传 ${list.length} 个文件`));
  }
}

// 上传弹窗拖拽选框
const modalDragOver = ref(false);
const modalFileInputRef = ref<HTMLInputElement | null>(null);
function triggerFileInput() {
  modalFileInputRef.value?.click();
}
function onModalDrop(e: DragEvent) {
  modalDragOver.value = false;
  const list = e.dataTransfer?.files;
  if (list && list.length) {
    files.upload(Array.from(list)).then(() => {
      message.success(`已上传 ${list.length} 个文件`);
      files.showUpload = false;
    });
  }
}
function onModalFileChange(e: Event) {
  const input = e.target as HTMLInputElement;
  if (input.files?.length) {
    files.upload(Array.from(input.files)).then(() => {
      message.success(`已上传 ${input.files!.length} 个文件`);
      files.showUpload = false;
    });
    input.value = '';
  }
}

onActivated(() => files.refresh());
</script>

<template>
  <div
    class="file-page"
    @dragenter.prevent="dragOver = true"
    @dragover.prevent
    @dragleave.prevent="dragOver = false"
    @drop.prevent="onDrop"
  >
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ currentTitle }}</h1>
        <p class="page-desc">
          {{ files.currentDir === null ? '重要的文件，在这里井然有序。' : '我的文件中的文件夹' }}
        </p>
      </div>
      <n-button type="primary" @click="files.showUpload = true">
        <template #icon><n-icon :component="UploadIcon" /></template>上传文件
      </n-button>
    </div>
    <div class="file-categories" role="tablist" aria-label="文件类型">
      <button
        v-for="item in categories"
        :key="item.key"
        role="tab"
        :aria-selected="category === item.key"
        :class="{ active: category === item.key }"
        @click="
          category = item.key;
          selectedId = null;
        "
      >
        {{ item.label }}
      </button>
    </div>
    <!-- 工具条 -->
    <div class="toolbar">
      <div class="toolbar-left">
        <div class="path-bar">
          <button
            class="path-btn root-btn"
            :class="{ active: files.currentDir == null }"
            @click="goRoot"
            aria-label="网盘根目录"
          >
            <n-icon :component="HouseIcon" size="15" />
            <span>我的文件</span>
          </button>
          <template v-for="(b, i) in files.breadcrumbs" :key="b.id">
            <n-icon :component="CaretRightIcon" size="13" class="path-sep" />
            <button
              class="path-btn"
              :class="{ active: i === files.breadcrumbs.length - 1 }"
              @click="files.enterDir(b.id)"
            >
              {{ b.name }}
            </button>
          </template>
        </div>
      </div>
      <div class="toolbar-right">
        <n-tooltip trigger="hover">
          <template #trigger>
            <n-button quaternary circle size="small" @click="files.refresh()" aria-label="刷新">
              <template #icon><n-icon :component="RefreshIcon" /></template>
            </n-button>
          </template>
          刷新列表
        </n-tooltip>
        <n-tooltip trigger="hover">
          <template #trigger>
            <n-button
              quaternary
              circle
              size="small"
              @click="showNewDir = true"
              aria-label="新建目录"
            >
              <template #icon><n-icon :component="FolderPlusIcon" /></template>
            </n-button>
          </template>
          新建目录
        </n-tooltip>
        <n-dropdown
          trigger="click"
          :options="sortOptions"
          @select="(key: string) => (sortBy = key)"
        >
          <n-tooltip
            ><template #trigger
              ><n-button quaternary size="small" aria-label="排序"
                ><template #icon><n-icon :component="SortIcon" /></template
                ><span class="sort-label">{{
                  sortOptions.find((option) => option.key === sortBy)?.label
                }}</span
                ><n-icon
                  :component="CaretDownIcon"
                  size="11"
                  class="sort-caret" /></n-button></template
            >文件排序</n-tooltip
          >
        </n-dropdown>
        <div class="view-switch" role="group" aria-label="显示方式">
          <n-tooltip
            ><template #trigger
              ><button
                :class="{ active: viewMode === 'grid' }"
                :aria-pressed="viewMode === 'grid'"
                aria-label="网格视图"
                @click="viewMode = 'grid'"
              >
                <n-icon :component="GridIcon" size="16" /></button></template
            >网格视图</n-tooltip
          >
          <n-tooltip
            ><template #trigger
              ><button
                :class="{ active: viewMode === 'list' }"
                :aria-pressed="viewMode === 'list'"
                aria-label="列表视图"
                @click="viewMode = 'list'"
              >
                <n-icon :component="ListIcon" size="16" /></button></template
            >列表视图</n-tooltip
          >
        </div>
      </div>
    </div>

    <!-- 拖拽上传全屏遮罩 -->
    <div v-if="dragOver" class="drop-mask">
      <n-icon :component="CloudArrowUpIcon" size="48" />
      <p class="drop-mask-title">松开立即上传到当前目录</p>
      <span class="drop-mask-sub">支持多文件批量上传</span>
    </div>

    <!-- 内容区 -->
    <n-spin :show="files.loading">
      <div v-if="files.error" class="error-box">
        <n-alert type="error" :title="files.error">
          <div class="error-row">
            <span>后端服务可能尚未就绪，请稍后重试</span>
            <n-button size="small" secondary @click="files.refresh()">重试</n-button>
          </div>
        </n-alert>
      </div>

      <!-- 网格视图 -->
      <div v-else-if="viewMode === 'grid'" class="grid-view">
        <div
          v-for="node in visibleItems"
          :key="node.id"
          class="grid-item hover-lift"
          :class="{ selected: selectedId === node.id }"
          @click="selectNode(node)"
          @dblclick="openNode(node)"
          tabindex="0"
          role="button"
          :aria-label="node.name"
          :aria-pressed="selectedId === node.id"
          @keydown.enter.self.prevent="openNode(node)"
          @keydown.space.self.prevent="selectNode(node)"
        >
          <div class="grid-icon">
            <file-thumbnail :node="node" />
          </div>
          <div class="grid-name" :title="node.name" @click.stop="openNode(node)">
            {{ node.name }}
          </div>
          <div class="grid-meta tabular-nums">
            {{ node.is_dir ? '目录' : formatSize(node.size) }}
          </div>
          <div class="grid-actions" @click.stop>
            <n-tooltip trigger="hover">
              <template #trigger>
                <n-button
                  quaternary
                  circle
                  size="tiny"
                  class="grid-action-btn"
                  :aria-label="node.is_dir ? '打开' : '预览'"
                  @click="openNode(node)"
                >
                  <template #icon
                    ><n-icon :component="node.is_dir ? FolderOpenIcon : EyeIcon" size="14"
                  /></template>
                </n-button>
              </template>
              {{ node.is_dir ? '打开' : '预览' }}
            </n-tooltip>
            <n-dropdown
              trigger="click"
              :options="[
                { label: '打开所在目录', key: 'reveal' },
                { label: '重命名', key: 'rename' },
                { label: '移动', key: 'move' },
                { label: '删除', key: 'delete' },
              ]"
              @select="
                (k: string) =>
                  k === 'reveal'
                    ? reveal(node)
                    : k === 'rename'
                      ? openRename(node)
                      : k === 'move'
                        ? openMove(node)
                        : confirmDelete(node)
              "
            >
              <n-button quaternary circle size="tiny" class="grid-action-btn" aria-label="更多操作">
                <template #icon><n-icon :component="MoreIcon" size="14" /></template>
              </n-button>
            </n-dropdown>
          </div>
        </div>
        <div v-if="!files.loading && !visibleItems.length" class="empty-box">
          <n-empty :description="category === 'all' ? '还没有文件' : '没有此类型的文件'">
            <template #icon
              ><n-icon :component="FolderIcon" size="54" color="var(--zinc-300)"
            /></template>
            <template #extra>
              <n-button size="small" type="primary" secondary @click="files.showUpload = true">
                <template #icon><n-icon :component="UploadIcon" /></template>
                上传文件
              </n-button>
            </template>
          </n-empty>
        </div>
      </div>

      <!-- 列表视图 -->
      <n-table v-else-if="viewMode === 'list'" class="list-table" :bordered="false" size="small">
        <thead>
          <tr>
            <th style="width: 44px"></th>
            <th>名称</th>
            <th style="width: 110px; text-align: right">大小</th>
            <th style="width: 170px; text-align: right">修改时间</th>
            <th style="width: 110px; text-align: right">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="node in visibleItems"
            :key="node.id"
            :class="{ 'row-selected': selectedId === node.id }"
            @click="selectNode(node)"
            @dblclick="openNode(node)"
          >
            <td class="cell-icon">
              <file-icon :name="node.name" :is-dir="node.is_dir" :mime="node.mime" :size="19" />
            </td>
            <td class="cell-name" @click.stop="openNode(node)">
              <span class="file-name-text" :title="node.name">{{ node.name }}</span>
            </td>
            <td class="cell-size tabular-nums">{{ node.is_dir ? '—' : formatSize(node.size) }}</td>
            <td class="cell-muted tabular-nums">
              {{ new Date(node.updated_at).toLocaleString('zh-CN') }}
            </td>
            <td class="cell-actions">
              <n-space size="small" justify="end" align="center">
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button
                      size="tiny"
                      quaternary
                      circle
                      :aria-label="node.is_dir ? '打开' : '预览'"
                      @click.stop="openNode(node)"
                    >
                      <template #icon
                        ><n-icon :component="node.is_dir ? FolderOpenIcon : EyeIcon"
                      /></template>
                    </n-button>
                  </template>
                  {{ node.is_dir ? '进入' : '预览' }}
                </n-tooltip>
                <n-dropdown
                  trigger="click"
                  :options="[
                    { label: '打开所在目录', key: 'reveal' },
                    { label: '重命名', key: 'rename' },
                    { label: '移动', key: 'move' },
                    { label: '删除', key: 'delete' },
                  ]"
                  @select="
                    (k: string) =>
                      k === 'reveal'
                        ? reveal(node)
                        : k === 'rename'
                          ? openRename(node)
                          : k === 'move'
                            ? openMove(node)
                            : confirmDelete(node)
                  "
                >
                  <n-button size="tiny" quaternary circle aria-label="更多操作" @click.stop>
                    <template #icon><n-icon :component="MoreIcon" /></template>
                  </n-button>
                </n-dropdown>
              </n-space>
            </td>
          </tr>
        </tbody>
      </n-table>
      <div
        v-if="!files.loading && !files.error && !visibleItems.length && viewMode === 'list'"
        class="empty-box"
      >
        <n-empty :description="category === 'all' ? '还没有文件' : '没有此类型的文件'" />
      </div>
    </n-spin>
    <div v-if="!files.error" class="file-summary">
      <span
        >{{ visibleItems.length }} 个项目<span v-if="selectedId !== null">，已选中 1 项</span></span
      ><span v-if="totalSize">{{ formatSize(totalSize) }}</span>
    </div>

    <!-- 上传弹窗 -->
    <n-modal
      v-model:show="files.showUpload"
      preset="card"
      title="上传文件"
      :style="{ width: '500px' }"
    >
      <div
        class="upload-dropzone"
        :class="{ dragging: modalDragOver }"
        @dragenter.prevent="modalDragOver = true"
        @dragover.prevent="modalDragOver = true"
        @dragleave.prevent="modalDragOver = false"
        @drop.prevent="onModalDrop"
        @click="triggerFileInput"
      >
        <input
          ref="modalFileInputRef"
          type="file"
          multiple
          class="hidden-file-input"
          @change="onModalFileChange"
        />
        <div class="dropzone-icon">
          <n-icon :component="UploadIcon" size="32" color="var(--accent)" />
        </div>
        <div class="dropzone-title">点击选择文件，或拖拽文件到此处</div>
        <div class="dropzone-hint">支持批量多文件上传，上传后将保存在当前目录下</div>
        <n-button
          size="small"
          type="primary"
          secondary
          style="margin-top: 6px"
          @click.stop="triggerFileInput"
        >
          浏览本地文件
        </n-button>
      </div>
      <div v-if="files.uploading" class="upload-progress-box">
        <n-spin size="small" />
        <span>正在上传中，请稍候…</span>
      </div>
    </n-modal>

    <!-- 新建目录 -->
    <n-modal v-model:show="showNewDir" preset="card" title="新建目录" :style="{ width: '400px' }">
      <n-input
        v-model:value="newDirName"
        placeholder="输入目录名"
        @keydown.enter="confirmNewDir"
        autofocus
      />
      <template #footer>
        <n-space justify="end">
          <n-button @click="showNewDir = false">取消</n-button>
          <n-button type="primary" @click="confirmNewDir">创建</n-button>
        </n-space>
      </template>
    </n-modal>

    <!-- 重命名 -->
    <n-modal v-model:show="showRename" preset="card" title="重命名" :style="{ width: '400px' }">
      <n-input v-model:value="renameValue" placeholder="新名称" @keydown.enter="confirmRename" />
      <template #footer>
        <n-space justify="end">
          <n-button @click="showRename = false">取消</n-button>
          <n-button type="primary" @click="confirmRename">确定</n-button>
        </n-space>
      </template>
    </n-modal>

    <!-- 移动 -->
    <n-modal v-model:show="showMove" preset="card" title="移动到" :style="{ width: '420px' }">
      <n-tree-select
        v-model:value="moveParent"
        :options="treeOptions"
        key-field="key"
        label-field="label"
        children-field="children"
        placeholder="选择目标目录"
        clearable
        style="width: 100%"
      />
      <template #footer>
        <n-space justify="end">
          <n-button @click="showMove = false">取消</n-button>
          <n-button type="primary" @click="confirmMove">移动</n-button>
        </n-space>
      </template>
    </n-modal>

    <!-- 预览 -->
    <n-modal
      v-model:show="previewOpen"
      preset="card"
      :title="previewNode?.name"
      :style="{ width: 'min(860px, 92vw)' }"
      @close="closePreview"
    >
      <template #header-extra>
        <n-space v-if="previewNode" size="small" align="center">
          <n-tag size="small" :bordered="false">{{ formatSize(previewNode.size) }}</n-tag>
          <n-tag size="small" :bordered="false" type="info">{{
            previewNode.mime || '未知格式'
          }}</n-tag>
        </n-space>
      </template>
      <div class="preview-body">
        <template v-if="previewNode">
          <img
            v-if="previewNode.mime.startsWith('image/')"
            :src="previewUrl"
            class="preview-img"
            alt="图片预览"
          />
          <video
            v-else-if="previewNode.mime.startsWith('video/')"
            :src="previewUrl"
            class="preview-media"
            controls
            autoplay
          />
          <audio
            v-else-if="previewNode.mime.startsWith('audio/')"
            :src="previewUrl"
            class="preview-audio"
            controls
          />
          <iframe
            v-else-if="
              previewNode.mime === 'text/plain' ||
              previewNode.mime === 'text/markdown' ||
              previewNode.mime === 'application/json'
            "
            :src="previewUrl"
            class="preview-frame"
          />
          <div v-else class="preview-unavailable">
            <n-icon :component="ImageGlyph" size="44" color="var(--zinc-400)" />
            <p>该格式暂不支持直接在线预览</p>
            <n-button type="primary" secondary @click="previewNode && reveal(previewNode)">
              <template #icon><n-icon :component="RevealIcon" /></template>
              在系统文件管理器中打开
            </n-button>
          </div>
        </template>
      </div>
      <template #footer>
        <n-space justify="space-between" align="center">
          <n-button v-if="previewNode" secondary size="small" @click="reveal(previewNode)">
            <template #icon><n-icon :component="RevealIcon" /></template>
            打开所在目录
          </n-button>
          <div v-else></div>
          <n-button size="small" @click="closePreview">关闭</n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.file-page {
  position: relative;
  min-height: 100%;
  max-width: 1400px;
  margin: 0 auto;
}
.file-categories {
  display: flex;
  gap: 26px;
  border-bottom: 1px solid var(--line);
  margin-bottom: 17px;
  overflow-x: auto;
}
.file-categories button {
  position: relative;
  flex-shrink: 0;
  height: 42px;
  padding: 0 1px 13px;
  border: 0;
  background: transparent;
  color: var(--zinc-500);
  font-size: 12px;
  cursor: pointer;
}
.file-categories button:hover {
  color: var(--zinc-900);
}
.file-categories button.active {
  color: var(--accent);
  font-weight: 600;
}
.file-categories button.active::after {
  content: '';
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent);
  border-radius: 2px;
}
.view-switch {
  display: flex;
  align-items: center;
  padding: 2px;
  gap: 2px;
  background: var(--zinc-100);
  border-radius: 6px;
}
.view-switch button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 26px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--zinc-500);
  cursor: pointer;
}
.view-switch button.active {
  background: var(--surface);
  color: var(--zinc-900);
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.08);
}
.sort-label {
  font-size: 11px;
  color: var(--zinc-500);
}
.sort-caret {
  margin-left: 8px;
  color: var(--zinc-500);
}
.file-summary {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 0;
  margin-top: 24px;
  border-top: 1px solid var(--line);
  font-size: 11px;
  color: var(--zinc-500);
}
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 24px;
  flex-wrap: wrap;
}
.toolbar-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.path-bar {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 0;
  max-width: 440px;
  overflow-x: auto;
}
.path-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: transparent;
  border: none;
  font-size: 11px;
  font-weight: 500;
  color: var(--zinc-700);
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  transition:
    background 0.12s ease,
    color 0.12s ease;
  white-space: nowrap;
}
.path-btn:hover {
  background: var(--zinc-200);
  color: var(--zinc-900);
}
.path-btn.active {
  color: var(--zinc-600);
  font-weight: 600;
}
.path-sep {
  color: var(--zinc-400);
  flex-shrink: 0;
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}
.drop-mask {
  position: fixed;
  inset: 0;
  z-index: 999;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  background: color-mix(in srgb, var(--accent) 12%, rgba(0, 0, 0, 0.45));
  backdrop-filter: blur(4px);
  border: 2px dashed var(--accent);
  color: #ffffff;
  pointer-events: none;
}
.drop-mask-title {
  font-size: 18px;
  font-weight: 600;
  margin: 0;
}
.drop-mask-sub {
  font-size: 13px;
  opacity: 0.9;
}
.error-box {
  margin-bottom: 16px;
}
.error-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.grid-view {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(138px, 1fr));
  gap: 20px 16px;
}
.grid-item {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 12px 10px;
  min-width: 0;
  min-height: 164px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 7px;
  cursor: pointer;
  user-select: none;
  transition: all 0.15s ease;
}
.grid-item:hover {
  background: var(--hover);
  transform: none;
}
.grid-item.selected {
  border-color: var(--accent);
  background: var(--selection);
  box-shadow: none;
}
.grid-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 88px;
}
.grid-name {
  width: 100%;
  text-align: center;
  font-size: 12px;
  font-weight: 500;
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-overflow: ellipsis;
  word-break: break-all;
  height: 36px;
}
.grid-meta {
  font-size: 10px;
  color: var(--zinc-500);
}
.grid-actions {
  position: absolute;
  top: 6px;
  right: 6px;
  display: flex;
  gap: 2px;
  opacity: 0;
  transition: opacity 0.12s ease;
}
.grid-item:hover .grid-actions,
.grid-item.selected .grid-actions,
.grid-item:focus-within .grid-actions {
  opacity: 1;
}
.grid-action-btn {
  background: var(--zinc-100);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}
.empty-box {
  grid-column: 1 / -1;
  padding: 70px 0;
}
.list-table {
  --n-th-color: transparent;
}
.list-table tr {
  cursor: pointer;
  transition: background 0.1s ease;
}
.list-table tr:hover td {
  background: var(--zinc-100);
}
.list-table tr.row-selected td {
  background: color-mix(in srgb, var(--accent) 6%, var(--zinc-50));
}
.cell-icon {
  text-align: center;
}
.cell-name {
  font-weight: 500;
}
.file-name-text {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
  transition: color 0.12s ease;
}
.file-name-text:hover {
  color: var(--accent);
}
.cell-size {
  text-align: right;
  color: var(--zinc-600);
  font-size: 13px;
}
.cell-muted {
  text-align: right;
  color: var(--zinc-500);
  font-size: 12.5px;
}
.cell-actions {
  text-align: right;
}
.upload-dropzone {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 36px 20px;
  border: 2px dashed var(--zinc-300);
  border-radius: var(--radius-panel);
  background: var(--zinc-100);
  cursor: pointer;
  transition: all 0.15s ease;
}
.upload-dropzone:hover,
.upload-dropzone.dragging {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 5%, var(--zinc-100));
}
.dropzone-icon {
  width: 48px;
  height: 48px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 4px;
}
.dropzone-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--zinc-900);
}
.dropzone-hint {
  font-size: 12px;
  color: var(--zinc-500);
}
.hidden-file-input {
  display: none;
}
.upload-progress-box {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  margin-top: 14px;
  font-size: 13px;
  color: var(--zinc-600);
}
.preview-body {
  min-height: 240px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 10px 0;
}
.preview-img {
  max-width: 100%;
  max-height: 70vh;
  border-radius: var(--radius-control);
  object-fit: contain;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.08);
}
.preview-media {
  max-width: 100%;
  max-height: 70vh;
  border-radius: var(--radius-control);
}
.preview-audio {
  width: 100%;
}
.preview-frame {
  width: 100%;
  height: 62vh;
  border: 1px solid var(--zinc-200);
  border-radius: var(--radius-control);
  background: var(--zinc-100);
}
.preview-unavailable {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: var(--zinc-500);
}
@media (max-width: 900px) {
  .sort-label {
    display: none;
  }
  .sort-caret {
    display: none;
  }
}
@media (max-width: 680px) {
  .file-categories {
    gap: 23px;
  }
  .toolbar {
    gap: 8px;
  }
  .toolbar-right {
    gap: 3px;
    margin-left: auto;
  }
  .path-bar {
    max-width: 170px;
  }
  .grid-view {
    grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
    gap: 12px 6px;
  }
  .grid-actions {
    opacity: 1;
  }
  .grid-actions > .n-tooltip {
    display: none;
  }
  .list-table th:nth-child(4),
  .list-table td:nth-child(4) {
    display: none;
  }
  .list-table th:nth-child(3) {
    width: 64px !important;
  }
  .list-table th:nth-child(5) {
    width: 68px !important;
  }
  .list-table td {
    padding: 10px 4px;
  }
  .list-table th:first-child {
    width: 28px !important;
  }
  .cell-actions :deep(.n-space) {
    gap: 0 !important;
  }
  .error-row {
    flex-wrap: wrap;
  }
}
@media (hover: none) {
  .grid-actions {
    opacity: 1;
  }
}
</style>
