<script setup lang="ts">
import { computed } from 'vue';
import { NIcon } from 'naive-ui';
import {
  PhFolder as FolderGlyph, PhFolderOpen as FolderOpenGlyph, PhFile as FileGlyph,
  PhImage as ImageGlyph, PhFilmSlate as FilmGlyph, PhMusicNotes as MusicGlyph,
  PhFileZip as ZipGlyph, PhFileCode as CodeGlyph, PhFilePdf as PdfGlyph,
  PhFileText as TextGlyph, PhFileVideo as VideoGlyph, PhFileAudio as AudioGlyph,
  PhFileCsv as CsvGlyph, PhFileXls as XlsGlyph, PhFilePpt as PptGlyph, PhFileDoc as DocGlyph,
} from '@phosphor-icons/vue';

const props = defineProps<{
  name: string;
  isDir: boolean;
  mime: string;
  size?: number;
  badge?: boolean;
}>();

const icon = computed(() => {
  if (props.isDir) return FolderGlyph;
  const ext = props.name.split('.').pop()?.toLowerCase() || '';
  if (props.mime.startsWith('image/')) return ImageGlyph;
  if (props.mime.startsWith('video/')) return FilmGlyph;
  if (props.mime.startsWith('audio/')) return MusicGlyph;
  switch (ext) {
    case 'zip': case 'rar': case '7z': case 'tar': case 'gz': return ZipGlyph;
    case 'pdf': return PdfGlyph;
    case 'js': case 'ts': case 'html': case 'css': case 'json': case 'xml': case 'vue': case 'py': case 'go': case 'rs': case 'java': case 'c': case 'cpp': case 'sh': return CodeGlyph;
    case 'csv': return CsvGlyph;
    case 'xls': case 'xlsx': return XlsGlyph;
    case 'ppt': case 'pptx': return PptGlyph;
    case 'doc': case 'docx': return DocGlyph;
    case 'txt': case 'md': case 'log': return TextGlyph;
    default: return FileGlyph;
  }
});

const categoryColor = computed(() => {
  if (props.isDir) return 'var(--accent)';
  const ext = props.name.split('.').pop()?.toLowerCase() || '';
  if (props.mime.startsWith('image/')) return '#eab308';
  if (props.mime.startsWith('video/')) return '#f43f5e';
  if (props.mime.startsWith('audio/')) return '#8b5cf6';
  switch (ext) {
    case 'zip': case 'rar': case '7z': case 'tar': case 'gz': return '#d97706';
    case 'pdf': return '#ef4444';
    case 'js': case 'ts': case 'html': case 'css': case 'json': case 'xml': case 'vue': case 'py': case 'go': case 'rs': case 'java': case 'c': case 'cpp': case 'sh': return '#3b82f6';
    case 'csv': case 'xls': case 'xlsx': return '#10b981';
    case 'ppt': case 'pptx': return '#f97316';
    case 'doc': case 'docx': return '#2563eb';
    case 'txt': case 'md': case 'log': return 'var(--zinc-500)';
    default: return 'var(--zinc-500)';
  }
});
</script>

<template>
  <div v-if="badge" class="icon-badge" :style="{ '--cat-color': categoryColor }">
    <n-icon :component="icon" :size="size ?? 28" :color="categoryColor" />
  </div>
  <n-icon v-else :component="icon" :size="size ?? 20" :color="isDir ? 'var(--accent)' : undefined" />
</template>

<style scoped>
.icon-badge {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--cat-color) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--cat-color) 20%, transparent);
  transition: transform 0.15s ease, background 0.15s ease;
}
</style>
