<script setup lang="ts">
import { ref, watch } from 'vue';
import { api } from '@/api';
import FileIcon from './FileIcon.vue';
import type { FileNode } from '@/types';

const props = defineProps<{ node: FileNode }>();
const imageUrl = ref('');
const failed = ref(false);
watch(
  () => [props.node.id, props.node.mime],
  async () => {
    imageUrl.value = '';
    failed.value = false;
    if (!props.node.is_dir && props.node.mime.startsWith('image/')) {
      const id = props.node.id;
      const url = await api.downloadUrl(id, true).catch(() => '');
      if (props.node.id === id) imageUrl.value = url;
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="file-thumbnail">
    <img
      v-if="imageUrl && !failed"
      :src="imageUrl"
      :alt="node.name"
      loading="lazy"
      @error="failed = true"
    />
    <file-icon
      v-else
      :name="node.name"
      :is-dir="node.is_dir"
      :mime="node.mime"
      :size="node.is_dir ? 65 : 40"
      badge
    />
  </div>
</template>

<style scoped>
.file-thumbnail {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 108px;
  height: 88px;
}
.file-thumbnail img {
  max-width: 104px;
  max-height: 80px;
  object-fit: contain;
  border-radius: 4px;
  box-shadow: 0 2px 7px rgba(0, 0, 0, 0.1);
}
</style>
