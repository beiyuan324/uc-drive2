<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue';
import { NConfigProvider, NMessageProvider, NDialogProvider, NNotificationProvider } from 'naive-ui';
import { darkTheme, lightOverrides, darkOverrides, zhCN, dateZhCN } from '@/styles/theme';
import { useSettingsStore } from '@/stores/settings';
import AppLayout from '@/components/AppLayout.vue';

const settings = useSettingsStore();

const isDark = computed(() => document.documentElement.dataset.theme === 'dark');

onMounted(() => {
  settings.applyTheme(settings.themeMode);
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', settings.systemThemeListener);
});

onUnmounted(() => {
  window.matchMedia('(prefers-color-scheme: dark)').removeEventListener('change', settings.systemThemeListener);
});
</script>

<template>
  <n-config-provider
    :theme="isDark ? darkTheme : null"
    :theme-overrides="isDark ? darkOverrides : lightOverrides"
    :locale="zhCN"
    :date-locale="dateZhCN"
  >
    <n-message-provider>
      <n-dialog-provider>
        <n-notification-provider>
          <app-layout />
        </n-notification-provider>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>
