<template>
  <section class="context-section">
    <div class="context-section-summary">
      <div class="context-section-meta">
        <span class="font-mono">{{ `${openFiles.length} files` }}</span>
        <span class="font-mono text-text-muted">{{ totalTokenUsage }}</span>
      </div>
    </div>

    <div v-if="treeNodes.length" class="space-y-0.5">
      <OpenFilesTreeNode
        v-for="node in treeNodes"
        :key="node.key"
        :node="node"
        :depth="0"
        :busy="busy"
        :expanded-keys="expandedKeys"
        @toggle-directory="toggleDirectory"
        @toggle-file-lock="(scope, path, locked) => $emit('toggle-file-lock', scope, path, locked)"
        @close-file="(scope, path) => $emit('close-file', scope, path)"
      />
    </div>

    <div v-else class="compact-empty">No open files</div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import OpenFilesTreeNode from '@/components/context/OpenFilesTreeNode.vue';
import { buildOpenFilesTree, formatTokenCount, isDirectoryNode } from '@/components/context/openFilesTree';
import type { OpenFileItem } from '@/data/mock';

const props = defineProps<{
  openFiles: OpenFileItem[];
  workspaceRoot?: string;
  busy?: boolean;
}>();

const emit = defineEmits<{
  'toggle-file-lock': [scope: string, path: string, locked: boolean];
  'close-file': [scope: string, path: string];
}>();

const storageKey = computed(() => {
  const scope = props.workspaceRoot?.replaceAll('\\', '/').toLowerCase() ?? 'global';
  return `ma:open-files-tree:${scope}`;
});

const expandedKeys = ref<Set<string>>(new Set());

const tree = computed(() => buildOpenFilesTree(props.openFiles, props.workspaceRoot));
const treeNodes = computed(() => tree.value.nodes);
const totalTokenUsage = computed(() => formatTokenCount(tree.value.totalTokens));

watch(
  [storageKey, treeNodes],
  ([nextKey, nodes]) => {
    const saved = loadExpandedKeys(nextKey);
    const knownKeys = new Set(collectDirectoryKeys(nodes));
    const nextExpanded = new Set<string>();

    for (const key of saved) {
      if (knownKeys.has(key)) {
        nextExpanded.add(key);
      }
    }

    // New directories default to expanded so the first render stays discoverable.
    for (const key of knownKeys) {
      if (!saved.length) {
        nextExpanded.add(key);
      }
    }

    expandedKeys.value = nextExpanded;
  },
  { immediate: true },
);

function toggleDirectory(key: string) {
  const next = new Set(expandedKeys.value);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  expandedKeys.value = next;
  persistExpandedKeys(storageKey.value, next);
}

function collectDirectoryKeys(nodes: ReturnType<typeof buildOpenFilesTree>['nodes']) {
  const keys: string[] = [];

  for (const node of nodes) {
    if (!isDirectoryNode(node)) {
      continue;
    }
    keys.push(node.key);
    keys.push(...collectDirectoryKeys(node.children));
  }

  return keys;
}

function loadExpandedKeys(key: string) {
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : [];
  } catch {
    return [];
  }
}

function persistExpandedKeys(key: string, values: Set<string>) {
  try {
    window.localStorage.setItem(key, JSON.stringify([...values]));
  } catch {
    // Ignore persistence failures and keep the in-memory state usable.
  }
}
</script>
