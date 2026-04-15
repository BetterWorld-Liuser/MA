<template>
  <section class="context-section">
    <div class="context-section-header">
      <h3 class="context-section-title">Hints</h3>
      <span class="context-section-meta">{{ hints.length }}</span>
    </div>
    <div class="space-y-0.5">
      <div v-for="hint in hints" :key="`${hint.source}-${hint.content}`" class="context-row-quiet items-start">
        <div class="min-w-0 flex-1 space-y-0.5">
          <div class="flex items-center gap-2">
            <span class="shrink-0 rounded-full bg-bg-tertiary px-1.5 py-0.5 text-[8px] uppercase tracking-[0.16em] text-text-dim">
              {{ hint.source }}
            </span>
            <p class="truncate text-[10px] text-text">{{ hint.content }}</p>
          </div>
        </div>
        <div class="shrink-0 text-right text-[9px] text-text-dim">
          <p class="font-mono">{{ hint.timeLeft }}</p>
          <p>{{ hint.turnsLeft }}</p>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import type { HintItem } from '@/data/mock';

defineProps<{
  hints: HintItem[];
}>();
</script>
