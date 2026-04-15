<template>
  <header
    data-tauri-drag-region
    class="flex h-10 items-center border-b border-border bg-bg-secondary/90 select-none backdrop-blur-sm"
  >
    <div data-tauri-drag-region class="flex min-w-0 items-center gap-2.5 px-4">
      <span
        class="inline-flex h-5 w-5 items-center justify-center rounded-md border border-accent/40 bg-accent-dim text-[11px] font-semibold text-accent"
      >
        M
      </span>
      <div data-tauri-drag-region class="min-w-0">
        <p class="truncate text-[12px] font-semibold tracking-[0.01em] text-text">{{ title }}</p>
      </div>
    </div>
    <div data-tauri-drag-region class="min-w-0 flex-1"></div>
    <div class="flex shrink-0 items-center">
      <button class="titlebar-button" type="button" aria-label="Minimize" data-no-drag @click="$emit('minimize')">
        <Icon :icon="minusIcon" class="titlebar-icon" />
      </button>
      <button class="titlebar-button" type="button" aria-label="Maximize" data-no-drag @click="$emit('toggle-maximize')">
        <Icon :icon="isMaximized ? copyIcon : squareIcon" class="titlebar-icon" />
      </button>
      <button
        class="titlebar-button titlebar-button-close"
        type="button"
        aria-label="Close"
        data-no-drag
        @click="$emit('close')"
      >
        <Icon :icon="xIcon" class="titlebar-icon titlebar-icon-close" />
      </button>
    </div>
  </header>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue';
import minusIcon from '@iconify-icons/lucide/minus';
import copyIcon from '@iconify-icons/lucide/copy';
import squareIcon from '@iconify-icons/lucide/square';
import xIcon from '@iconify-icons/lucide/x';

defineProps<{
  title: string;
  isMaximized: boolean;
}>();

defineEmits<{
  minimize: [];
  'toggle-maximize': [];
  close: [];
}>();
</script>
