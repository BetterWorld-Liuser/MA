<template>
  <section class="context-section">
    <div class="context-section-summary">
      <div class="context-section-meta">
        <span class="font-mono">{{ `${memories.length} memories` }}</span>
      </div>
      <div class="flex items-center gap-1">
        <button class="pill px-1.5" type="button" :disabled="busy" @click="$emit('add-memory')">
          <Icon :icon="plusIcon" class="h-3.5 w-3.5" />
        </button>
      </div>
    </div>

    <div v-if="memories.length" class="space-y-0.5">
      <div v-for="memory in memories" :key="memory.id" class="group context-row-quiet items-start gap-2">
        <div class="min-w-0 flex-1 space-y-0.5">
          <div class="flex items-center gap-2 overflow-hidden">
            <span class="shrink-0 font-mono text-[9px] uppercase tracking-widest text-text-dim">{{ memory.id }}</span>
            <span class="shrink-0 rounded-full bg-bg-tertiary px-1.5 py-0.5 text-[8px] uppercase tracking-[0.16em] text-text-dim">
              {{ memory.type }}
            </span>
            <span class="truncate text-[9px] uppercase tracking-[0.16em] text-text-muted">{{ memory.topic }}</span>
          </div>
          <p class="text-[10px] leading-snug text-text">{{ memory.title }}</p>
        </div>
        <div class="flex shrink-0 items-center gap-1 opacity-0 transition group-hover:opacity-100">
          <span class="shrink-0 text-[8px] uppercase tracking-[0.16em] text-text-dim">{{ memory.level }}</span>
          <button class="context-icon-button" type="button" :disabled="busy" :title="`Edit ${memory.id}`" @click="$emit('edit-memory', memory.id)">
            <Icon :icon="pencilIcon" class="h-3.5 w-3.5" />
          </button>
          <button class="context-icon-button" type="button" :disabled="busy" :title="`Delete ${memory.id}`" @click="$emit('delete-memory', memory.id)">
            <Icon :icon="xIcon" class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
    <div v-else class="compact-empty">No matched memories</div>

    <div v-if="warnings.length" class="space-y-1 pt-1">
      <p v-for="warning in warnings" :key="warning" class="text-[10px] leading-snug text-warning">
        {{ warning }}
      </p>
    </div>
  </section>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue';
import pencilIcon from '@iconify-icons/lucide/pencil';
import plusIcon from '@iconify-icons/lucide/plus';
import xIcon from '@iconify-icons/lucide/x';
import type { MemoryItem } from '@/data/mock';

defineProps<{
  memories: MemoryItem[];
  warnings: string[];
  busy?: boolean;
}>();

defineEmits<{
  'add-memory': [];
  'edit-memory': [memoryId: string];
  'delete-memory': [memoryId: string];
}>();
</script>
