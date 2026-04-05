<template>
  <div>
    <button
      v-if="node.type === 'directory'"
      class="open-files-tree-row open-files-tree-row-directory w-full"
      type="button"
      :style="{ paddingLeft: `${depth * 12 + 8}px` }"
      @click="emit('toggle-directory', node.key)"
    >
      <span class="open-files-tree-arrow text-text-dim">{{ expanded ? '▾' : '▸' }}</span>
      <span v-if="node.allLocked" class="open-files-tree-lock text-text-dim">🔒</span>
      <span class="truncate font-mono text-[11px] text-text-muted">{{ node.name }}</span>
      <span v-if="!expanded" class="shrink-0 font-mono text-[9px] text-text-dim">{{ node.tokenUsage }}</span>
    </button>

    <div
      v-else
      class="group open-files-tree-row"
      :style="{ paddingLeft: `${depth === 0 ? 8 : depth * 12 + 24}px` }"
    >
      <button
        v-if="!node.locked"
        class="open-file-glyph shrink-0 opacity-0 transition group-hover:opacity-100"
        type="button"
        :disabled="busy"
        :aria-label="`Close ${node.name}`"
        :title="`Close ${node.name}`"
        @click.stop="emit('close-file', node.fullPath)"
      >
        <Icon :icon="xIcon" class="h-3.5 w-3.5 transition-transform duration-150 group-hover:scale-110" />
      </button>
      <span v-else class="inline-flex h-4 w-4 shrink-0"></span>

      <div class="min-w-0 flex-1" :title="node.fullPath">
        <div class="flex min-w-0 items-center gap-1.5">
          <span v-if="node.locked" class="shrink-0 text-[10px] text-accent">🔒</span>
          <p class="truncate font-mono text-[11px]" :class="fileNameClass(node)">
            {{ node.name }}
          </p>
          <span v-if="node.state.kind === 'moved'" class="truncate font-mono text-[9px] text-text-dim">
            → {{ movedLabel(node.state.newPath) }}
          </span>
        </div>
        <p v-if="node.displayPath !== node.name" class="truncate font-mono text-[9px] text-text-dim">
          {{ node.displayPath }}
        </p>
      </div>

      <span class="shrink-0 font-mono text-[9px] text-text-dim">{{ node.tokenUsage }}</span>

      <button
        class="open-file-glyph shrink-0"
        :class="node.locked ? 'open-file-glyph-locked' : 'open-file-glyph-unlocked'"
        type="button"
        :disabled="busy"
        :aria-label="`${node.locked ? 'Unlock' : 'Lock'} ${node.name}`"
        :title="`${node.locked ? 'Unlock' : 'Lock'} ${node.name}`"
        @click.stop="emit('toggle-file-lock', node.fullPath, !node.locked)"
      >
        <Icon :icon="node.locked ? lockIcon : unlockIcon" class="h-3.5 w-3.5" />
      </button>
    </div>

    <div v-if="node.type === 'directory' && expanded" class="space-y-0.5">
      <OpenFilesTreeNode
        v-for="child in node.children"
        :key="child.key"
        :node="child"
        :depth="depth + 1"
        :busy="busy"
        :expanded-keys="expandedKeys"
        @toggle-directory="emit('toggle-directory', $event)"
        @toggle-file-lock="(path, locked) => emit('toggle-file-lock', path, locked)"
        @close-file="emit('close-file', $event)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Icon } from '@iconify/vue';
import lockIcon from '@iconify-icons/lucide/lock';
import unlockIcon from '@iconify-icons/lucide/unlock';
import xIcon from '@iconify-icons/lucide/x';
import type { OpenFileTreeFileNode, OpenFileTreeNode } from '@/components/context/openFilesTree';

const props = defineProps<{
  node: OpenFileTreeNode;
  depth: number;
  busy?: boolean;
  expandedKeys: Set<string>;
}>();

const emit = defineEmits<{
  'toggle-directory': [key: string];
  'toggle-file-lock': [path: string, locked: boolean];
  'close-file': [path: string];
}>();

const expanded = computed(() => props.node.type === 'directory' && props.expandedKeys.has(props.node.key));

function fileNameClass(node: OpenFileTreeFileNode) {
  if (node.state.kind === 'deleted') {
    return 'text-text-dim line-through';
  }
  if (node.state.kind === 'moved') {
    return 'text-text-dim line-through';
  }
  if (node.locked) {
    return 'text-text-muted';
  }
  if (node.freshness === 'high') {
    return 'text-text';
  }
  if (node.freshness === 'medium') {
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
