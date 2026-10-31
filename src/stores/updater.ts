import { computed, ref } from 'vue';
import { defineStore } from 'pinia';
import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { version } from '../../package.json';

interface UpdateInfo {
  version: string;
  notes: string;
  publishedAt: string | null;
}
interface CheckResult {
  currentVersion: string;
  update: UpdateInfo | null;
}
type UpdateEvent =
  | { event: 'progress'; downloaded: number; total: number | null }
  | { event: 'downloaded' }
  | { event: 'error'; message: string };
type Phase = 'idle' | 'checking' | 'upToDate' | 'available' | 'downloading' | 'downloaded' | 'installing' | 'error';
type InstallResult = { status: 'blocked'; reason: string } | { status: 'installed' };

export const useUpdaterStore = defineStore('updater', () => {
  const desktop = isTauri();
  const canInstall = desktop && !import.meta.env.DEV;
  const autoCheck = ref(localStorage.getItem('ucd2-auto-update') !== 'false');
  const currentVersion = ref(version);
  const update = ref<UpdateInfo | null>(null);
  const phase = ref<Phase>('idle');
  const error = ref('');
  const blockedReason = ref('');
  const downloadedBytes = ref(0);
  const totalBytes = ref<number | null>(null);
  const lastCheckedAt = ref('');
  const promptRequest = ref(0);
  const seenVersions = new Set<string>();
  const busy = computed(() => ['checking', 'downloading', 'installing'].includes(phase.value));
  const progress = computed(() => totalBytes.value
    ? Math.min(100, Math.floor(downloadedBytes.value / totalBytes.value * 100)) : 0);
  let startupTimer: number | undefined;
  let intervalTimer: number | undefined;

  function setAutoCheck(value: boolean) {
    autoCheck.value = value;
    localStorage.setItem('ucd2-auto-update', String(value));
  }

  function promptForDownload() {
    if (canInstall && update.value && phase.value === 'available') promptRequest.value += 1;
  }

  async function check(manual = true): Promise<boolean> {
    if (!desktop || busy.value || phase.value === 'downloaded') return false;
    const previous = update.value ? 'available' : 'idle';
    phase.value = 'checking';
    error.value = '';
    try {
      const result = await invoke<CheckResult>('check_app_update');
      currentVersion.value = result.currentVersion;
      update.value = result.update;
      lastCheckedAt.value = new Date().toLocaleString('zh-CN');
      phase.value = result.update ? 'available' : 'upToDate';
      if (!manual && result.update && !seenVersions.has(result.update.version)) {
        seenVersions.add(result.update.version);
        promptForDownload();
      }
      return true;
    } catch (e) {
      error.value = String(e);
      phase.value = previous === 'available' ? previous : 'error';
      return false;
    }
  }

  async function install(): Promise<void> {
    if (!canInstall || phase.value !== 'downloaded') return;
    phase.value = 'installing';
    error.value = '';
    blockedReason.value = '';
    try {
      const result = await invoke<InstallResult>('install_app_update');
      if (result.status === 'blocked') {
        blockedReason.value = result.reason;
        phase.value = 'downloaded';
      }
    } catch (e) {
      error.value = String(e);
      phase.value = 'downloaded';
    }
  }

  async function download(): Promise<void> {
    if (!canInstall || busy.value || !update.value || phase.value === 'downloaded') return;
    phase.value = 'downloading';
    error.value = '';
    blockedReason.value = '';
    downloadedBytes.value = 0;
    totalBytes.value = null;
    const onEvent = new Channel<UpdateEvent>();
    onEvent.onmessage = (event) => {
      if (event.event === 'progress') {
        downloadedBytes.value = event.downloaded;
        totalBytes.value = event.total;
      } else if (event.event === 'error') {
        error.value = event.message;
      }
    };
    try {
      await invoke('download_app_update', { onEvent });
      phase.value = 'downloaded';
      // 后端在同一把安装锁内复核传输状态；忙碌时保留更新，之后仅手动安装。
      await install();
    } catch (e) {
      error.value = String(e);
      phase.value = 'available';
    }
  }

  function start() {
    if (!desktop || startupTimer !== undefined || intervalTimer !== undefined) return;
    startupTimer = window.setTimeout(() => {
      startupTimer = undefined;
      if (autoCheck.value) void check(false);
    }, 10_000);
    intervalTimer = window.setInterval(() => {
      if (autoCheck.value) void check(false);
    }, 6 * 60 * 60 * 1000);
  }

  function stop() {
    window.clearTimeout(startupTimer);
    window.clearInterval(intervalTimer);
    startupTimer = undefined;
    intervalTimer = undefined;
  }

  return {
    desktop, canInstall, autoCheck, currentVersion, update, phase, error, blockedReason,
    downloadedBytes, totalBytes, lastCheckedAt, promptRequest, busy, progress,
    setAutoCheck, promptForDownload, check, download, install, start, stop,
  };
});
