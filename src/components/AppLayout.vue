<script setup lang="ts">
import { computed, watch, h, onMounted, onUnmounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import {
  NLayout, NLayoutSider, NLayoutHeader, NLayoutContent, NMenu, NButton,
  NInput, NIcon, NPopover, NEmpty, NBadge, NTooltip, NDropdown,
} from 'naive-ui';
import type { InputInst } from 'naive-ui';
import {
  PhFiles as FilesIcon, PhHardDrives as HardDrivesIcon, PhGearSix as GearSixIcon,
  PhLinkSimple as LinkSimpleIcon, PhClock as ClockIcon,
  PhMagnifyingGlass as MagnifyingGlassIcon, PhSun as SunIcon, PhMoon as MoonIcon,
  PhMonitor as MonitorIcon, PhCloudArrowUp as CloudArrowUpIcon,
  PhFile as FileGlyph, PhFolder as FolderGlyph,
} from '@phosphor-icons/vue';
import { useSettingsStore } from '@/stores/settings';
import { useFilesStore } from '@/stores/files';
import { useTasksStore } from '@/stores/tasks';
import type { ThemeMode } from '@/types';

const route = useRoute();
const router = useRouter();
const settings = useSettingsStore();
const files = useFilesStore();
const tasks = useTasksStore();

const searchInputRef = ref<InputInst | null>(null);

const activeTasksCount = computed(() =>
  tasks.tasks.filter(t => t.status === 'running' || t.status === 'queued').length,
);

function handleKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    if (route.path !== '/files') {
      router.push('/files').then(() => {
        setTimeout(() => searchInputRef.value?.focus(), 100);
      });
    } else {
      searchInputRef.value?.focus();
    }
  }
}

onMounted(() => {
  settings.load();
  tasks.refresh();
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});

// 全局搜索：输入防抖 250ms
let searchDebounce = 0;
watch(
  () => files.searchQuery,
  () => {
    window.clearTimeout(searchDebounce);
    searchDebounce = window.setTimeout(() => files.doSearch(), 250);
  },
);

const menuOptions = computed(() => [
  { label: '文件', key: '/files', icon: () => h(FilesIcon) },
  { label: 'UC 解析', key: '/parse', icon: () => h(LinkSimpleIcon) },
  {
    label: '离线下载',
    key: '/downloads',
    icon: () => h(HardDrivesIcon),
    extra: () =>
      activeTasksCount.value > 0
        ? h(NBadge, {
            value: activeTasksCount.value,
            type: 'info',
            processing: true,
          })
        : null,
  },
  { label: '历史记录', key: '/history', icon: () => h(ClockIcon) },
  { label: '设置', key: '/settings', icon: () => h(GearSixIcon) },
]);

const themeDropdownOptions = [
  { label: '浅色模式', key: 'light', icon: () => h(SunIcon) },
  { label: '深色模式', key: 'dark', icon: () => h(MoonIcon) },
  { label: '跟随系统', key: 'auto', icon: () => h(MonitorIcon) },
];

const isDark = computed(() => settings.isDark);

function onThemeChange(v: ThemeMode) {
  settings.applyTheme(v);
}
</script>

<template>
  <n-layout class="app-shell" has-sider>
    <n-layout-sider
      bordered
      :width="216"
      :collapsed-width="64"
      collapse-mode="width"
      show-trigger="bar"
      class="app-sider"
    >
      <div class="brand" @click="router.push('/files')">
        <img class="brand-mark" src="@/assets/app-icon.png" alt="uc-drive2" />
        <div class="brand-info">
          <span class="brand-name">uc-drive2</span>
          <span class="brand-ver">v2.0</span>
        </div>
      </div>
      <n-menu
        :value="route.path"
        :options="menuOptions"
        :root-indent="16"
        @update:value="(k) => router.push(k as string)"
      />
    </n-layout-sider>

    <n-layout>
      <n-layout-header bordered class="app-header">
        <div class="header-left">
          <n-popover
            v-if="route.path === '/files'"
            trigger="focus"
            :show="files.searchQuery.length > 0"
            :style="{ maxWidth: '480px' }"
          >
            <template #trigger>
              <n-input
                ref="searchInputRef"
                class="search-input"
                v-model:value="files.searchQuery"
                placeholder="搜索文件…"
                clearable
              >
                <template #prefix>
                  <n-icon :component="MagnifyingGlassIcon" />
                </template>
                <template #suffix>
                  <span class="kbd-hint">Ctrl K</span>
                </template>
              </n-input>
            </template>
            <div class="search-panel">
              <n-empty
                v-if="!files.searching && files.searchResults.length === 0"
                description="没有匹配结果"
                size="small"
              />
              <div
                v-for="r in files.searchResults"
                :key="r.id"
                class="search-item"
                @click="files.openSearchResult(r)"
              >
                <n-icon
                  :component="r.is_dir ? FolderGlyph : FileGlyph"
                  :color="r.is_dir ? 'var(--accent)' : undefined"
                  size="17"
                />
                <span class="search-item-name">{{ r.name }}</span>
                <span class="search-item-path">{{ r.path }}</span>
              </div>
            </div>
          </n-popover>
        </div>
        <div class="header-right">
          <n-button
            v-if="route.path === '/files'"
            type="primary"
            secondary
            size="small"
            @click="files.showUpload = true"
          >
            <template #icon><n-icon><cloud-arrow-up-icon /></n-icon></template>
            上传
          </n-button>
          <n-dropdown
            trigger="click"
            :options="themeDropdownOptions"
            @select="(k) => onThemeChange(k as ThemeMode)"
          >
            <n-tooltip trigger="hover">
              <template #trigger>
                <n-button quaternary circle size="small" aria-label="主题切换">
                  <template #icon>
                    <n-icon :size="18" class="theme-icon">
                      <sun-icon v-if="settings.themeMode === 'light'" />
                      <moon-icon v-else-if="settings.themeMode === 'dark'" />
                      <monitor-icon v-else />
                    </n-icon>
                  </template>
                </n-button>
              </template>
              {{ settings.themeMode === 'light' ? '浅色模式' : settings.themeMode === 'dark' ? '深色模式' : '跟随系统' }}（点击切换）
            </n-tooltip>
          </n-dropdown>
        </div>
      </n-layout-header>

      <n-layout-content class="app-content" content-style="padding: 22px 28px;">
        <router-view v-slot="{ Component }">
          <transition name="fade" mode="out-in">
            <keep-alive>
              <component :is="Component" />
            </keep-alive>
          </transition>
        </router-view>
      </n-layout-content>
    </n-layout>
  </n-layout>
</template>

<style scoped>
.app-shell {
  height: 100vh;
}
.app-sider {
  --n-color: transparent;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 18px 20px 14px;
  cursor: pointer;
  user-select: none;
}
.brand-mark {
  width: 26px;
  height: 26px;
  border-radius: 7px;
  object-fit: contain;
}
.brand-info {
  display: flex;
  align-items: center;
  gap: 6px;
}
.brand-name {
  font-weight: 600;
  font-size: 15px;
  letter-spacing: -0.01em;
}
.brand-ver {
  font-size: 10.5px;
  font-weight: 500;
  color: var(--zinc-500);
  background: var(--zinc-100);
  padding: 1px 5px;
  border-radius: 4px;
  border: 1px solid var(--zinc-200);
}
.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 24px;
  height: 56px;
}
.header-left {
  display: flex;
  align-items: center;
  flex: 1;
  max-width: 420px;
}
.header-right {
  display: flex;
  align-items: center;
  gap: 10px;
}
.search-input {
  width: 100%;
}
.kbd-hint {
  font-size: 11px;
  font-family: inherit;
  color: var(--zinc-400);
  background: var(--zinc-100);
  border: 1px solid var(--zinc-200);
  border-radius: 4px;
  padding: 1px 5px;
  margin-right: 2px;
}
.search-panel {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 4px;
  min-width: 320px;
}
.search-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
  color: var(--zinc-500);
  font-size: 13px;
  transition: background 0.12s ease;
}
.search-item:hover {
  background: var(--zinc-100);
}
.search-item-name {
  color: var(--zinc-900);
  font-weight: 500;
  flex-shrink: 0;
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.search-item-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}
.theme-icon {
  color: var(--zinc-500);
  transition: color 0.15s ease;
}
.theme-icon:hover {
  color: var(--accent);
}
.app-content {
  height: calc(100vh - 56px);
  overflow: auto;
}
</style>
