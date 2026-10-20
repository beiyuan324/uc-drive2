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
  PhCloud,
  PhCaretRight,
  PhSidebarSimple,
  PhX,
  PhShieldCheck,
  PhHardDrives,
  PhFile,
} from '@phosphor-icons/vue';
import { useSettingsStore } from '@/stores/settings';
import { useFilesStore } from '@/stores/files';
import { useTasksStore } from '@/stores/tasks';
import type { ThemeMode } from '@/types';
import { version } from '../../package.json';
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
  { label: '我的文件', key: '/files', icon: PhFolderSimple, group: '资料库' },
  { label: '离线下载', key: '/downloads', icon: PhDownloadSimple, group: '传输' },
  { label: '链接解析', key: '/parse', icon: PhLinkSimple },
  { label: '历史记录', key: '/history', icon: PhClockCounterClockwise },
];
const pageTitle = computed(
  () => navigation.find((item) => item.key === route.path)?.label || '设置',
);
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
      <button class="brand" @click="navigate('/files')">
        <img class="brand-mark" src="@/assets/app-icon.png" alt="" />
        <span class="brand-info"
          ><strong>uc-drive<span class="brand-two">2</span></strong
          ><span>你的个人文件空间</span></span
        >
      </button>
      <nav class="sidebar-nav">
        <template v-for="item in navigation" :key="item.key">
          <div v-if="item.group" class="nav-section-label">{{ item.group }}</div>
          <button
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
        </template>
      </nav>
      <div class="sidebar-bottom">
        <div class="local-space">
          <div class="local-space-icon"><n-icon :component="PhHardDrives" :size="22" /></div>
          <div>
            <strong>本地空间</strong
            ><span>{{ settings.info ? '文件保存在这台电脑' : '正在连接本地服务' }}</span>
          </div>
          <span class="connection-dot" :class="{ connected: settings.info }" />
        </div>
        <div class="sidebar-divider" />
        <button
          class="nav-item"
          :class="{ active: route.path === '/settings' }"
          :aria-current="route.path === '/settings' ? 'page' : undefined"
          @click="navigate('/settings')"
        >
          <n-icon :component="PhGearSix" :size="19" /><span>设置</span
          ><n-icon class="nav-chevron" :component="PhCaretRight" :size="12" />
        </button>
        <div class="sidebar-footnote">
          <n-icon :component="PhShieldCheck" :size="13" /><span>私密存储，尽在本机</span>
        </div>
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
          <span class="header-location-name" data-tauri-drag-region>{{ pageTitle }}</span>
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
          <span class="header-divider" />
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
      <footer class="app-statusbar">
        <span><n-icon :component="PhCloud" :size="14" />个人空间</span
        ><span>uc-drive2 <span class="status-version">{{ version }}</span></span>
      </footer>
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
  width: 232px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--sidebar);
  border-right: 1px solid var(--line);
  padding: 0 14px;
}
.brand {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 30px 12px 34px;
  border: 0;
  background: transparent;
  text-align: left;
  color: var(--zinc-900);
  cursor: pointer;
}
.brand-mark {
  width: 38px;
  height: 38px;
  object-fit: contain;
  border-radius: 8px;
}
.brand-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.brand-info strong {
  font-size: 20px;
  font-weight: 650;
  line-height: 1.2;
}
.brand-two {
  color: var(--accent);
  margin-left: 2px;
}
.brand-info > span {
  color: var(--zinc-500);
  font-size: 10px;
}
.nav-section-label {
  color: var(--zinc-500);
  font-size: 11px;
  font-weight: 600;
  padding: 0 13px 9px;
}
.nav-section-label:not(:first-child) {
  padding-top: 27px;
}
.nav-item {
  width: 100%;
  height: 39px;
  display: flex;
  align-items: center;
  gap: 11px;
  background: transparent;
  border: 0;
  border-radius: 6px;
  padding: 0 13px;
  margin-bottom: 4px;
  color: var(--zinc-700);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
  text-align: left;
  transition: background 140ms;
}
.nav-item > .n-icon {
  color: var(--zinc-500);
}
.nav-item:hover {
  background: var(--hover);
}
.nav-item.active {
  background: var(--selection);
  color: var(--accent);
  font-weight: 600;
}
.nav-item.active > .n-icon {
  color: var(--accent);
}
.nav-count {
  margin-left: auto;
  font-size: 11px;
  font-weight: 600;
  color: var(--accent);
}
.nav-chevron {
  margin-left: auto;
}
.sidebar-bottom {
  margin-top: auto;
  padding: 24px 0 16px;
}
.local-space {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 10px 20px;
}
.local-space-icon {
  color: var(--zinc-500);
}
.local-space strong,
.local-space span {
  display: block;
}
.local-space strong {
  font-size: 12px;
  font-weight: 550;
}
.local-space div > span {
  font-size: 10px;
  margin-top: 4px;
  color: var(--zinc-500);
}
.connection-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--zinc-400);
  margin-left: auto;
  flex-shrink: 0;
}
.connection-dot.connected {
  background: #34c759;
}
.sidebar-divider {
  height: 1px;
  background: var(--line);
  margin: 0 9px 14px;
}
.sidebar-footnote {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  margin-top: 19px;
  color: var(--zinc-500);
  font-size: 10px;
}
.app-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.app-header {
  height: 70px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 0 36px;
  border-bottom: 1px solid var(--line);
  background: var(--surface-glass);
  backdrop-filter: blur(20px);
  user-select: none;
}
.header-location,
.header-right {
  display: flex;
  align-items: center;
  gap: 16px;
  min-width: 0;
}
.header-location-name {
  font-size: 12px;
  font-weight: 550;
  color: var(--zinc-600);
  white-space: nowrap;
}
.search-input {
  width: 236px;
  --n-color: var(--zinc-100) !important;
  --n-border: 1px solid transparent !important;
  --n-height: 30px !important;
  font-size: 12px;
}
.header-divider {
  height: 18px;
  width: 1px;
  background: var(--line);
}
.sidebar-toggle {
  display: none;
}
.app-content {
  flex: 1;
  overflow: auto;
  padding: 34px 36px 24px;
}
.app-statusbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 34px;
  padding: 0 36px;
  border-top: 1px solid var(--line);
  color: var(--zinc-500);
  font-size: 10px;
  flex-shrink: 0;
}
.app-statusbar > span {
  display: flex;
  align-items: center;
  gap: 6px;
}
.status-version {
  color: var(--zinc-400);
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
    padding: 40px 48px 24px;
  }
  .app-header,
  .app-statusbar {
    padding-left: 48px;
    padding-right: 48px;
  }
}
@media (max-width: 900px) {
  .app-sider {
    width: 205px;
  }
  .app-content {
    padding: 26px 24px;
  }
  .app-header,
  .app-statusbar {
    padding-left: 24px;
    padding-right: 24px;
  }
}
@media (max-width: 680px) {
  .app-sider {
    position: fixed;
    inset: 0 auto 0 0;
    width: 232px;
    z-index: 102;
    visibility: hidden;
    transform: translateX(-100%);
    transition:
      transform 180ms ease,
      visibility 180ms;
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
    backdrop-filter: blur(3px);
  }
  .sidebar-toggle {
    display: inline-flex;
  }
  .app-header {
    height: 58px;
    padding: 0 16px;
    gap: 8px;
  }
  .header-location,
  .header-right {
    gap: 8px;
  }
  .header-location-name {
    display: none;
  }
  .search-input {
    width: min(230px, 58vw);
  }
  .app-content {
    padding: 24px 18px 18px;
  }
  .app-statusbar {
    padding: 0 18px;
  }
}
</style>
