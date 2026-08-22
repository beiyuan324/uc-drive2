import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api';
import type { SettingsInfo, ThemeMode } from '@/types';

export const useSettingsStore = defineStore('settings', () => {
  const themeMode = ref<ThemeMode>(localStorage.getItem('ucd2-theme') as ThemeMode || 'auto');
  const info = ref<SettingsInfo | null>(null);

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
    } catch { /* 后端暂不可达 */ }
  }

  return { themeMode, info, applyTheme, systemThemeListener, load };
});
