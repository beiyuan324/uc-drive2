import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { api } from '@/api';
import type { FileNode } from '@/types';

export const useFilesStore = defineStore('files', () => {
  const currentDir = ref<number | null>(null);
  const items = ref<FileNode[]>([]);
  const breadcrumbs = ref<FileNode[]>([]);
  const loading = ref(false);
  const error = ref('');
  const uploading = ref(false);
  const showUpload = ref(false);
  const searchQuery = ref('');
  const searchResults = ref<FileNode[]>([]);
  const searching = ref(false);

  const isEmpty = computed(() => !loading.value && items.value.length === 0);

  async function doSearch() {
    const q = searchQuery.value.trim();
    if (!q) {
      searchResults.value = [];
      return;
    }
    searching.value = true;
    try {
      searchResults.value = await api.search(q);
    } finally {
      searching.value = false;
    }
  }

  async function openSearchResult(node: FileNode) {
    if (node.is_dir) {
      searchQuery.value = '';
      searchResults.value = [];
      await enterDir(node.id);
    } else {
      // 文件：跳转到其所在目录并预览
      searchQuery.value = '';
      searchResults.value = [];
      await enterDir(node.parent_id);
    }
  }

  async function refresh() {
    loading.value = true;
    error.value = '';
    try {
      items.value = await api.listFiles(currentDir.value);
      // 重建面包屑
      if (currentDir.value == null) {
        breadcrumbs.value = [];
      } else {
        const chain: FileNode[] = [];
        let node: FileNode | null = await api.getFile(currentDir.value);
        while (node) {
          chain.unshift(node);
          node = node.parent_id == null ? null : await api.getFile(node.parent_id);
        }
        breadcrumbs.value = chain;
      }
    } catch (e) {
      error.value = (e as Error).message;
    } finally {
      loading.value = false;
    }
  }

  async function enterDir(id: number | null) {
    currentDir.value = id;
    await refresh();
  }

  async function createDir(name: string) {
    await api.mkdir(name, currentDir.value);
    await refresh();
  }

  async function upload(files: File[]) {
    uploading.value = true;
    try {
      await api.upload(currentDir.value, files);
      await refresh();
    } finally {
      uploading.value = false;
    }
  }

  async function rename(id: number, name: string) {
    await api.rename(id, { name });
    await refresh();
  }

  async function move(id: number, parent: number | null) {
    await api.rename(id, { parent });
    await refresh();
  }

  async function remove(id: number) {
    await api.remove(id);
    await refresh();
  }

  return {
    currentDir, items, breadcrumbs, loading, error, uploading, isEmpty,
    showUpload, searchQuery, searchResults, searching,
    refresh, enterDir, createDir, upload, rename, move, remove, doSearch, openSearchResult,
  };
});
