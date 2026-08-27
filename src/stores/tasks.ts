import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api';
import type { TaskItem, TaskSource } from '@/types';

export const useTasksStore = defineStore('tasks', () => {
  const tasks = ref<TaskItem[]>([]);
  const loading = ref(false);
  const pollTimer = ref<number | null>(null);

  /** 轮询合并：逐条比对关键字段，全等时保留旧数组引用（组件不重渲染）。
   *  后端仅在状态/进度/速度/错误实际变化时才更新行（updated_at 随之变化），
   *  因此字段全等 = 无变化，直接跳过赋值可避免轮询期间整页任务列表反复 patch。 */
  function applyTasks(next: TaskItem[]) {
    const cur = tasks.value;
    if (cur.length === next.length) {
      let same = true;
      for (let i = 0; i < next.length; i += 1) {
        const a = cur[i];
        const b = next[i];
        if (a.id !== b.id || a.status !== b.status || a.progress !== b.progress
          || a.speed !== b.speed || a.error !== b.error || a.updated_at !== b.updated_at) {
          same = false;
          break;
        }
      }
      if (same) return;
    }
    tasks.value = next;
  }

  async function refresh() {
    try {
      applyTasks(await api.listTasks());
    } catch { /* 后端暂不可达 */ }
  }

  async function create(payload: { source: TaskSource; url?: string; torrentId?: number; torrentName?: string; connections?: number }) {
    const row = await api.createTask(payload);
    await refresh();
    return row;
  }

  async function pause(id: number) {
    await api.pauseTask(id);
    await refresh();
  }

  async function resume(id: number) {
    await api.resumeTask(id);
    await refresh();
  }

  async function remove(id: number, force = false) {
    await api.deleteTask(id, force);
    await refresh();
  }

  /** 2s 轮询任务列表（后端由 gopeed 轮询驱动，前端再拉一次） */
  function startPolling(interval = 2000) {
    stopPolling();
    refresh();
    pollTimer.value = window.setInterval(refresh, interval);
  }

  function stopPolling() {
    if (pollTimer.value != null) {
      clearInterval(pollTimer.value);
      pollTimer.value = null;
    }
  }

  return { tasks, loading, refresh, create, pause, resume, remove, startPolling, stopPolling };
});
