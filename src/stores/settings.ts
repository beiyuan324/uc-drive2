import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { api } from '@/api';
import type { DownloadConfig, SettingsInfo, ThemeMode } from '@/types';

export const DEFAULT_DOWNLOAD_CONFIG: DownloadConfig = {
  ucConnections: 300,
  httpConnections: 0,
  maxRunning: 3,
};

/** 系统深色偏好（响应式，随 matchMedia change 更新） */
const darkMedia = window.matchMedia('(prefers-color-scheme: dark)');

function resolveDark(mode: ThemeMode, systemDark: boolean): boolean {
  return mode === 'dark' || (mode === 'auto' && systemDark);
}

export const useSettingsStore = defineStore('settings', () => {
  const themeMode = ref<ThemeMode>((localStorage.getItem('ucd2-theme') as ThemeMode) || 'auto');
  const systemDark = ref(darkMedia.matches);

  // 由响应式 state 推导，切换主题时模板/计算属性能真正触发重渲染
  const isDark = computed(() => resolveDark(themeMode.value, systemDark.value));

  // 挂载前就同步 <html data-theme>，避免浅色闪屏；后续 applyTheme 更新
  document.documentElement.dataset.theme = isDark.value ? 'dark' : 'light';

  const info = ref<SettingsInfo | null>(null);
  const downloadConfig = ref<DownloadConfig>({ ...DEFAULT_DOWNLOAD_CONFIG });

  function applyTheme(mode: ThemeMode) {
    themeMode.value = mode;
    localStorage.setItem('ucd2-theme', mode);
    document.documentElement.dataset.theme = isDark.value ? 'dark' : 'light';
  }

  function systemThemeListener(e: MediaQueryListEvent) {
    systemDark.value = e.matches;
    if (themeMode.value === 'auto') applyTheme('auto');
  }

  async function load() {
    try {
      info.value = await api.settings();
      if (info.value?.download) downloadConfig.value = { ...DEFAULT_DOWNLOAD_CONFIG, ...info.value.download };
    } catch { /* 后端暂不可达 */ }
  }

  /** 保存下载参数（后端持久化并应用到 gopeed），成功后同步本地 */
  async function saveDownloadConfig(patch: Partial<DownloadConfig>) {
    const cfg = await api.saveDownloadConfig(patch);
    downloadConfig.value = { ...DEFAULT_DOWNLOAD_CONFIG, ...cfg };
    return cfg;
  }

  /** 切换网盘存储目录（后端迁移文件并持久化），成功后同步本地信息 */
  async function saveStorageDir(dir: string, moveFiles = true) {
    const next = await api.setStorageDir(dir, moveFiles);
    if (next) info.value = next;
    return next;
  }

  return { themeMode, systemDark, isDark, info, downloadConfig, applyTheme, systemThemeListener, load, saveDownloadConfig, saveStorageDir };
});
