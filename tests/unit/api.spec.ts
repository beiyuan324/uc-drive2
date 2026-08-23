import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// 非 Tauri 环境：invoke 直接抛错，走探测逻辑
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockRejectedValue(new Error('no tauri')),
}));

import { getBase, resetBase, api } from '@/api';

describe('api base 探测（启动竞态回归）', () => {
  const healthOk = { ok: true, gopeed: true, version: '1.0.0' };
  const jsonRes = (body: unknown, ok = true) =>
    ({ ok, status: ok ? 200 : 500, json: async () => body }) as Response;

  beforeEach(() => {
    resetBase();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    resetBase();
  });

  it('后端延迟就绪时：先失败后成功的探测最终返回正确 base', async () => {
    // 第一轮并行探测（20 个端口）全部失败（后端未起），之后全部成功
    let calls = 0;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      calls += 1;
      if (calls <= 20) throw new TypeError('Failed to fetch');
      return jsonRes(healthOk);
    });

    const p = getBase();
    // 推进：第一轮探测失败 + 500ms sleep 后第二轮成功
    await vi.advanceTimersByTimeAsync(2000);
    expect(await p).toBe('http://127.0.0.1:17210');
    expect(fetchMock).toHaveBeenCalled();
  });

  it('req 遇到网络错误自动重试（重置 base 缓存后成功）', async () => {
    let calls = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      calls += 1;
      const url = String(input);
      if (url.includes('/api/health')) return jsonRes(healthOk);
      if (calls <= 2) throw new TypeError('Failed to fetch');
      return jsonRes([]); // GET /api/files 返回空列表
    });

    const p = api.listFiles(null);
    await vi.advanceTimersByTimeAsync(3000);
    await expect(p).resolves.toEqual([]);
    expect(calls).toBeGreaterThan(2); // health + 失败1次 + 重试成功
  });

  it('业务错误（4xx/5xx）不重试，直接抛出', async () => {
    let fileCalls = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const url = String(input);
      if (url.includes('/api/health')) return jsonRes(healthOk);
      if (url.includes('/api/files')) {
        fileCalls += 1;
        return jsonRes({ error: '目录不存在' }, false);
      }
      return jsonRes(healthOk);
    });

    const p = api.listFiles(null);
    // 先挂上断言再推进时间，避免 promise 提前 reject 触发 unhandled rejection
    const assertion = expect(p).rejects.toThrow('目录不存在');
    await vi.advanceTimersByTimeAsync(1000);
    await assertion;
    expect(fileCalls).toBe(1); // 业务请求只发一次，不因 4xx 重试
  });
});
