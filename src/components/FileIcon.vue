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
</script>

<template>
  <n-icon :component="icon" :size="size ?? 20" :color="isDir ? 'var(--accent)' : undefined" />
</template>
