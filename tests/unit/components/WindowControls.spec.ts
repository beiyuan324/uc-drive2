import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import WindowControls from '@/components/WindowControls.vue';

const mocks = vi.hoisted(() => ({
  isTauri: vi.fn(),
  isMaximized: vi.fn(),
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
  onResized: vi.fn(),
  unlisten: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: mocks.isTauri }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => mocks }));

beforeEach(() => {
  vi.resetAllMocks();
  mocks.isTauri.mockReturnValue(true);
  mocks.isMaximized.mockResolvedValue(false);
  mocks.onResized.mockResolvedValue(mocks.unlisten);
});
afterEach(() => vi.restoreAllMocks());

describe('窗口操作', () => {
  it('浏览器模式不显示桌面窗口按钮或调用窗口 API', async () => {
    mocks.isTauri.mockReturnValue(false);
    const wrapper = mount(WindowControls);
    await flushPromises();
    expect(wrapper.find('button').exists()).toBe(false);
    expect(mocks.onResized).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('最小化和关闭使用原生窗口 API', async () => {
    const wrapper = mount(WindowControls);
    await flushPromises();
    await wrapper.get('[aria-label="最小化"]').trigger('click');
    await wrapper.get('[aria-label="关闭窗口"]').trigger('click');
    expect(mocks.minimize).toHaveBeenCalledOnce();
    expect(mocks.close).toHaveBeenCalledOnce();
    wrapper.unmount();
    expect(mocks.unlisten).toHaveBeenCalledOnce();
  });

  it('最大化按钮和外部窗口缩放都能更新还原状态', async () => {
    const wrapper = mount(WindowControls);
    await flushPromises();
    mocks.isMaximized.mockResolvedValue(true);
    await wrapper.get('[aria-label="最大化"]').trigger('click');
    await flushPromises();
    expect(mocks.toggleMaximize).toHaveBeenCalledOnce();
    expect(wrapper.find('[aria-label="还原"]').exists()).toBe(true);
    mocks.isMaximized.mockResolvedValue(false);
    mocks.onResized.mock.calls[0][0]();
    await flushPromises();
    expect(wrapper.find('[aria-label="最大化"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it('组件提前卸载也会移除随后注册成功的监听', async () => {
    let resolveListener!: (unlisten: () => void) => void;
    mocks.onResized.mockReturnValue(new Promise((resolve) => { resolveListener = resolve; }));
    const wrapper = mount(WindowControls);
    wrapper.unmount();
    resolveListener(mocks.unlisten);
    await flushPromises();
    expect(mocks.unlisten).toHaveBeenCalledOnce();
  });
});
