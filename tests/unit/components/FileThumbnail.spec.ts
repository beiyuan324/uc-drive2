import { describe, expect, it, vi } from 'vitest';
import { flushPromises, shallowMount } from '@vue/test-utils';
import { api } from '@/api';
import FileThumbnail from '@/components/FileThumbnail.vue';
import type { FileNode } from '@/types';

vi.mock('@/api', () => ({ api: { downloadUrl: vi.fn().mockResolvedValue('/test-preview.png') } }));
const node: FileNode = {
  id: 1,
  name: 'photo.png',
  mime: 'image/png',
  is_dir: false,
  size: 100,
  path: 'photo.png',
  parent_id: null,
  created_at: '',
  updated_at: '',
};

describe('FileThumbnail 图片预览', () => {
  it('加载本地预览，图片失败时回退到文件图标', async () => {
    const wrapper = shallowMount(FileThumbnail, { props: { node } });
    await flushPromises();
    expect(wrapper.find('img').attributes('src')).toBe('/test-preview.png');
    await wrapper.find('img').trigger('error');
    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.findComponent({ name: 'FileIcon' }).exists()).toBe(true);
    wrapper.unmount();
  });

  it('文件夹不请求图片预览', async () => {
    vi.mocked(api.downloadUrl).mockClear();
    const wrapper = shallowMount(FileThumbnail, { props: { node: { ...node, is_dir: true } } });
    await flushPromises();
    expect(api.downloadUrl).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
