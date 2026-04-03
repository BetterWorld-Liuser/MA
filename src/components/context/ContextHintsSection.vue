<template>
  <div class="space-y-1.5">
    <h3 class="section-title !text-[9px]">Hints</h3>
    <div class="space-y-0.5">
      <div v-for="hint in hints" :key="`${hint.source}-${hint.content}`" class="compact-row">
        <div class="min-w-0 flex-1">
          <p class="truncate text-[11px] text-text">
            <span class="mr-2 text-[9px] uppercase tracking-widest text-text-dim">{{ hint.source }}</span>{{ hint.content }}
          </p>
        </div>
        <div class="shrink-0 text-right text-[9px] text-text-dim">
          <p class="font-mono">{{ hint.timeLeft }}</p>
          <p>{{ hint.turnsLeft }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { HintItem } from '@/data/mock';

defineProps<{
  hints: HintItem[];
}>();
</script>
