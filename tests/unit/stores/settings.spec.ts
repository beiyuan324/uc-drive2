import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('@/api', () => ({
  api: {
    settings: vi.fn().mockResolvedValue({
      storageDir: 'C:/data',
      port: 17210,
      gopeed: { running: true, port: 34567 },
      version: '1.0.0',
    }),
  },
}));

import { useSettingsStore } from '@/stores/settings';

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

  it('load() 拉取后端设置', async () => {
    const store = useSettingsStore();
    await store.load();
    expect(store.info).not.toBeNull();
    expect(store.info!.gopeed.running).toBe(true);
    expect(store.info!.port).toBe(17210);
  });

  it('load() 失败时 info 保持 null 不抛错', async () => {
    const { api } = await import('@/api');
    (api.settings as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('后端不可达'));
    const store = useSettingsStore();
    await store.load();
    expect(store.info).toBeNull();
  });
});
