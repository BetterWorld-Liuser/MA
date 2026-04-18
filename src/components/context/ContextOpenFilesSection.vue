<template>
  <section class="context-section">
    <div class="context-section-summary">
      <div class="context-section-meta">
        <span class="font-mono">{{ `${panel.totalFiles} files` }}</span>
        <span class="font-mono text-text-muted">{{ totalTokenUsage }}</span>
        <span v-if="panel.lockedCount" class="font-mono text-text-dim">{{ `${panel.lockedCount} locked` }}</span>
      </div>
    </div>

    <div v-if="panel.sources.length" class="space-y-3">
      <section v-for="source in panel.sources" :key="source.key" class="space-y-1.5">
        <div class="open-files-source-heading">
          <span>{{ source.name }}</span>
          <div class="ml-auto flex items-center gap-2 font-mono text-[10px] text-text-dim">
            <span>{{ `${source.fileCount} files` }}</span>
            <span>{{ formatTokenCount(source.tokenCount) }}</span>
          </div>
        </div>

        <div class="space-y-0.5">
          <OpenFilesMaterialEntry
            v-for="entry in source.entries"
            :key="entry.key"
            :entry="entry"
            :depth="0"
            :busy="busy"
            :expanded-keys="expandedKeys"
            @toggle-group="toggleGroup"
            @close-file="(scope, path) => $emit('close-file', scope, path)"
            @toggle-file-lock="(scope, path, locked) => $emit('toggle-file-lock', scope, path, locked)"
          />
        </div>
      </section>
    </div>

    <div v-else class="compact-empty">No open files</div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import OpenFilesMaterialEntry from '@/components/context/OpenFilesMaterialEntry.vue';
import { buildOpenFilesPanel, formatTokenCount, isGroupEntry } from '@/components/context/openFilesPanel';
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
  return `ma:open-files-panel:${scope}`;
});

const expandedKeys = ref<Set<string>>(new Set());

const panel = computed(() => buildOpenFilesPanel(props.openFiles, props.workspaceRoot));
const totalTokenUsage = computed(() => formatTokenCount(panel.value.totalTokens));

watch(
  [storageKey, panel],
  ([nextKey, nextPanel]) => {
    const saved = loadExpandedKeys(nextKey);
    const knownKeys = new Set(collectGroupKeys(nextPanel.sources.flatMap((source) => source.entries)));
    const nextExpanded = new Set<string>();

    for (const key of saved) {
      if (knownKeys.has(key)) {
        nextExpanded.add(key);
      }
    }

    for (const key of knownKeys) {
      if (!saved.length || !saved.includes(key)) {
        nextExpanded.add(key);
      }
    }

    expandedKeys.value = nextExpanded;
  },
  { immediate: true },
);

function toggleGroup(key: string) {
  const next = new Set(expandedKeys.value);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  expandedKeys.value = next;
  persistExpandedKeys(storageKey.value, next);
}

function collectGroupKeys(entries: ReturnType<typeof buildOpenFilesPanel>['sources'][number]['entries']) {
  const keys: string[] = [];

  for (const entry of entries) {
    if (!isGroupEntry(entry) || !entry.collapsible) {
      continue;
    }
    keys.push(entry.key);
    keys.push(...collectGroupKeys(entry.children));
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
