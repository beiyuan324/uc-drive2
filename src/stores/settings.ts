import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api';
import type { DownloadConfig, SettingsInfo, ThemeMode } from '@/types';

export const DEFAULT_DOWNLOAD_CONFIG: DownloadConfig = {
  ucConnections: 300,
  httpConnections: 0,
  maxRunning: 3,
};

export const useSettingsStore = defineStore('settings', () => {
  const themeMode = ref<ThemeMode>(localStorage.getItem('ucd2-theme') as ThemeMode || 'auto');
  const info = ref<SettingsInfo | null>(null);
  const downloadConfig = ref<DownloadConfig>({ ...DEFAULT_DOWNLOAD_CONFIG });

  function applyTheme(mode: ThemeMode) {
    themeMode.value = mode;
    localStorage.setItem('ucd2-theme', mode);
    const dark = mode === 'dark' || (mode === 'auto' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  }

  function systemThemeListener() {
    if (themeMode.value === 'auto') {
      applyTheme('auto');
    }
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

  return { themeMode, info, downloadConfig, applyTheme, systemThemeListener, load, saveDownloadConfig };
});
