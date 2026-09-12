import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { defineComponent } from 'vue';

const mocks = vi.hoisted(() => ({
  ucParse: vi.fn(),
  ucListFolder: vi.fn(),
  ucDownload: vi.fn(),
  message: {
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
  dialogWarning: vi.fn(),
  push: vi.fn(),
}));

vi.mock('@/api', () => ({
  api: {
    ucParse: (...args: unknown[]) => mocks.ucParse(...args),
    ucListFolder: (...args: unknown[]) => mocks.ucListFolder(...args),
    ucDownload: (...args: unknown[]) => mocks.ucDownload(...args),
  },
  formatSize: (value: number) => `${value} B`,
}));

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: mocks.push }),
}));

vi.mock('naive-ui', async (importOriginal) => {
  const actual = await importOriginal<typeof import('naive-ui')>();
  return {
    ...actual,
    useMessage: () => mocks.message,
    useDialog: () => ({ warning: mocks.dialogWarning }),
  };
});

import ParseView from '@/views/ParseView.vue';

const NButtonStub = defineComponent({
  name: 'NButton',
  props: { loading: Boolean },
  emits: ['click'],
  template: '<button type="button" :disabled="loading" @click="$emit(\'click\')"><slot /></button>',
});

const NInputStub = defineComponent({
  name: 'NInput',
  props: { value: { type: String, default: '' } },
  emits: ['update:value'],
  template: '<textarea :value="value" @input="$emit(\'update:value\', $event.target.value)" />',
});

const NTagStub = defineComponent({
  name: 'NTag',
  emits: ['click'],
  template: '<span @click="$emit(\'click\')"><slot /></span>',
});

const file = (fid: string, name: string) => ({
  fid,
  name,
  size: 1024,
  file: true,
  format_type: 'zip',
  share_fid_token: `token-${fid}`,
});

function mountView() {
  return mount(ParseView, {
    global: {
      plugins: [createPinia()],
      stubs: {
        NButton: NButtonStub,
        NInput: NInputStub,
        NTag: NTagStub,
        'n-button': NButtonStub,
        'n-input': NInputStub,
        'n-tag': NTagStub,
      },
    },
  });
}

describe('ParseView 批量下载', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    vi.useFakeTimers();
    mocks.ucParse.mockResolvedValue({
      shareId: 'share-1',
      shareLink: 'https://drive.uc.cn/s/share-1',
      session: { stoken: 'stoken', ctoken: 'ctoken', cookies: 'ctoken=ctoken' },
      cookieUsed: true,
    });
    mocks.ucListFolder.mockResolvedValue({
      files: [file('fid-1', 'a.zip'), file('fid-2', 'b.zip')],
    });
    mocks.dialogWarning.mockResolvedValue(true);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('Cookie 失效时停止批次，不继续请求也不显示成功提示', async () => {
    const wrapper = mountView();
    await wrapper.find('textarea').setValue('https://drive.uc.cn/s/share-1');
    const parseButton = wrapper.findAll('button').find((button) => button.text().includes('开始解析'));
    expect(parseButton).toBeDefined();
    await parseButton!.trigger('click');
    await flushPromises();

    mocks.message.success.mockClear();
    mocks.message.warning.mockClear();
    mocks.message.error.mockClear();
    mocks.ucDownload.mockRejectedValueOnce(
      Object.assign(new Error('Cookie 已失效'), { kind: 'cookie_expired' }),
    );

    const downloadAllButton = wrapper.findAll('button').find((button) => button.text().includes('全部下载'));
    expect(downloadAllButton).toBeDefined();
    await downloadAllButton!.trigger('click');
    await flushPromises();

    expect(mocks.ucDownload).toHaveBeenCalledTimes(1);
    expect(mocks.message.warning).toHaveBeenCalledTimes(1);
    expect(mocks.message.error).not.toHaveBeenCalled();
    expect(mocks.message.success).not.toHaveBeenCalled();
  });

  it('切换分享后，进行中的批次仍使用启动时的分享会话', async () => {
    mocks.ucParse.mockImplementation(async (url: unknown) => {
      const shareId = String(url).endsWith('/share-2') ? 'share-2' : 'share-1';
      return {
        shareId,
        shareLink: String(url),
        session: { stoken: `stoken-${shareId}`, ctoken: `ctoken-${shareId}`, cookies: `ctoken=ctoken-${shareId}` },
        cookieUsed: true,
      };
    });
    mocks.ucListFolder.mockImplementation(async (shareId: unknown) => ({
      files: shareId === 'share-1'
        ? [file('fid-1', 'a.zip'), file('fid-2', 'b.zip')]
        : [file('fid-3', 'c.zip')],
    }));

    let releaseFirst!: () => void;
    mocks.ucDownload
      .mockImplementationOnce(() => new Promise((resolve) => {
        releaseFirst = () => resolve({ id: 1 });
      }))
      .mockResolvedValueOnce({ id: 2 });

    const wrapper = mountView();
    await wrapper.find('textarea').setValue(
      'https://drive.uc.cn/s/share-1\nhttps://drive.uc.cn/s/share-2',
    );
    const parseButton = wrapper.findAll('button').find((button) => button.text().includes('开始解析'));
    await parseButton!.trigger('click');
    await flushPromises();

    const downloadAllButton = wrapper.findAll('button').find((button) => button.text().includes('全部下载'));
    await downloadAllButton!.trigger('click');
    await flushPromises();
    expect(mocks.ucDownload).toHaveBeenCalledTimes(1);

    const secondShare = wrapper.findAll('.link-tag').find((tag) => tag.text().includes('分享 2'));
    expect(secondShare).toBeDefined();
    await secondShare!.trigger('click');
    await flushPromises();

    releaseFirst();
    await flushPromises();

    expect(mocks.ucDownload).toHaveBeenCalledTimes(2);
    expect(mocks.ucDownload.mock.calls[0][0]).toMatchObject({ shareId: 'share-1', stoken: 'stoken-share-1' });
    expect(mocks.ucDownload.mock.calls[1][0]).toMatchObject({ shareId: 'share-1', stoken: 'stoken-share-1' });
  });

  it('确认框等待期间锁定全部下载，避免重复启动批次', async () => {
    let confirm!: (value: boolean) => void;
    mocks.dialogWarning.mockImplementationOnce(() => new Promise((resolve) => {
      confirm = resolve;
    }));
    mocks.ucDownload.mockResolvedValue({ id: 1 });

    const wrapper = mountView();
    await wrapper.find('textarea').setValue('https://drive.uc.cn/s/share-1');
    const parseButton = wrapper.findAll('button').find((button) => button.text().includes('开始解析'));
    await parseButton!.trigger('click');
    await flushPromises();

    const downloadAllButton = wrapper.findAll('button').find((button) => button.text().includes('全部下载'));
    await downloadAllButton!.trigger('click');
    await downloadAllButton!.trigger('click');
    confirm(true);
    await flushPromises();

    expect(mocks.dialogWarning).toHaveBeenCalledTimes(1);
    expect(mocks.ucDownload).toHaveBeenCalledTimes(2);
  });
});
