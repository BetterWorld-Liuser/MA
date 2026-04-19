<template>
  <div>
    <button
      v-if="entry.type === 'group' && entry.collapsible"
      class="open-files-material-group open-files-material-group-button w-full"
      type="button"
      :style="{ paddingLeft: `${4 + depth * 14}px` }"
      :title="entry.fullPath"
      @click="emit('toggle-group', entry.key)"
    >
      <div class="min-w-0 flex items-center gap-2">
        <Icon
          :icon="expanded ? chevronDownIcon : chevronRightIcon"
          class="open-files-material-arrow text-text-dim"
        />
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
      :style="{ paddingLeft: `${28 + depth * 14}px` }"
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
      class="group open-files-material-file cursor-default"
      :style="{ paddingLeft: `${6 + depth * 14}px` }"
      :title="`${entry.fullPath}\n双击用默认程序打开`"
      @dblclick="openInDefaultApp(entry)"
    >
      <button
        class="flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded text-text-dim transition hover:text-text disabled:cursor-not-allowed"
        type="button"
        :disabled="busy"
        :aria-label="entry.locked ? `Unlock ${entry.name}` : `Lock ${entry.name}`"
        :title="entry.locked ? '点击解锁' : '点击锁定'"
        @click.stop="emit('toggle-file-lock', entry.scope, entry.fullPath, !entry.locked)"
      >
        <Icon v-if="entry.locked" :icon="lockIcon" class="h-3 w-3" />
        <Icon v-else :icon="unlockIcon" class="h-3 w-3 opacity-0 transition group-hover:opacity-60" />
      </button>
      <div class="min-w-0 flex flex-1 items-start gap-2">
        <div class="min-w-0 flex-1">
          <div class="flex min-w-0 items-center gap-1.5">
            <p class="truncate text-[11px] leading-[1.35]" :class="fileNameClass(entry)">
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

      <div class="relative ml-auto flex shrink-0 items-center">
        <span class="font-mono text-[10px] text-text-dim transition group-hover:opacity-0">{{ entry.tokenUsage }}</span>
        <button
          v-if="!entry.locked"
          class="absolute inset-0 flex items-center justify-end text-text-dim opacity-0 transition hover:text-text group-hover:opacity-100"
          type="button"
          :disabled="busy"
          :aria-label="`Close ${entry.name}`"
          :title="`Close ${entry.name}`"
          @click.stop="emit('close-file', entry.scope, entry.fullPath)"
        >
          <Icon :icon="xIcon" class="h-3.5 w-3.5" />
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
        @toggle-file-lock="(scope, path, locked) => emit('toggle-file-lock', scope, path, locked)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Icon } from '@iconify/vue';
import chevronDownIcon from '@iconify-icons/lucide/chevron-down';
import chevronRightIcon from '@iconify-icons/lucide/chevron-right';
import lockIcon from '@iconify-icons/lucide/lock';
import unlockIcon from '@iconify-icons/lucide/unlock';
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
  'toggle-file-lock': [scope: string, path: string, locked: boolean];
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

async function openInDefaultApp(entry: OpenFilesPanelFileEntry) {
  if (entry.state.kind === 'deleted') {
    return;
  }
  try {
    await invoke('open_path_in_default_app', { path: entry.fullPath });
  } catch (error) {
    console.error('failed to open file', entry.fullPath, error);
  }
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
