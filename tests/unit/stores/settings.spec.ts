import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

const saveDownloadConfig = vi.fn();

vi.mock('@/api', () => ({
  api: {
    settings: vi.fn().mockResolvedValue({
      storageDir: 'C:/data',
      port: 17210,
      gopeed: { running: true, port: 34567 },
      version: '1.0.0',
      download: { ucConnections: 500, httpConnections: 40, maxRunning: 4 },
    }),
    saveDownloadConfig: (...a: unknown[]) => saveDownloadConfig(...a),
  },
}));

import { useSettingsStore, DEFAULT_DOWNLOAD_CONFIG } from '@/stores/settings';

describe('settings store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
    vi.clearAllMocks();
  });

  it('默认主题为 auto 且跟随系统', () => {
    const store = useSettingsStore();
    expect(store.themeMode).toBe('auto');
    store.applyTheme('auto');
    const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    expect(document.documentElement.dataset.theme).toBe(dark ? 'dark' : 'light');
    expect(localStorage.getItem('ucd2-theme')).toBe('auto');
  });

  it('applyTheme 切换 dark 并写入 localStorage', () => {
    const store = useSettingsStore();
    store.applyTheme('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(localStorage.getItem('ucd2-theme')).toBe('dark');
  });

  it('applyTheme 切换 light', () => {
    const store = useSettingsStore();
    store.applyTheme('light');
    expect(document.documentElement.dataset.theme).toBe('light');
  });

  it('isDark 响应式：light→dark 切换后立即更新（回归：曾因读 DOM 属性不响应而失效）', () => {
    const store = useSettingsStore();
    store.applyTheme('light');
    expect(store.isDark).toBe(false);
    expect(document.documentElement.dataset.theme).toBe('light');
    store.applyTheme('dark');
    expect(store.isDark).toBe(true);
    expect(document.documentElement.dataset.theme).toBe('dark');
    store.applyTheme('light');
    expect(store.isDark).toBe(false);
  });

  it('auto 模式跟随系统偏好', () => {
    const store = useSettingsStore();
    const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    store.applyTheme('auto');
    expect(store.isDark).toBe(dark);
    // 模拟系统偏好变化：auto 下应跟随，显式 light/dark 下不跟随
    store.systemThemeListener({ matches: !dark } as MediaQueryListEvent);
    expect(store.isDark).toBe(!dark);
    store.applyTheme('light');
    store.systemThemeListener({ matches: true } as MediaQueryListEvent);
    expect(store.isDark).toBe(false);
  });

  it('load() 拉取后端设置', async () => {
    const store = useSettingsStore();
    await store.load();
    expect(store.info).not.toBeNull();
    expect(store.info!.gopeed.running).toBe(true);
    expect(store.info!.port).toBe(17210);
  });

  it('load() 同步后端下载参数', async () => {
    const store = useSettingsStore();
    // 默认值
    expect(store.downloadConfig.ucConnections).toBe(DEFAULT_DOWNLOAD_CONFIG.ucConnections);
    await store.load();
    expect(store.downloadConfig.ucConnections).toBe(500);
    expect(store.downloadConfig.httpConnections).toBe(40);
    expect(store.downloadConfig.maxRunning).toBe(4);
  });

  it('load() 失败时 info 保持 null 不抛错', async () => {
    const { api } = await import('@/api');
    (api.settings as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('后端不可达'));
    const store = useSettingsStore();
    await store.load();
    expect(store.info).toBeNull();
  });

  it('saveDownloadConfig() 提交后端并同步本地', async () => {
    saveDownloadConfig.mockResolvedValue({ ucConnections: 800, httpConnections: 0, maxRunning: 6 });
    const store = useSettingsStore();
    const cfg = await store.saveDownloadConfig({ ucConnections: 800, maxRunning: 6 });
    expect(saveDownloadConfig).toHaveBeenCalledWith({ ucConnections: 800, maxRunning: 6 });
    expect(cfg.ucConnections).toBe(800);
    expect(store.downloadConfig.maxRunning).toBe(6);
  });
});
