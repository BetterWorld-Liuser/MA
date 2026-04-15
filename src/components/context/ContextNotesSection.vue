<template>
  <section class="context-section">
    <div class="context-section-summary">
      <div class="context-section-meta">
        <span class="font-mono">{{ `${orderedNotes.length} notes` }}</span>
      </div>
      <button class="pill px-1.5" type="button" :disabled="busy" @click="$emit('add-note')">
        <Icon :icon="plusIcon" class="h-3.5 w-3.5" />
      </button>
    </div>
    <div v-if="orderedNotes.length" class="space-y-0.5">
      <div v-for="note in orderedNotes" :key="note.id" class="group context-row-quiet">
        <div class="min-w-0 flex flex-1 items-baseline gap-2 overflow-hidden">
          <span class="shrink-0 font-mono text-[9px] uppercase tracking-widest text-text-dim">{{ note.id }}</span>
          <p class="truncate text-[10px] text-text">{{ note.content }}</p>
        </div>
        <div class="flex shrink-0 items-center gap-1 opacity-0 transition group-hover:opacity-100">
          <button class="context-icon-button" type="button" :disabled="busy" :title="`Edit ${note.id}`" @click="$emit('edit-note', note.id)">
            <Icon :icon="pencilIcon" class="h-3.5 w-3.5" />
          </button>
          <button class="context-icon-button" type="button" :disabled="busy" :title="`Delete ${note.id}`" @click="$emit('delete-note', note.id)">
            <Icon :icon="xIcon" class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
    <div v-else class="compact-empty">No notes</div>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Icon } from '@iconify/vue';
import pencilIcon from '@iconify-icons/lucide/pencil';
import plusIcon from '@iconify-icons/lucide/plus';
import xIcon from '@iconify-icons/lucide/x';
import type { NoteItem } from '@/data/mock';

const props = defineProps<{
  notes: NoteItem[];
  busy?: boolean;
}>();

defineEmits<{
  'add-note': [];
  'edit-note': [noteId: string];
  'delete-note': [noteId: string];
}>();

const orderedNotes = computed(() =>
  [...props.notes].sort((a, b) => {
    if (a.id.toLowerCase() === 'target') return -1;
    if (b.id.toLowerCase() === 'target') return 1;
    return a.id.localeCompare(b.id);
  }),
);
</script>
