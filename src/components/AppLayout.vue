<script setup lang="ts">
import { computed, watch, h, onMounted } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { NLayout, NLayoutSider, NLayoutHeader, NLayoutContent, NMenu, NButton, NInput, NSelect, NIcon, NPopover, NEmpty } from 'naive-ui';
import {
  PhFiles as FilesIcon, PhHardDrives as HardDrivesIcon, PhGearSix as GearSixIcon,
  PhLinkSimple as LinkSimpleIcon, PhClock as ClockIcon,
  PhMagnifyingGlass as MagnifyingGlassIcon, PhGridFour as GridFourIcon, PhList as ListIcon,
  PhSun as SunIcon, PhMoon as MoonIcon, PhCloudArrowUp as CloudArrowUpIcon,
  PhFile as FileGlyph, PhFolder as FolderGlyph,
} from '@phosphor-icons/vue';
import { useSettingsStore } from '@/stores/settings';
import { useFilesStore } from '@/stores/files';
import type { ThemeMode } from '@/types';

const route = useRoute();
const router = useRouter();
const settings = useSettingsStore();
const files = useFilesStore();

onMounted(() => {
  settings.load();
});

// 全局搜索：输入防抖 250ms（每键一声请求会造成后端 LIKE 扫描 + 下拉重渲染抖动）
let searchDebounce = 0;
watch(
  () => files.searchQuery,
  () => {
    window.clearTimeout(searchDebounce);
    searchDebounce = window.setTimeout(() => files.doSearch(), 250);
  },
);

const menuOptions = [
  { label: '文件', key: '/files', icon: () => h(FilesIcon) },
  { label: 'UC 解析', key: '/parse', icon: () => h(LinkSimpleIcon) },
  { label: '离线下载', key: '/downloads', icon: () => h(HardDrivesIcon) },
  { label: '历史记录', key: '/history', icon: () => h(ClockIcon) },
  { label: '设置', key: '/settings', icon: () => h(GearSixIcon) },
];

const themeOptions: { label: string; value: ThemeMode }[] = [
  { label: '浅色', value: 'light' },
  { label: '深色', value: 'dark' },
  { label: '跟随系统', value: 'auto' },
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
      :width="208"
      :collapsed-width="64"
      collapse-mode="width"
      show-trigger="bar"
      class="app-sider"
    >
      <div class="brand">
        <img class="brand-mark" src="@/assets/app-icon.png" alt="uc-drive2" />
        <span class="brand-name">uc-drive2</span>
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
          <n-popover v-if="route.path === '/files'" trigger="focus" :show="files.searchQuery.length > 0" :style="{ maxWidth: '480px' }">
            <template #trigger>
              <n-input
                class="search-input"
                v-model:value="files.searchQuery"
                placeholder="搜索文件…"
                clearable
              >
                <template #prefix>
                  <n-icon :component="MagnifyingGlassIcon" />
                </template>
              </n-input>
            </template>
            <div class="search-panel">
              <n-empty v-if="!files.searching && files.searchResults.length === 0" description="没有匹配结果" size="small" />
              <div
                v-for="r in files.searchResults"
                :key="r.id"
                class="search-item"
                @click="files.openSearchResult(r)"
              >
                <n-icon :component="r.is_dir ? FolderGlyph : FileGlyph" size="16" />
                <span class="search-item-name">{{ r.name }}</span>
                <span class="search-item-path">{{ r.path }}</span>
              </div>
            </div>
          </n-popover>
        </div>
        <div class="header-right">
          <n-button
            v-if="route.path === '/files'"
            secondary
            size="small"
            @click="files.showUpload = true"
          >
            <template #icon><n-icon><cloud-arrow-up-icon /></n-icon></template>
            上传
          </n-button>
          <n-select
            class="theme-select"
            size="small"
            :value="settings.themeMode"
            :options="themeOptions"
            @update:value="onThemeChange"
          />
          <n-icon class="theme-icon" size="18">
            <moon-icon v-if="isDark" />
            <sun-icon v-else />
          </n-icon>
        </div>
      </n-layout-header>

      <n-layout-content class="app-content" content-style="padding: 20px 24px;">
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
}
.brand-mark {
  width: 24px;
  height: 24px;
  border-radius: 6px;
  object-fit: contain;
}
.brand-name {
  font-weight: 600;
  font-size: 15px;
  letter-spacing: -0.01em;
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
  gap: 12px;
}
.search-input {
  width: 100%;
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
.theme-select {
  width: 110px;
}
.theme-icon {
  color: var(--zinc-500);
}
.app-content {
  height: calc(100vh - 56px);
  overflow: auto;
}
</style>
