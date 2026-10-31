<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue';
import { useDialog, useMessage } from 'naive-ui';
import { useUpdaterStore } from '@/stores/updater';

const updates = useUpdaterStore();
const dialog = useDialog();
const message = useMessage();

watch(() => updates.promptRequest, () => {
  if (!updates.update) return;
  dialog.info({
    title: `发现新版本 ${updates.update.version}`,
    content: `${updates.update.notes || '新版本已发布。'}\n\n下载并校验后，空闲时将安装并重启；传输进行中会保留更新，待你手动安装。`,
    positiveText: '下载更新',
    negativeText: '稍后',
    onPositiveClick: () => { void updates.download(); },
  });
});
watch(() => updates.blockedReason, (reason) => {
  if (reason) message.info(`更新已下载。${reason}`, { duration: 6000 });
});
watch(() => updates.error, (error) => {
  if (error && ['available', 'downloaded'].includes(updates.phase)) message.error(error, { duration: 6000 });
});
onMounted(updates.start);
onUnmounted(updates.stop);
</script>

<template><span hidden aria-hidden="true" /></template>
