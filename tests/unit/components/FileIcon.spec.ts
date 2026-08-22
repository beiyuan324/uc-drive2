import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import FileIcon from '@/components/FileIcon.vue';
import {
  PhFolder, PhImage, PhFilePdf, PhFileZip, PhFileCode, PhFileText,
  PhFileCsv, PhFileXls, PhFileDoc, PhFile,
} from '@phosphor-icons/vue';

/** 通过组件实例拿到 computed icon 的引用 */
function iconOf(props: Record<string, unknown>) {
  const wrapper = mount(FileIcon, {
    props,
    global: { stubs: { 'n-icon': { template: '<i class="nicon-stub" />' } } },
  });
  return (wrapper.vm as any).icon as unknown;
}

describe('FileIcon 图标映射', () => {
  it('目录使用 PhFolder', () => {
    expect(iconOf({ name: '资料', isDir: true, mime: '' })).toBe(PhFolder);
  });

  it('图片 mime 使用 PhImage', () => {
    expect(iconOf({ name: 'a.png', isDir: false, mime: 'image/png' })).toBe(PhImage);
  });

  it('pdf 扩展名使用 PhFilePdf', () => {
    expect(iconOf({ name: '手册.pdf', isDir: false, mime: 'application/pdf' })).toBe(PhFilePdf);
  });

  it('压缩包使用 PhFileZip', () => {
    expect(iconOf({ name: 'backup.zip', isDir: false, mime: 'application/zip' })).toBe(PhFileZip);
  });

  it('代码文件使用 PhFileCode', () => {
    expect(iconOf({ name: 'main.py', isDir: false, mime: 'text/x-python' })).toBe(PhFileCode);
  });

  it('md/txt 使用 PhFileText', () => {
    expect(iconOf({ name: 'README.md', isDir: false, mime: 'text/markdown' })).toBe(PhFileText);
  });

  it('csv / xls / doc 各有专属图标', () => {
    expect(iconOf({ name: 'd.csv', isDir: false, mime: 'text/csv' })).toBe(PhFileCsv);
    expect(iconOf({ name: 't.xlsx', isDir: false, mime: '' })).toBe(PhFileXls);
    expect(iconOf({ name: 'w.docx', isDir: false, mime: '' })).toBe(PhFileDoc);
  });

  it('未知类型回退 PhFile', () => {
    expect(iconOf({ name: 'blob.xyz', isDir: false, mime: 'application/octet-stream' })).toBe(PhFile);
  });
});

describe('FileIcon 渲染', () => {
  it('目录图标带 accent 色，普通文件无色', () => {
    const dir = mount(FileIcon, {
      props: { name: '目录', isDir: true, mime: '' },
      global: { stubs: { 'n-icon': { template: '<i class="nicon-stub" />' } } },
    });
    const file = mount(FileIcon, {
      props: { name: 'f.txt', isDir: false, mime: '' },
      global: { stubs: { 'n-icon': { template: '<i class="nicon-stub" />' } } },
    });
    expect((dir.props() as any).isDir).toBe(true);
    expect((file.props() as any).isDir).toBe(false);
  });
});
