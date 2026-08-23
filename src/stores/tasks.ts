import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api';
import type { TaskItem, TaskSource } from '@/types';

export const useTasksStore = defineStore('tasks', () => {
  const tasks = ref<TaskItem[]>([]);
  const loading = ref(false);
  const pollTimer = ref<number | null>(null);

  async function refresh() {
    try {
      tasks.value = await api.listTasks();
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
