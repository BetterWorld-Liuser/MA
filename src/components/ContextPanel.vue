<template>
  <aside class="panel context-column-divider flex min-h-0 flex-col overflow-hidden">
    <div class="panel-header flex items-center gap-2">
      <span class="text-accent">
        <Icon :icon="panelRightIcon" class="h-4 w-4" />
      </span>
      <p class="truncate text-[13px] font-semibold text-text">Context</p>
    </div>

    <div class="min-h-0 flex-1 space-y-3 overflow-y-auto p-2">
      <section class="space-y-1.5">
        <div class="flex items-center justify-between gap-3">
          <h3 class="section-title mb-0">Notes</h3>
          <button class="pill px-1.5" type="button" :disabled="busy" @click="$emit('add-note')">
            <Icon :icon="plusIcon" class="h-3.5 w-3.5" />
          </button>
        </div>
        <div v-if="orderedNotes.length" class="space-y-0.5">
          <div v-for="note in orderedNotes" :key="note.id" class="group compact-row">
            <div class="min-w-0 flex flex-1 items-baseline gap-2 overflow-hidden">
              <span class="shrink-0 font-mono text-[10px] uppercase tracking-widest text-text-dim">{{ note.id }}</span>
              <p class="truncate text-[12px] text-text">{{ note.content }}</p>
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

      <section class="space-y-1.5">
        <h3 class="section-title">Open files</h3>
        <div class="space-y-0.5">
          <div v-for="file in openFiles" :key="file.path" class="group compact-row">
            <button
              v-if="!file.locked"
              class="open-file-glyph shrink-0 opacity-0 transition group-hover:opacity-100"
              type="button"
              :disabled="busy"
              :aria-label="`Close ${fileName(file.path)}`"
              :title="`Close ${fileName(file.path)}`"
              @click="$emit('close-file', file.path)"
            >
              <Icon :icon="xIcon" class="h-3.5 w-3.5 transition-transform duration-150 group-hover:scale-110" />
            </button>

            <div class="min-w-0 flex flex-1 items-center gap-2" :title="file.path">
              <p class="truncate font-mono text-[13px]" :class="freshnessClass(file.freshness)">
                {{ fileName(file.path) }}
              </p>
              <span class="shrink-0 font-mono text-[10px] text-text-dim">{{ file.time }}</span>
            </div>

            <button
              class="open-file-glyph shrink-0"
              :class="file.locked ? 'open-file-glyph-locked' : 'open-file-glyph-unlocked'"
              type="button"
              :disabled="busy"
              :aria-label="`${file.locked ? 'Unlock' : 'Lock'} ${fileName(file.path)}`"
              :title="`${file.locked ? 'Unlock' : 'Lock'} ${fileName(file.path)}`"
              @click="$emit('toggle-file-lock', file.path, !file.locked)"
            >
              <Icon :icon="file.locked ? lockIcon : unlockIcon" class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </section>

      <section v-if="hints.length" class="space-y-1.5">
        <h3 class="section-title">Hints</h3>
        <div class="space-y-0.5">
          <div v-for="hint in hints" :key="`${hint.source}-${hint.content}`" class="compact-row">
            <div class="min-w-0 flex-1">
              <p class="truncate text-[12px] text-text">
                <span class="mr-2 text-[10px] uppercase tracking-widest text-text-dim">{{ hint.source }}</span>{{ hint.content }}
              </p>
            </div>
            <div class="shrink-0 text-right text-[10px] text-text-dim">
              <p class="font-mono">{{ hint.timeLeft }}</p>
              <p>{{ hint.turnsLeft }}</p>
            </div>
          </div>
        </div>
      </section>

      <section class="space-y-1.5">
        <div class="flex items-center justify-between gap-3">
          <h3 class="section-title mb-0">Context usage</h3>
          <div class="flex items-center gap-2 text-[10px] text-text-dim">
            <span class="font-mono">{{ usage.percent }}%</span>
            <span class="font-mono text-text-muted">{{ usage.current }} / {{ usage.limit }}</span>
          </div>
        </div>

        <div class="h-2 overflow-hidden rounded-full bg-bg-tertiary">
          <div
            class="h-full rounded-full transition-all"
            :class="usage.percent > 95 ? 'bg-error' : usage.percent > 80 ? 'bg-warning' : 'bg-accent'"
            :style="{ width: `${usage.percent}%` }"
          ></div>
        </div>

        <ul class="grid grid-cols-2 gap-x-3 gap-y-1 text-[11px]">
          <li v-for="section in usage.sections" :key="section.name" class="flex items-center justify-between gap-2">
            <span class="truncate text-text-dim">{{ section.name }}</span>
            <span class="font-mono text-text">{{ section.size }}</span>
          </li>
        </ul>
      </section>

      <section class="space-y-1.5">
        <div class="flex items-center justify-between gap-3">
          <h3 class="section-title mb-0">Debug</h3>
          <span class="text-[10px] text-text-dim">{{ debugRounds.length ? `${debugRounds.length} rounds` : 'No trace yet' }}</span>
        </div>

        <div v-if="debugRounds.length" class="space-y-2">
          <div class="debug-tab-row">
            <button
              v-for="tab in debugTabs"
              :key="tab"
              class="debug-tab"
              :class="activeDebugTab === tab ? 'debug-tab-active' : ''"
              type="button"
              @click="activeDebugTab = tab"
            >
              {{ tab }}
            </button>
          </div>

          <div v-if="activeDebugTab === 'Overview'" class="debug-panel space-y-2">
            <div v-for="round in debugRounds" :key="round.iteration" class="debug-round-summary">
              <div class="flex items-center justify-between gap-2">
                <span class="font-mono text-[11px] text-text">Round {{ round.iteration }}</span>
                <span class="text-[10px] text-text-dim">{{ round.toolCalls.length ? `${round.toolCalls.length} tools` : 'no tools' }}</span>
              </div>
              <p class="text-[11px] text-text-dim">{{ summarizeRound(round) }}</p>
            </div>
          </div>

          <div v-else-if="activeDebugTab === 'Context'" class="debug-panel">
            <div v-for="round in debugRounds" :key="`context-${round.iteration}`" class="debug-block">
              <div class="debug-block-header">
                <span>Round {{ round.iteration }}</span>
                <div class="flex items-center gap-1">
                  <button
                    class="debug-copy-button"
                    type="button"
                    title="Open large preview"
                    @click="openPreview(`Round ${round.iteration} · Context`, round.contextPreview)"
                  >
                    <Icon :icon="expandIcon" class="h-3.5 w-3.5" />
                  </button>
                  <button class="debug-copy-button" type="button" title="Copy context" @click="copyText(round.contextPreview)">
                    <Icon :icon="copyIcon" class="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
              <pre class="debug-pre">{{ round.contextPreview }}</pre>
            </div>
          </div>

          <div v-else-if="activeDebugTab === 'Request'" class="debug-panel">
            <div v-for="round in debugRounds" :key="`request-${round.iteration}`" class="debug-block">
              <div class="debug-block-header">
                <span>Round {{ round.iteration }}</span>
                <div class="flex items-center gap-1">
                  <button
                    class="debug-copy-button"
                    type="button"
                    title="Open large preview"
                    @click="openPreview(`Round ${round.iteration} · Request`, round.providerRequestJson)"
                  >
                    <Icon :icon="expandIcon" class="h-3.5 w-3.5" />
                  </button>
                  <button class="debug-copy-button" type="button" title="Copy request" @click="copyText(round.providerRequestJson)">
                    <Icon :icon="copyIcon" class="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
              <pre class="debug-pre">{{ round.providerRequestJson }}</pre>
            </div>
          </div>

          <div v-else-if="activeDebugTab === 'Response'" class="debug-panel">
            <div v-for="round in debugRounds" :key="`response-${round.iteration}`" class="debug-block">
              <div class="debug-block-header">
                <span>Round {{ round.iteration }}</span>
                <div class="flex items-center gap-1">
                  <button
                    class="debug-copy-button"
                    type="button"
                    title="Open large preview"
                    @click="openPreview(`Round ${round.iteration} · Response`, round.providerResponseRaw)"
                  >
                    <Icon :icon="expandIcon" class="h-3.5 w-3.5" />
                  </button>
                  <button class="debug-copy-button" type="button" title="Copy response" @click="copyText(round.providerResponseRaw)">
                    <Icon :icon="copyIcon" class="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
              <pre class="debug-pre">{{ round.providerResponseRaw }}</pre>
            </div>
          </div>

          <div v-else class="debug-panel space-y-2">
            <div v-for="round in debugRounds" :key="`tools-${round.iteration}`" class="debug-block">
              <div class="debug-block-header">
                <span>Round {{ round.iteration }}</span>
                <div class="flex items-center gap-1">
                  <button
                    class="debug-copy-button"
                    type="button"
                    title="Open large preview"
                    @click="openPreview(`Round ${round.iteration} · Tools`, formatToolsRound(round))"
                  >
                    <Icon :icon="expandIcon" class="h-3.5 w-3.5" />
                  </button>
                  <button
                    class="debug-copy-button"
                    type="button"
                    title="Copy all tools"
                    @click="copyText(formatToolsRound(round))"
                  >
                    <Icon :icon="copyIcon" class="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
              <div class="space-y-2">
                <div>
                  <p class="debug-subtitle">Tool calls</p>
                  <div v-if="round.toolCalls.length" class="space-y-1">
                    <div v-for="toolCall in round.toolCalls" :key="`${round.iteration}-${toolCall.id}`" class="debug-inline-card">
                      <div class="flex items-center justify-between gap-2">
                        <p class="text-[11px] text-text">
                          <span class="font-mono">{{ toolCall.name }}</span>
                          <span class="ml-2 text-text-dim">{{ toolCall.id }}</span>
                        </p>
                        <button
                          class="debug-copy-button"
                          type="button"
                          title="Open large preview"
                          @click="openPreview(`Round ${round.iteration} · ${toolCall.name} ${toolCall.id}`, toolCall.argumentsJson)"
                        >
                          <Icon :icon="expandIcon" class="h-3.5 w-3.5" />
                        </button>
                        <button
                          class="debug-copy-button"
                          type="button"
                          title="Copy tool call"
                          @click="copyText(toolCall.argumentsJson)"
                        >
                          <Icon :icon="copyIcon" class="h-3.5 w-3.5" />
                        </button>
                      </div>
                      <pre class="debug-pre debug-pre-inline">{{ toolCall.argumentsJson }}</pre>
                    </div>
                  </div>
                  <p v-else class="text-[11px] text-text-dim">(none)</p>
                </div>

                <div>
                  <p class="debug-subtitle">Tool results</p>
                  <div v-if="round.toolResults.length" class="space-y-1">
                    <div v-for="(result, index) in round.toolResults" :key="`${round.iteration}-result-${index}`" class="debug-inline-card">
                      <div class="mb-1 flex items-center justify-between gap-2">
                        <p class="text-[11px] text-text-dim">Result {{ index + 1 }}</p>
                        <div class="flex items-center gap-1">
                          <button
                            class="debug-copy-button"
                            type="button"
                            title="Open large preview"
                            @click="openPreview(`Round ${round.iteration} · Tool result ${index + 1}`, result)"
                          >
                            <Icon :icon="expandIcon" class="h-3.5 w-3.5" />
                          </button>
                          <button class="debug-copy-button" type="button" title="Copy tool result" @click="copyText(result)">
                            <Icon :icon="copyIcon" class="h-3.5 w-3.5" />
                          </button>
                        </div>
                      </div>
                      <pre class="debug-pre debug-pre-inline">{{ result }}</pre>
                    </div>
                  </div>
                  <p v-else class="text-[11px] text-text-dim">(none)</p>
                </div>
              </div>
            </div>
          </div>
        </div>
        <div v-else class="compact-empty">Send a message to inspect the latest trace</div>
      </section>
    </div>
  </aside>

  <Dialog v-model:open="isPreviewOpen">
    <DialogContent
      :show-close-button="false"
      class="!h-[92vh] !w-[96vw] !max-h-[92vh] !max-w-[96vw] overflow-hidden rounded-[24px] p-0 sm:!max-w-[96vw]"
    >
      <div class="flex h-full min-h-0 flex-col">
        <div class="flex items-center justify-between gap-3 border-b border-border px-4 py-2.5">
          <DialogTitle class="text-[13px] font-semibold text-text">{{ previewTitle }}</DialogTitle>
          <DialogClose class="inline-flex h-7 w-7 items-center justify-center rounded-md text-text-dim transition hover:bg-bg-hover hover:text-text">
            <Icon :icon="xIcon" class="h-4 w-4" />
            <span class="sr-only">Close</span>
          </DialogClose>
        </div>

        <div class="flex min-h-0 flex-1 overflow-hidden px-3 py-3">
          <pre class="debug-pre-modal">{{ previewContent }}</pre>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue';
import copyIcon from '@iconify-icons/lucide/copy';
import expandIcon from '@iconify-icons/lucide/expand';
import lockIcon from '@iconify-icons/lucide/lock';
import panelRightIcon from '@iconify-icons/lucide/panel-right';
import pencilIcon from '@iconify-icons/lucide/pencil';
import plusIcon from '@iconify-icons/lucide/plus';
import unlockIcon from '@iconify-icons/lucide/unlock';
import xIcon from '@iconify-icons/lucide/x';
import { Dialog, DialogClose, DialogContent, DialogTitle } from './ui/dialog';
import type { ContextUsage, DebugRoundItem, HintItem, NoteItem, OpenFileItem } from '../data/mock';
import { computed, ref } from 'vue';

const props = defineProps<{
  notes: NoteItem[];
  openFiles: OpenFileItem[];
  hints: HintItem[];
  usage: ContextUsage;
  debugRounds: DebugRoundItem[];
  busy?: boolean;
}>();

defineEmits<{
  'add-note': [];
  'edit-note': [noteId: string];
  'delete-note': [noteId: string];
  'toggle-file-lock': [path: string, locked: boolean];
  'close-file': [path: string];
}>();

const orderedNotes = computed(() =>
  [...props.notes].sort((a, b) => {
    if (a.id.toLowerCase() === 'target') return -1;
    if (b.id.toLowerCase() === 'target') return 1;
    return a.id.localeCompare(b.id);
  }),
);

const debugTabs = ['Overview', 'Context', 'Request', 'Response', 'Tools'] as const;
const activeDebugTab = ref<(typeof debugTabs)[number]>('Overview');
const isPreviewOpen = ref(false);
const previewTitle = ref('');
const previewContent = ref('');

function freshnessClass(freshness: OpenFileItem['freshness']) {
  if (freshness === 'high') return 'text-text';
  if (freshness === 'medium') return 'text-text-muted';
  return 'text-text-dim';
}

function fileName(path: string) {
  const normalized = path.replaceAll('\\', '/');
  const segments = normalized.split('/');
  return segments[segments.length - 1] || normalized;
}

function summarizeRound(round: DebugRoundItem) {
  if (!round.toolCalls.length && !round.toolResults.length) {
    return 'Assistant completed this round without tool activity.';
  }

  const parts = [];
  if (round.toolCalls.length) {
    parts.push(`called ${round.toolCalls.length} tool${round.toolCalls.length > 1 ? 's' : ''}`);
  }
  if (round.toolResults.length) {
    parts.push(`recorded ${round.toolResults.length} result${round.toolResults.length > 1 ? 's' : ''}`);
  }
  return `${parts.join(' and ')}.`;
}

function formatToolsRound(round: DebugRoundItem) {
  const toolCalls = round.toolCalls.length
    ? round.toolCalls
        .map((toolCall) => `${toolCall.name} ${toolCall.id}\n${toolCall.argumentsJson}`)
        .join('\n\n')
    : '(none)';
  const toolResults = round.toolResults.length ? round.toolResults.join('\n\n') : '(none)';

  return `Round ${round.iteration}\n\n[Tool Calls]\n${toolCalls}\n\n[Tool Results]\n${toolResults}`;
}

function openPreview(title: string, content: string) {
  previewTitle.value = title;
  previewContent.value = content;
  isPreviewOpen.value = true;
}

async function copyText(value: string) {
  await navigator.clipboard.writeText(value);
}
</script>
