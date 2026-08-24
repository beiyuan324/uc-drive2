<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue';
import { useMessage, useDialog } from 'naive-ui';
import {
  NButton, NIcon, NBreadcrumb, NBreadcrumbItem, NEmpty, NAlert,
  NInput, NModal, NTreeSelect, NSpin, NDropdown, NSpace,
} from 'naive-ui';
import {
  PhArrowLeft as ArrowLeftIcon, PhTrash as TrashIcon,
  PhUploadSimple as UploadIcon, PhGridFour as GridIcon, PhList as ListIcon,
  PhImage as ImageGlyph, PhDotsThreeVertical as MoreIcon,
  PhFolderPlus as FolderPlusIcon,
  PhPencilSimple as PencilIcon, PhFolderOpen as FolderOpenIcon, PhEye as EyeIcon,
  PhArrowSquareOut as RevealIcon, PhArrowLineRight as MoveIcon,
} from '@phosphor-icons/vue';
import { useFilesStore } from '@/stores/files';
import { api, formatSize, revealInFolder } from '@/api';
import FileIcon from '@/components/FileIcon.vue';
import type { FileNode, ViewMode } from '@/types';

const files = useFilesStore();
const message = useMessage();
const dialog = useDialog();

const viewMode = ref<ViewMode>((localStorage.getItem('ucd2-view') as ViewMode) || 'grid');
watch(viewMode, v => localStorage.setItem('ucd2-view', v));

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
  // 递归剪除被移动节点自身（含其整棵子树），避免移动到自身/后代
  const prune = (nodes: any[]): any[] =>
    nodes.filter(n => n.id !== node.id).map(n => ({ label: n.name, key: n.id, children: prune(n.children) }));
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
  set: (v: boolean) => { if (!v) closePreview(); },
});
async function openPreview(node: FileNode) {
  previewNode.value = node;
  previewUrl.value = await api.downloadUrl(node.id, true);
}
function closePreview() {
  previewNode.value = null;
  previewUrl.value = '';
}

/** 在系统文件管理器中定位/打开（文件用 explorer /select, 定位，目录直接打开） */
async function reveal(node: FileNode) {
  const ok = await revealInFolder(node.path);
  if (!ok) message.info('当前环境不支持打开系统文件管理器，请手动访问目录');
}

function openNode(node: FileNode) {
  if (node.is_dir) files.enterDir(node.id);
  else openPreview(node);
}

function goRoot() {
  files.enterDir(null);
}

const breadcrumbItems = computed(() => [
  { label: '网盘根目录', id: null as number | null },
  ...files.breadcrumbs.map(b => ({ label: b.name, id: b.id })),
]);

// 拖拽上传
const dragOver = ref(false);
function onDrop(e: DragEvent) {
  dragOver.value = false;
  const list = e.dataTransfer?.files;
  if (list && list.length) {
    files.upload(Array.from(list)).then(() => message.success(`已上传 ${list.length} 个文件`));
  }
}

onActivated(() => files.refresh());
</script>

<template>
  <div class="file-page" @dragenter.prevent="dragOver = true" @dragover.prevent @dragleave.prevent="dragOver = false" @drop.prevent="onDrop">
    <!-- 工具条 -->
    <div class="toolbar">
      <div class="toolbar-left">
        <n-tooltip trigger="hover">
          <template #trigger>
            <n-button quaternary circle size="small" :disabled="files.currentDir == null" @click="goRoot" aria-label="返回根目录">
              <template #icon><n-icon :component="ArrowLeftIcon" /></template>
            </n-button>
          </template>
          返回根目录
        </n-tooltip>
        <n-breadcrumb>
          <n-breadcrumb-item v-for="(b, i) in breadcrumbItems" :key="i" @click="files.enterDir(b.id)">
            {{ b.label }}
          </n-breadcrumb-item>
        </n-breadcrumb>
      </div>
      <div class="toolbar-right">
        <n-tooltip trigger="hover">
          <template #trigger>
            <n-button quaternary circle size="small" @click="showNewDir = true" aria-label="新建目录">
              <template #icon><n-icon :component="FolderPlusIcon" /></template>
            </n-button>
          </template>
          新建目录
        </n-tooltip>
        <n-tooltip trigger="hover">
          <template #trigger>
            <n-button quaternary circle size="small" @click="files.showUpload = true" aria-label="上传文件">
              <template #icon><n-icon :component="UploadIcon" /></template>
            </n-button>
          </template>
          上传文件
        </n-tooltip>
        <n-button-group size="small">
          <n-button :type="viewMode === 'grid' ? 'primary' : 'default'" @click="viewMode = 'grid'" aria-label="网格视图">
            <template #icon><n-icon :component="GridIcon" /></template>
          </n-button>
          <n-button :type="viewMode === 'list' ? 'primary' : 'default'" @click="viewMode = 'list'" aria-label="列表视图">
            <template #icon><n-icon :component="ListIcon" /></template>
          </n-button>
        </n-button-group>
      </div>
    </div>

    <!-- 拖拽上传遮罩 -->
    <div v-if="dragOver" class="drop-mask">
      <n-icon :component="UploadIcon" size="40" />
      <p>松开以上传</p>
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
          v-for="node in files.items"
          :key="node.id"
          class="grid-item"
          @click="openNode(node)"
          @dblclick="node.is_dir && files.enterDir(node.id)"
        >
          <div class="grid-icon">
            <file-icon :name="node.name" :is-dir="node.is_dir" :mime="node.mime" :size="34" />
          </div>
          <div class="grid-name" :title="node.name">{{ node.name }}</div>
          <div class="grid-meta">{{ node.is_dir ? '目录' : formatSize(node.size) }}</div>
          <n-dropdown
            trigger="click"
            :options="[
              { label: '打开所在目录', key: 'reveal' },
              { label: '重命名', key: 'rename' },
              { label: '移动', key: 'move' },
              { label: '删除', key: 'delete' },
            ]"
            @select="(k: string) => k === 'reveal' ? reveal(node) : k === 'rename' ? openRename(node) : k === 'move' ? openMove(node) : confirmDelete(node)"
          >
            <n-button quaternary circle size="small" class="grid-more" @click.stop>
              <template #icon><n-icon :component="MoreIcon" /></template>
            </n-button>
          </n-dropdown>
        </div>
        <n-empty v-if="files.isEmpty" description="目录为空，拖拽文件到此处上传" class="empty-box" />
      </div>

      <!-- 列表视图 -->
      <n-table v-else-if="viewMode === 'list'" class="list-table" :bordered="false">
        <thead>
          <tr>
            <th style="width: 40px"></th>
            <th>名称</th>
            <th style="width: 120px">大小</th>
            <th style="width: 180px">修改时间</th>
            <th style="width: 160px">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="node in files.items" :key="node.id" @dblclick="node.is_dir && files.enterDir(node.id)">
            <td class="cell-icon"><file-icon :name="node.name" :is-dir="node.is_dir" :mime="node.mime" /></td>
            <td class="cell-name" @click="openNode(node)">{{ node.name }}</td>
            <td>{{ node.is_dir ? '—' : formatSize(node.size) }}</td>
            <td class="cell-muted">{{ new Date(node.updated_at).toLocaleString('zh-CN') }}</td>
            <td>
              <n-space size="small" align="center">
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary circle :aria-label="node.is_dir ? '打开' : '预览'" @click="node.is_dir ? files.enterDir(node.id) : openPreview(node)">
                      <template #icon><n-icon :component="node.is_dir ? FolderOpenIcon : EyeIcon" /></template>
                    </n-button>
                  </template>
                  {{ node.is_dir ? '打开' : '预览' }}
                </n-tooltip>
                <n-tooltip v-if="!node.is_dir" trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary circle aria-label="打开所在目录" @click="reveal(node)">
                      <template #icon><n-icon :component="RevealIcon" /></template>
                    </n-button>
                  </template>
                  打开所在目录
                </n-tooltip>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary circle aria-label="移动" @click="openMove(node)">
                      <template #icon><n-icon :component="MoveIcon" /></template>
                    </n-button>
                  </template>
                  移动
                </n-tooltip>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary circle aria-label="重命名" @click="openRename(node)">
                      <template #icon><n-icon :component="PencilIcon" /></template>
                    </n-button>
                  </template>
                  重命名
                </n-tooltip>
                <n-tooltip trigger="hover">
                  <template #trigger>
                    <n-button size="tiny" quaternary circle type="error" aria-label="删除" @click="confirmDelete(node)">
                      <template #icon><n-icon :component="TrashIcon" /></template>
                    </n-button>
                  </template>
                  删除
                </n-tooltip>
              </n-space>
            </td>
          </tr>
        </tbody>
      </n-table>
      <n-empty v-if="files.isEmpty && viewMode === 'list'" description="目录为空" />
    </n-spin>

    <!-- 上传弹窗 -->
    <n-modal v-model:show="files.showUpload" preset="card" title="上传文件" :style="{ width: '480px' }">
      <input type="file" multiple class="upload-input" @change="(e: Event) => {
        const input = e.target as HTMLInputElement;
        if (input.files?.length) {
          files.upload(Array.from(input.files)).then(() => { message.success('上传完成'); files.showUpload = false; });
          input.value = '';
        }
      }" />
    </n-modal>

    <!-- 新建目录 -->
    <n-modal v-model:show="showNewDir" preset="card" title="新建目录" :style="{ width: '400px' }">
      <n-input v-model:value="newDirName" placeholder="目录名" @keydown.enter="confirmNewDir" autofocus />
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
    <n-modal v-model:show="previewOpen" preset="card" :title="previewNode?.name" :style="{ width: 'min(840px, 90vw)' }" @close="closePreview">
      <div class="preview-body">
        <template v-if="previewNode">
          <img v-if="previewNode.mime.startsWith('image/')" :src="previewUrl" class="preview-img" />
          <video v-else-if="previewNode.mime.startsWith('video/')" :src="previewUrl" class="preview-media" controls autoplay />
          <audio v-else-if="previewNode.mime.startsWith('audio/')" :src="previewUrl" class="preview-audio" controls />
          <iframe v-else-if="previewNode.mime === 'text/plain' || previewNode.mime === 'text/markdown' || previewNode.mime === 'application/json'" :src="previewUrl" class="preview-frame" />
          <div v-else class="preview-unavailable">
            <n-icon :component="ImageGlyph" size="40" />
            <p>该类型不支持在线预览</p>
            <n-button type="primary" @click="previewNode && reveal(previewNode)">打开所在目录</n-button>
          </div>
        </template>
      </div>
    </n-modal>
  </div>
</template>

<style scoped>
.file-page {
  position: relative;
  min-height: 100%;
}
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}
.toolbar-left {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}
.drop-mask {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  background: color-mix(in srgb, var(--accent) 8%, transparent);
  border: 2px dashed var(--accent);
  border-radius: var(--radius-panel);
  color: var(--accent);
  font-weight: 500;
  pointer-events: none;
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
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
}
.grid-item {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 16px 10px 12px;
  border: 1px solid transparent;
  border-radius: var(--radius-panel);
  cursor: pointer;
  transition: background 0.12s ease, border-color 0.12s ease;
}
.grid-item:hover {
  background: var(--zinc-100);
  border-color: var(--zinc-200);
}
.grid-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 44px;
}
.grid-name {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  font-weight: 500;
}
.grid-meta {
  font-size: 12px;
  color: var(--zinc-500);
}
.grid-more {
  position: absolute;
  top: 8px;
  right: 8px;
  opacity: 0;
  transition: opacity 0.12s ease;
}
.grid-item:hover .grid-more {
  opacity: 1;
}
.empty-box {
  grid-column: 1 / -1;
  padding: 60px 0;
}
.list-table {
  --n-th-color: transparent;
}
.cell-icon {
  text-align: center;
}
.cell-name {
  cursor: pointer;
  font-weight: 500;
}
.cell-muted {
  color: var(--zinc-500);
  font-size: 13px;
}
.upload-input {
  width: 100%;
  padding: 24px;
  border: 1.5px dashed var(--zinc-200);
  border-radius: var(--radius-panel);
  font-size: 13px;
}
.preview-body {
  min-height: 200px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.preview-img {
  max-width: 100%;
  max-height: 70vh;
  border-radius: var(--radius-panel);
}
.preview-media {
  max-width: 100%;
  max-height: 70vh;
  border-radius: var(--radius-panel);
}
.preview-audio {
  width: 100%;
}
.preview-frame {
  width: 100%;
  height: 60vh;
  border: none;
  border-radius: var(--radius-panel);
  background: var(--zinc-100);
}
.preview-unavailable {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: var(--zinc-500);
}
</style>
