<template>
  <div>
    <button
      v-if="entry.type === 'group' && entry.collapsible"
      class="open-files-material-group open-files-material-group-button w-full"
      type="button"
      :style="{ paddingLeft: `${depth * 14}px` }"
      :title="entry.fullPath"
      @click="emit('toggle-group', entry.key)"
    >
      <div class="min-w-0 flex items-center gap-2">
        <span class="open-files-material-arrow text-text-dim">{{ expanded ? '▾' : '▸' }}</span>
        <span class="truncate text-[12px]">{{ entry.name }}</span>
      </div>
      <div class="ml-auto flex shrink-0 items-center gap-2 font-mono text-[10px] text-text-dim">
        <span>{{ `${entry.fileCount} files` }}</span>
        <span>{{ entry.tokenUsage }}</span>
      </div>
    </button>

    <div
      v-else-if="entry.type === 'group'"
      class="open-files-material-group"
      :style="{ paddingLeft: `${depth * 14}px` }"
      :title="entry.fullPath"
    >
      <div class="min-w-0 flex items-center gap-2">
        <span class="truncate text-[12px]">{{ entry.name }}</span>
      </div>
      <div class="ml-auto flex shrink-0 items-center gap-2 font-mono text-[10px] text-text-dim">
        <span v-if="entry.fileCount > 1">{{ `${entry.fileCount} files` }}</span>
        <span>{{ entry.tokenUsage }}</span>
      </div>
    </div>

    <div
      v-else
      class="group open-files-material-file"
      :style="{ paddingLeft: `${depth * 14}px` }"
      :title="entry.fullPath"
    >
      <div class="min-w-0 flex flex-1 items-start gap-2">
        <div class="min-w-0 flex-1">
          <div class="flex min-w-0 items-center gap-1.5">
            <p class="truncate text-[13px] leading-[1.35]" :class="fileNameClass(entry)">
              {{ entry.name }}
            </p>
            <span v-if="entry.state.kind === 'deleted'" class="open-files-material-status">
              Deleted
            </span>
            <span v-else-if="entry.state.kind === 'moved'" class="open-files-material-status">
              → {{ movedLabel(entry.state.newPath) }}
            </span>
          </div>
        </div>
      </div>

      <div class="ml-auto flex shrink-0 items-center gap-2.5">
        <span class="font-mono text-[10px] text-text-dim">{{ entry.tokenUsage }}</span>
        <Icon v-if="entry.locked" :icon="lockIcon" class="open-files-material-lock text-text-dim" />
        <button
          v-if="!entry.locked"
          class="open-file-glyph shrink-0 opacity-0 transition group-hover:opacity-100"
          type="button"
          :disabled="busy"
          :aria-label="`Close ${entry.name}`"
          :title="`Close ${entry.name}`"
          @click.stop="emit('close-file', entry.scope, entry.fullPath)"
        >
          <Icon :icon="xIcon" class="h-3.5 w-3.5 transition-transform duration-150 group-hover:scale-110" />
        </button>
      </div>
    </div>

    <div
      v-if="entry.type === 'group' && (!entry.collapsible || expanded)"
      class="space-y-0.5"
    >
      <OpenFilesMaterialEntry
        v-for="child in entry.children"
        :key="child.key"
        :entry="child"
        :depth="depth + 1"
        :busy="busy"
        :expanded-keys="expandedKeys"
        @toggle-group="emit('toggle-group', $event)"
        @close-file="(scope, path) => emit('close-file', scope, path)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Icon } from '@iconify/vue';
import lockIcon from '@iconify-icons/lucide/lock';
import xIcon from '@iconify-icons/lucide/x';
import type { OpenFilesPanelEntry, OpenFilesPanelFileEntry } from '@/components/context/openFilesPanel';

const props = defineProps<{
  entry: OpenFilesPanelEntry;
  depth: number;
  busy?: boolean;
  expandedKeys: Set<string>;
}>();

const emit = defineEmits<{
  'toggle-group': [key: string];
  'close-file': [scope: string, path: string];
}>();

const expanded = computed(() => props.entry.type === 'group' && props.expandedKeys.has(props.entry.key));

function fileNameClass(entry: OpenFilesPanelFileEntry) {
  if (entry.state.kind === 'deleted' || entry.state.kind === 'moved') {
    return 'text-text-dim line-through';
  }
  if (entry.locked) {
    return 'text-text-muted';
  }
  if (entry.freshness === 'high') {
    return 'text-text';
  }
  if (entry.freshness === 'medium') {
    return 'text-text-muted';
  }
  return 'text-text-dim';
}

function movedLabel(path?: string) {
  if (!path) {
    return '(moved)';
  }
  const normalized = path.replaceAll('\\', '/');
  const segments = normalized.split('/');
  return segments[segments.length - 1] || normalized;
}
</script>
