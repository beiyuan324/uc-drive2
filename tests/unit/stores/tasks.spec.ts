import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

const listTasks = vi.fn();
const createTask = vi.fn();
const pauseTask = vi.fn();
const resumeTask = vi.fn();
const deleteTask = vi.fn();

vi.mock('@/api', () => ({
  api: {
    listTasks: (...a: unknown[]) => listTasks(...a),
    createTask: (...a: unknown[]) => createTask(...a),
    pauseTask: (...a: unknown[]) => pauseTask(...a),
    resumeTask: (...a: unknown[]) => resumeTask(...a),
    deleteTask: (...a: unknown[]) => deleteTask(...a),
  },
}));

import { useTasksStore } from '@/stores/tasks';

const taskFixture: any = {
  id: 1,
  source: 'url',
  name: '测试下载',
  status: 'running',
  progress: 42,
  speed: 123456,
  url: 'https://example.com/a.zip',
  gopeedId: 'gid-1',
};

describe('tasks store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    vi.useFakeTimers();
    listTasks.mockResolvedValue([taskFixture]);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('refresh() 拉取任务列表', async () => {
    const store = useTasksStore();
    await store.refresh();
    expect(store.tasks).toHaveLength(1);
    expect(store.tasks[0].name).toBe('测试下载');
  });

  it('create() 调用 api 并刷新列表', async () => {
    createTask.mockResolvedValue({ ...taskFixture, id: 2 });
    const store = useTasksStore();
    const row = await store.create({ source: 'url', url: 'https://example.com/b.zip' });
    expect(createTask).toHaveBeenCalledWith({ source: 'url', url: 'https://example.com/b.zip' });
    expect(row.id).toBe(2);
    expect(listTasks).toHaveBeenCalledTimes(1);
  });

  it('pause / resume / remove 均转发到 api 并刷新', async () => {
    const store = useTasksStore();
    await store.pause(1);
    expect(pauseTask).toHaveBeenCalledWith(1);
    await store.resume(1);
    expect(resumeTask).toHaveBeenCalledWith(1);
    await store.remove(1, true);
    expect(deleteTask).toHaveBeenCalledWith(1, true);
    expect(listTasks).toHaveBeenCalledTimes(3);
  });

  it('startPolling 立即刷新并按 2s 周期轮询', async () => {
    const store = useTasksStore();
    store.startPolling(2000);
    expect(listTasks).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(6000);
    expect(listTasks).toHaveBeenCalledTimes(4); // 立即 + 3 次周期
    store.stopPolling();
    await vi.advanceTimersByTimeAsync(6000);
    expect(listTasks).toHaveBeenCalledTimes(4); // 停止后不再增加
  });

  it('stopPolling 幂等', () => {
    const store = useTasksStore();
    store.stopPolling();
    store.stopPolling();
    // 停止后轮询不再发生（行为验证）
    expect(listTasks).not.toHaveBeenCalled();
  });
});
