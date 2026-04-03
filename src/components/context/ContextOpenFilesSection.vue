<template>
  <div class="space-y-1.5">
    <h3 class="section-title !text-[9px]">Open files</h3>
    <div class="space-y-0.5">
      <div v-for="file in openFiles" :key="file.path" class="group compact-row">
        <button
          v-if="!file.locked"
          class="open-file-glyph shrink-0 opacity-0 transition group-hover:opacity-100"
          type="button"
          :disabled="busy"
          :aria-label="`Close ${fileName(file.path)}`"
          :title="`Close ${fileName(file.path)}`"
          @click="$emit('close-file', file.path)"
        >
          <Icon :icon="xIcon" class="h-3.5 w-3.5 transition-transform duration-150 group-hover:scale-110" />
        </button>

        <div class="min-w-0 flex flex-1 items-center gap-2" :title="file.path">
          <p class="truncate font-mono text-[12px]" :class="freshnessClass(file.freshness)">
            {{ fileName(file.path) }}
          </p>
          <span class="shrink-0 font-mono text-[9px] text-text-dim">{{ file.tokenUsage }} tok</span>
        </div>

        <button
          class="open-file-glyph shrink-0"
          :class="file.locked ? 'open-file-glyph-locked' : 'open-file-glyph-unlocked'"
          type="button"
          :disabled="busy"
          :aria-label="`${file.locked ? 'Unlock' : 'Lock'} ${fileName(file.path)}`"
          :title="`${file.locked ? 'Unlock' : 'Lock'} ${fileName(file.path)}`"
          @click="$emit('toggle-file-lock', file.path, !file.locked)"
        >
          <Icon :icon="file.locked ? lockIcon : unlockIcon" class="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue';
import lockIcon from '@iconify-icons/lucide/lock';
import unlockIcon from '@iconify-icons/lucide/unlock';
import xIcon from '@iconify-icons/lucide/x';
import type { OpenFileItem } from '@/data/mock';

defineProps<{
  openFiles: OpenFileItem[];
  busy?: boolean;
}>();

defineEmits<{
  'toggle-file-lock': [path: string, locked: boolean];
  'close-file': [path: string];
}>();

function freshnessClass(freshness: OpenFileItem['freshness']) {
  if (freshness === 'high') return 'text-text';
  if (freshness === 'medium') return 'text-text-muted';
  return 'text-text-dim';
}

function fileName(path: string) {
  const normalized = path.replaceAll('\\', '/');
  const segments = normalized.split('/');
  return segments[segments.length - 1] || normalized;
}
</script>
