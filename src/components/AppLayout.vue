<script setup lang="ts">
import { computed, watch, h, onMounted, onUnmounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { NButton, NInput, NIcon, NPopover, NEmpty, NTooltip, NDropdown, NSpin } from 'naive-ui';
import type { InputInst } from 'naive-ui';
import {
  PhFolderSimple,
  PhDownloadSimple,
  PhGearSix,
  PhLinkSimple,
  PhClockCounterClockwise,
  PhMagnifyingGlass,
  PhSun,
  PhMoon,
  PhMonitor,
  PhCaretRight,
  PhSidebarSimple,
  PhX,
  PhFile,
} from '@phosphor-icons/vue';
import { useSettingsStore } from '@/stores/settings';
import { useFilesStore } from '@/stores/files';
import { useTasksStore } from '@/stores/tasks';
import type { ThemeMode } from '@/types';
import WindowControls from './WindowControls.vue';

const route = useRoute();
const router = useRouter();
const settings = useSettingsStore();
const files = useFilesStore();
const tasks = useTasksStore();
const searchInputRef = ref<InputInst | null>(null);
const sidebarOpen = ref(false);
const activeTasksCount = computed(
  () => tasks.tasks.filter((t) => t.status === 'running' || t.status === 'queued').length,
);
const navigation = [
  { label: '我的文件', key: '/files', icon: PhFolderSimple },
  { label: '离线下载', key: '/downloads', icon: PhDownloadSimple },
  { label: '链接解析', key: '/parse', icon: PhLinkSimple },
  { label: '历史记录', key: '/history', icon: PhClockCounterClockwise },
];
const themeOptions = [
  { label: '浅色外观', key: 'light', icon: () => h(PhSun) },
  { label: '深色外观', key: 'dark', icon: () => h(PhMoon) },
  { label: '跟随系统', key: 'auto', icon: () => h(PhMonitor) },
];
function navigate(path: string) {
  sidebarOpen.value = false;
  router.push(path);
}
function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') sidebarOpen.value = false;
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    router.push('/files').then(() => searchInputRef.value?.focus());
  }
}
let searchDebounce = 0;
watch(
  () => files.searchQuery,
  () => {
    window.clearTimeout(searchDebounce);
    searchDebounce = window.setTimeout(() => {
      files.doSearch().catch(() => {});
    }, 250);
  },
);
onMounted(() => {
  settings.load();
  tasks.refresh();
  window.addEventListener('keydown', handleKeydown);
});
onUnmounted(() => {
  window.clearTimeout(searchDebounce);
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div class="app-shell">
    <button
      v-if="sidebarOpen"
      class="sidebar-scrim"
      aria-label="关闭导航"
      @click="sidebarOpen = false"
    />
    <aside class="app-sider" :class="{ 'is-open': sidebarOpen }" aria-label="主导航">
      <button class="brand" aria-label="folder-drive，打开我的文件" @click="navigate('/files')">
        <img class="brand-mark" src="@/assets/app-icon.png" alt="" />
        <strong>folder-drive</strong>
      </button>
      <nav class="sidebar-nav">
        <button
          v-for="item in navigation"
          :key="item.key"
          class="nav-item"
          :class="{ active: route.path === item.key }"
          :aria-current="route.path === item.key ? 'page' : undefined"
          @click="navigate(item.key)"
        >
          <n-icon :component="item.icon" :size="19" /><span>{{ item.label }}</span>
          <span v-if="item.key === '/downloads' && activeTasksCount" class="nav-count">{{
            activeTasksCount
          }}</span>
        </button>
      </nav>
      <div class="sidebar-bottom">
        <button
          class="nav-item"
          :class="{ active: route.path === '/settings' }"
          :aria-current="route.path === '/settings' ? 'page' : undefined"
          @click="navigate('/settings')"
        >
          <n-icon :component="PhGearSix" :size="19" /><span>设置</span>
        </button>
      </div>
    </aside>
    <div class="app-main">
      <header class="app-header" data-tauri-drag-region>
        <div class="header-location" data-tauri-drag-region>
          <n-tooltip
            ><template #trigger
              ><n-button
                quaternary
                class="sidebar-toggle"
                aria-label="切换导航"
                :aria-expanded="sidebarOpen"
                @click="sidebarOpen = !sidebarOpen"
                ><template #icon
                  ><n-icon
                    :component="sidebarOpen ? PhX : PhSidebarSimple"
                    :size="20" /></template></n-button></template
            >切换导航</n-tooltip
          >
        </div>
        <div class="header-right">
          <n-popover
            v-if="route.path === '/files'"
            trigger="focus"
            :show="files.searchQuery.length > 0"
            placement="bottom-end"
          >
            <template #trigger
              ><n-input
                ref="searchInputRef"
                v-model:value="files.searchQuery"
                class="search-input"
                placeholder="搜索文件"
                :input-props="{ 'aria-label': '搜索文件' }"
                clearable
                ><template #prefix
                  ><n-icon :component="PhMagnifyingGlass" :size="16" /></template></n-input
            ></template>
            <div class="search-panel">
              <n-spin v-if="files.searching" size="small" />
              <n-empty
                v-else-if="!files.searchResults.length"
                description="没有找到相关文件"
                size="small"
              />
              <button
                v-for="result in files.searchResults"
                :key="result.id"
                class="search-item"
                @click="files.openSearchResult(result)"
              >
                <n-icon :component="result.is_dir ? PhFolderSimple : PhFile" :size="18" /><span
                  class="search-item-name"
                  >{{ result.name }}</span
                ><n-icon :component="PhCaretRight" :size="12" />
              </button>
            </div>
          </n-popover>
          <n-dropdown
            trigger="click"
            :options="themeOptions"
            @select="(key) => settings.applyTheme(key as ThemeMode)"
          >
            <n-tooltip
              ><template #trigger
                ><n-button quaternary circle aria-label="切换外观"
                  ><template #icon
                    ><n-icon
                      :component="
                        settings.themeMode === 'dark'
                          ? PhMoon
                          : settings.themeMode === 'light'
                            ? PhSun
                            : PhMonitor
                      "
                      :size="19" /></template></n-button></template
              >切换外观</n-tooltip
            >
          </n-dropdown>
          <WindowControls />
        </div>
      </header>
      <main class="app-content">
        <router-view v-slot="{ Component }"
          ><transition name="fade" mode="out-in"
            ><keep-alive><component :is="Component" /></keep-alive></transition
        ></router-view>
      </main>
    </div>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100dvh;
  overflow: hidden;
  background: var(--surface);
}
.app-sider {
  width: 208px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--sidebar);
  border-right: 1px solid var(--line);
  padding: 0 12px;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 72px;
  padding: 16px 10px;
  border: 0;
  background: transparent;
  text-align: left;
  color: var(--zinc-900);
  cursor: pointer;
}
.brand-mark {
  width: 30px;
  height: 30px;
  object-fit: contain;
  border-radius: 7px;
}
.brand strong {
  font-size: 16px;
  font-weight: 600;
}
.sidebar-nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.nav-item {
  width: 100%;
  min-height: 38px;
  display: flex;
  align-items: center;
  gap: 10px;
  background: transparent;
  border: 0;
  border-radius: 6px;
  padding: 0 10px;
  color: var(--zinc-700);
  font-size: 13px;
  cursor: pointer;
  text-align: left;
  transition: background 150ms ease, color 150ms ease;
}
.nav-item > .n-icon {
  color: var(--zinc-500);
}
.nav-item:hover {
  background: var(--hover);
  color: var(--zinc-900);
}
.nav-item.active {
  background: var(--selection);
  color: var(--accent);
  font-weight: 600;
}
.nav-item.active > .n-icon {
  color: inherit;
}
.nav-count {
  margin-left: auto;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.sidebar-bottom {
  margin-top: auto;
  padding: 16px 0;
}
.app-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.app-header {
  height: 52px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 0 24px;
  border-bottom: 1px solid var(--line);
  background: var(--surface);
  user-select: none;
}
.header-location,
.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.search-input {
  width: 240px;
  font-size: 13px;
}
.sidebar-toggle {
  display: none;
}
.app-content {
  flex: 1;
  overflow: auto;
  padding: 24px;
}
.search-panel {
  width: min(320px, 75vw);
  max-height: 340px;
  overflow: auto;
  padding: 8px 0;
}
.search-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 10px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--accent);
  text-align: left;
  cursor: pointer;
}
.search-item:hover {
  background: var(--hover);
}
.search-item-name {
  flex: 1;
  color: var(--zinc-900);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sidebar-scrim {
  display: none;
}
@media (min-width: 1600px) {
  .app-content {
    padding: 32px;
  }
  .app-header {
    padding: 0 32px;
  }
}
@media (max-width: 900px) {
  .app-sider {
    width: 184px;
  }
}
@media (max-width: 680px) {
  .app-sider {
    position: fixed;
    inset: 0 auto 0 0;
    width: 208px;
    z-index: 102;
    visibility: hidden;
    transform: translateX(-100%);
    transition: transform 180ms ease, visibility 180ms;
  }
  .app-sider.is-open {
    visibility: visible;
    transform: translateX(0);
  }
  .sidebar-scrim {
    display: block;
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    border: 0;
    z-index: 101;
  }
  .sidebar-toggle {
    display: inline-flex;
  }
  .app-header {
    padding: 0 12px;
    gap: 8px;
  }
  .header-location,
  .header-right {
    gap: 4px;
  }
  .search-input {
    width: clamp(72px, calc(100vw - 248px), 240px);
  }
  .app-content {
    padding: 20px 16px;
  }
}
</style>
