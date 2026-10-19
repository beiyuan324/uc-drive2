<script setup lang="ts">
import { computed } from 'vue';
import { NIcon } from 'naive-ui';
import {
  PhFolder as FolderGlyph,
  PhFolderOpen as FolderOpenGlyph,
  PhFile as FileGlyph,
  PhImage as ImageGlyph,
  PhFilmSlate as FilmGlyph,
  PhMusicNotes as MusicGlyph,
  PhFileZip as ZipGlyph,
  PhFileCode as CodeGlyph,
  PhFilePdf as PdfGlyph,
  PhFileText as TextGlyph,
  PhFileVideo as VideoGlyph,
  PhFileAudio as AudioGlyph,
  PhFileCsv as CsvGlyph,
  PhFileXls as XlsGlyph,
  PhFilePpt as PptGlyph,
  PhFileDoc as DocGlyph,
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
    case 'zip':
    case 'rar':
    case '7z':
    case 'tar':
    case 'gz':
      return ZipGlyph;
    case 'pdf':
      return PdfGlyph;
    case 'js':
    case 'ts':
    case 'html':
    case 'css':
    case 'json':
    case 'xml':
    case 'vue':
    case 'py':
    case 'go':
    case 'rs':
    case 'java':
    case 'c':
    case 'cpp':
    case 'sh':
      return CodeGlyph;
    case 'csv':
      return CsvGlyph;
    case 'xls':
    case 'xlsx':
      return XlsGlyph;
    case 'ppt':
    case 'pptx':
      return PptGlyph;
    case 'doc':
    case 'docx':
      return DocGlyph;
    case 'txt':
    case 'md':
    case 'log':
      return TextGlyph;
    default:
      return FileGlyph;
  }
});

const categoryColor = computed(() => {
  if (props.isDir) return '#55b7ed';
  const ext = props.name.split('.').pop()?.toLowerCase() || '';
  if (props.mime.startsWith('image/')) return '#eab308';
  if (props.mime.startsWith('video/')) return '#f43f5e';
  if (props.mime.startsWith('audio/')) return '#8b5cf6';
  switch (ext) {
    case 'zip':
    case 'rar':
    case '7z':
    case 'tar':
    case 'gz':
      return '#d97706';
    case 'pdf':
      return '#ef4444';
    case 'js':
    case 'ts':
    case 'html':
    case 'css':
    case 'json':
    case 'xml':
    case 'vue':
    case 'py':
    case 'go':
    case 'rs':
    case 'java':
    case 'c':
    case 'cpp':
    case 'sh':
      return '#3b82f6';
    case 'csv':
    case 'xls':
    case 'xlsx':
      return '#10b981';
    case 'ppt':
    case 'pptx':
      return '#f97316';
    case 'doc':
    case 'docx':
      return '#2563eb';
    case 'txt':
    case 'md':
    case 'log':
      return 'var(--zinc-500)';
    default:
      return 'var(--zinc-500)';
  }
});
</script>

<template>
  <div
    v-if="badge"
    class="icon-badge"
    :class="{ 'is-folder': isDir }"
    :style="{ '--cat-color': categoryColor }"
  >
    <n-icon :size="size ?? 28" :color="categoryColor"
      ><component :is="icon" :weight="isDir ? 'fill' : 'regular'"
    /></n-icon>
  </div>
  <n-icon
    v-else
    :component="icon"
    :size="size ?? 20"
    :color="isDir ? 'var(--accent)' : undefined"
  />
</template>

<style scoped>
.icon-badge {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 62px;
  height: 74px;
  border-radius: 5px;
  background: var(--surface);
  border: 1px solid var(--line);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.035);
  transition:
    transform 0.15s ease,
    background 0.15s ease;
}
.icon-badge.is-folder {
  width: 80px;
  height: 74px;
  border: 0;
  box-shadow: none;
  background: transparent;
  filter: drop-shadow(0 3px 2px rgba(38, 130, 190, 0.12));
}
</style>
