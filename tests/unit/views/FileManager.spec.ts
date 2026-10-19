import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, shallowMount, type VueWrapper } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { NDropdown } from 'naive-ui';
import { useFilesStore } from '@/stores/files';
import type { FileNode } from '@/types';

vi.mock('naive-ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('naive-ui')>()),
  useMessage: () => ({ success: vi.fn(), warning: vi.fn(), error: vi.fn(), info: vi.fn() }),
  useDialog: () => ({ warning: vi.fn() }),
}));
vi.mock('@/api', () => ({
  api: { listFiles: vi.fn().mockResolvedValue([]), ancestors: vi.fn().mockResolvedValue([]) },
  formatSize: (size: number) => `${size} B`,
  revealInFolder: vi.fn(),
}));

import FileManager from '@/views/FileManager.vue';

function file(id: number, name: string, mime = '', isDir = false, size = 10): FileNode {
  return {
    id,
    name,
    mime,
    is_dir: isDir,
    size,
    parent_id: null,
    path: name,
    created_at: '2026-10-19T00:00:00Z',
    updated_at: `2026-10-${String(id).padStart(2, '0')}T00:00:00Z`,
  };
}

let wrapper: VueWrapper;
function mountView() {
  const pinia = createPinia();
  const store = useFilesStore(pinia);
  wrapper = shallowMount(FileManager, {
    global: {
      plugins: [pinia],
      stubs: {
        Spin: { template: '<div><slot /></div>' },
        Tooltip: { template: '<span><slot name="trigger" /></span>' },
      },
    },
  });
  store.items = [
    file(1, 'photo.JPG'),
    file(2, 'notes.pdf'),
    file(3, 'clip.bin', 'video/mp4'),
    file(4, 'folder', '', true),
    file(5, 'file10.zip', '', false, 30),
    file(6, 'file2.zip', '', false, 20),
  ];
  return store;
}
const names = () => wrapper.findAll('.grid-name').map((node) => node.text());

describe('FileManager 文件浏览', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => wrapper?.unmount());

  it('文件夹排在前面，文件名称按自然顺序排列', async () => {
    const store = mountView();
    await flushPromises();
    expect(names()[0]).toBe('folder');
    expect(names().indexOf('file2.zip')).toBeLessThan(names().indexOf('file10.zip'));
    expect(store.items[0].name).toBe('photo.JPG');
  });

  it('按扩展名或 MIME 筛选，并展示真实项目数量', async () => {
    mountView();
    await wrapper.findAll('[role="tab"]')[2].trigger('click');
    expect(names()).toEqual(['photo.JPG']);
    expect(wrapper.find('.file-summary').text()).toContain('1 个项目');
    await wrapper.findAll('[role="tab"]')[3].trigger('click');
    expect(names()).toEqual(['clip.bin']);
  });

  it('大小和修改时间排序仍保留文件夹优先', async () => {
    mountView();
    wrapper.findComponent(NDropdown).vm.$emit('select', 'size');
    await flushPromises();
    expect(names().slice(0, 3)).toEqual(['folder', 'file10.zip', 'file2.zip']);
    wrapper.findComponent(NDropdown).vm.$emit('select', 'modified');
    await flushPromises();
    expect(names().slice(0, 3)).toEqual(['folder', 'file2.zip', 'file10.zip']);
  });

  it('筛选和目录变化会清除旧选择', async () => {
    const store = mountView();
    await flushPromises();
    await wrapper.find('.grid-item').trigger('keydown', { key: ' ' });
    expect(wrapper.find('.file-summary').text()).toContain('已选中 1 项');
    await wrapper.findAll('[role="tab"]')[2].trigger('click');
    expect(wrapper.find('.file-summary').text()).not.toContain('已选中');
    store.currentDir = 4;
    await flushPromises();
    expect(wrapper.find('[role="tab"]').attributes('aria-selected')).toBe('true');
  });

  it('切换列表视图后保存偏好', async () => {
    mountView();
    await wrapper.find('[aria-label="列表视图"]').trigger('click');
    expect(localStorage.getItem('ucd2-view')).toBe('list');
    expect(wrapper.findComponent({ name: 'Table' }).exists()).toBe(true);
  });
});
