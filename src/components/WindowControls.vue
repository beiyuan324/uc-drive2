<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { PhCopySimple, PhMinus, PhSquare, PhX } from '@phosphor-icons/vue';

const appWindow = isTauri() ? getCurrentWindow() : null;
const maximized = ref(false);
let unlistenResize: UnlistenFn | undefined;
let disposed = false;

async function syncMaximized() {
  if (appWindow) maximized.value = await appWindow.isMaximized();
}

async function control(action: 'minimize' | 'toggleMaximize' | 'close') {
  try {
    await appWindow?.[action]();
    if (action === 'toggleMaximize') await syncMaximized();
  } catch (error) {
    console.error('窗口操作失败', error);
  }
}

onMounted(async () => {
  if (!appWindow) return;
  try {
    const unlisten = await appWindow.onResized(() => {
      syncMaximized().catch((error) => console.error('读取窗口状态失败', error));
    });
    if (disposed) {
      unlisten();
      return;
    }
    unlistenResize = unlisten;
    await syncMaximized();
  } catch (error) {
    console.error('监听窗口状态失败', error);
  }
});

onUnmounted(() => {
  disposed = true;
  unlistenResize?.();
});
</script>

<template>
  <div v-if="appWindow" class="window-controls" role="group" aria-label="窗口操作">
    <button type="button" aria-label="最小化" title="最小化" @click="control('minimize')">
      <PhMinus :size="16" />
    </button>
    <button
      type="button"
      :aria-label="maximized ? '还原' : '最大化'"
      :title="maximized ? '还原' : '最大化'"
      @click="control('toggleMaximize')"
    >
      <component :is="maximized ? PhCopySimple : PhSquare" :size="14" />
    </button>
    <button
      type="button"
      class="window-close"
      aria-label="关闭窗口"
      title="关闭到托盘"
      @click="control('close')"
    >
      <PhX :size="17" />
    </button>
  </div>
</template>

<style scoped>
.window-controls {
  display: flex;
  align-self: stretch;
  align-items: center;
  flex-shrink: 0;
  gap: 2px;
  margin-left: 4px;
}
.window-controls button {
  display: grid;
  place-items: center;
  width: 36px;
  height: 32px;
  padding: 0;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--zinc-600);
  cursor: pointer;
}
.window-controls button:hover {
  background: var(--hover);
  color: var(--zinc-900);
}
.window-controls button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.window-controls .window-close:hover {
  background: #c42b1c;
  color: #ffffff;
}
</style>
