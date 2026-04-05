<template>
  <aside class="panel context-column-divider flex min-h-0 flex-col overflow-hidden">
    <div class="panel-header flex items-center gap-2">
      <span class="text-accent">
        <Icon :icon="panelRightIcon" class="h-4 w-4" />
      </span>
      <p class="truncate text-[12px] font-semibold tracking-[0.01em] text-text">Context</p>
    </div>

    <div class="min-h-0 flex-1 space-y-3.5 overflow-y-auto p-2">
      <ContextNotesSection
        :notes="notes"
        :busy="busy"
        @add-note="$emit('add-note')"
        @edit-note="$emit('edit-note', $event)"
        @delete-note="$emit('delete-note', $event)"
      />

      <ContextOpenFilesSection
        :open-files="openFiles"
        :workspace-root="workingDirectory"
        :busy="busy"
        @toggle-file-lock="(path, locked) => $emit('toggle-file-lock', path, locked)"
        @close-file="$emit('close-file', $event)"
      />

      <ContextHintsSection v-if="hints.length" :hints="hints" />
      <ContextSkillsSection :skills="skills" :busy="busy" @refresh="$emit('refresh-skills')" />

      <section class="space-y-3">
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-1.5">
            <Icon :icon="gaugeIcon" class="h-3.5 w-3.5 text-text-dim" />
            <h3 class="section-title mb-0">Context usage (est.)</h3>
          </div>
          <div class="flex items-center gap-2 text-[8px] text-text-dim">
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

        <ul class="grid grid-cols-2 gap-x-3 gap-y-1 text-[10px]">
          <li v-for="section in usage.sections" :key="section.name" class="flex items-center justify-between gap-2">
            <span class="truncate text-text-dim">{{ section.name }}</span>
            <span class="font-mono text-text">{{ section.size }}</span>
          </li>
        </ul>
      </section>

      <section class="space-y-1.5">
        <div class="flex items-center justify-between gap-3">
          <h3 class="section-title mb-0 !text-[9px]">Debug</h3>
          <span class="text-[9px] text-text-dim">{{ debugRounds.length ? `${debugRounds.length} rounds` : 'No trace yet' }}</span>
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
                <span class="font-mono text-[10px] text-text">Round {{ round.iteration }}</span>
                <span class="text-[9px] text-text-dim">{{ round.toolCalls.length ? `${round.toolCalls.length} tools` : 'no tools' }}</span>
              </div>
              <p class="text-[10px] text-text-dim">{{ summarizeRound(round) }}</p>
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
            <div class="mb-2 flex items-center justify-end">
              <div class="debug-tab-row">
                <button
                  v-for="mode in responseModes"
                  :key="mode"
                  class="debug-tab"
                  :class="activeResponseMode === mode ? 'debug-tab-active' : ''"
                  type="button"
                  @click="activeResponseMode = mode"
                >
                  {{ mode }}
                </button>
              </div>
            </div>
            <div v-for="round in debugRounds" :key="`response-${round.iteration}`" class="debug-block">
              <div class="debug-block-header">
                <span>Round {{ round.iteration }} · {{ activeResponseMode }}</span>
                <div class="flex items-center gap-1">
                  <button
                    class="debug-copy-button"
                    type="button"
                    title="Open large preview"
                    @click="openPreview(`Round ${round.iteration} · Response · ${activeResponseMode}`, selectedResponse(round))"
                  >
                    <Icon :icon="expandIcon" class="h-3.5 w-3.5" />
                  </button>
                  <button class="debug-copy-button" type="button" title="Copy response" @click="copyText(selectedResponse(round))">
                    <Icon :icon="copyIcon" class="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
              <pre class="debug-pre">{{ selectedResponse(round) }}</pre>
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
                        <p class="text-[10px] text-text">
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
                  <p v-else class="text-[10px] text-text-dim">(none)</p>
                </div>

                <div>
                  <p class="debug-subtitle">Tool results</p>
                  <div v-if="round.toolResults.length" class="space-y-1">
                    <div v-for="(result, index) in round.toolResults" :key="`${round.iteration}-result-${index}`" class="debug-inline-card">
                      <div class="mb-1 flex items-center justify-between gap-2">
                        <p class="text-[10px] text-text-dim">Result {{ index + 1 }}</p>
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
                  <p v-else class="text-[10px] text-text-dim">(none)</p>
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
          <DialogTitle class="text-[12px] font-semibold text-text">{{ previewTitle }}</DialogTitle>
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
import { computed, ref } from 'vue';
import { Icon } from '@iconify/vue';
import copyIcon from '@iconify-icons/lucide/copy';
import expandIcon from '@iconify-icons/lucide/expand';
import gaugeIcon from '@iconify-icons/lucide/gauge';
import panelRightIcon from '@iconify-icons/lucide/panel-right';
import xIcon from '@iconify-icons/lucide/x';
import ContextHintsSection from '@/components/context/ContextHintsSection.vue';
import ContextNotesSection from '@/components/context/ContextNotesSection.vue';
import ContextOpenFilesSection from '@/components/context/ContextOpenFilesSection.vue';
import ContextSkillsSection from '@/components/context/ContextSkillsSection.vue';
import { Dialog, DialogClose, DialogContent, DialogTitle } from './ui/dialog';
import type { ContextUsage, DebugRoundItem, HintItem, NoteItem, OpenFileItem, SkillItem } from '../data/mock';

withDefaults(defineProps<{
  notes: NoteItem[];
  openFiles: OpenFileItem[];
  workingDirectory?: string;
  hints: HintItem[];
  skills: SkillItem[];
  usage: ContextUsage;
  debugRounds: DebugRoundItem[];
  busy?: boolean;
}>(), {
  notes: () => [],
  openFiles: () => [],
  workingDirectory: undefined,
  hints: () => [],
  skills: () => [],
  busy: false,
});

defineEmits<{
  'add-note': [];
  'edit-note': [noteId: string];
  'delete-note': [noteId: string];
  'toggle-file-lock': [path: string, locked: boolean];
  'close-file': [path: string];
  'refresh-skills': [];
}>();

const debugTabs = ['Overview', 'Context', 'Request', 'Response', 'Tools'] as const;
const activeDebugTab = ref<(typeof debugTabs)[number]>('Overview');
const responseModes = ['Structured', 'Raw'] as const;
const activeResponseMode = ref<(typeof responseModes)[number]>('Structured');
const isPreviewOpen = ref(false);
const previewTitle = ref('');
const previewContent = ref('');

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

function selectedResponse(round: DebugRoundItem) {
  return activeResponseMode.value === 'Raw' ? round.providerResponseRaw : round.providerResponseJson;
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
