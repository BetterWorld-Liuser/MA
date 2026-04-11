<template>
  <article
    class="chat-row chat-row-assistant"
    :class="isHighlighted ? 'ring-1 ring-accent/45 rounded-2xl' : ''"
    :data-entry-key="entryKey"
  >
    <span class="message-avatar shrink-0">{{ entry.agentName.slice(0, 1) }}</span>

    <div class="message-stack items-start">
      <div class="message-meta justify-start">
        <span class="text-[11px] font-semibold text-text">{{ entry.agentName }}</span>
        <time class="font-mono text-[9px] text-text-dim">{{ formatEntryTime(entry.ts) }}</time>
        <span class="text-[10px] text-text-dim">← {{ triggerLabel }}</span>
        <button
          v-if="entry.state === 'streaming'"
          class="inline-flex h-5 w-5 items-center justify-center rounded-md text-text-dim transition hover:bg-bg-hover hover:text-text"
          type="button"
          title="中断这个 turn"
          @click="$emit('cancel-turn', entry.turnId)"
        >
          <Icon :icon="pauseIcon" class="h-3 w-3" />
        </button>
        <button class="text-[10px] text-text-dim transition hover:text-text" type="button" @click="$emit('reply-entry', selfReply)">
          引用
        </button>
      </div>

      <div class="message-bubble message-bubble-assistant opacity-95">
        <p v-if="entry.statusLabel && entry.state === 'streaming' && !entry.messages.length" class="text-[11px] text-text-dim">
          {{ entry.statusLabel }}
        </p>

        <template v-if="entry.state === 'streaming' || expanded || !canCollapse">
          <div v-for="message in visibleMessages" :key="message.messageId" class="space-y-2">
            <p v-if="message.reasoning" class="whitespace-pre-wrap text-[11px] leading-[1.5] text-text-dim">{{ message.reasoning }}</p>
            <TimelineRenderer :entries="message.timeline" :final="entry.state !== 'streaming'" />
          </div>
        </template>

        <template v-else>
          <button class="text-[11px] text-text-dim transition hover:text-text" type="button" @click="expanded = true">
            {{ toolSummaryLabel }}
          </button>
          <div v-if="entry.state === 'done' && finalMessage" class="mt-3 space-y-3">
            <div class="text-[10px] uppercase tracking-[0.12em] text-text-dim">最终消息</div>
            <TimelineRenderer :entries="finalMessage.timeline" final />
          </div>
          <p v-if="entry.state !== 'done' && entry.errorMessage" class="mt-2 whitespace-pre-wrap text-[11px] text-error">
            {{ entry.errorMessage }}
          </p>
        </template>

        <p v-if="entry.state !== 'streaming' && expanded && canCollapse" class="mt-3">
          <button class="text-[11px] text-text-dim transition hover:text-text" type="button" @click="expanded = false">
            收起过程
          </button>
        </p>

        <p v-if="entry.state !== 'done' && entry.errorMessage && (expanded || !canCollapse)" class="mt-3 whitespace-pre-wrap text-[11px] text-error">
          {{ entry.errorMessage }}
        </p>
      </div>
    </div>
  </article>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { Icon } from '@iconify/vue';
import pauseIcon from '@iconify-icons/lucide/pause';
import type { AssistantTimelineEntry, Turn } from '@/data/mock';
import { countTurnToolCalls } from '@/composables/chatRuntime/chatEventReducer';
import type { ComposerReplyPreview } from '@/composables/workspaceApp/types';
import TimelineRenderer from './TimelineRenderer.vue';

const props = defineProps<{
  entry: Turn;
  activeHighlightKey?: string | null;
}>();

defineEmits<{
  'reply-entry': [reply: ComposerReplyPreview];
  'cancel-turn': [turnId: string];
}>();

const expanded = ref(false);
const entryKey = computed(() => `turn:${props.entry.turnId}`);
const isHighlighted = computed(() => props.activeHighlightKey === entryKey.value);

const finalMessage = computed(() => props.entry.messages.at(-1));
const canCollapse = computed(() => props.entry.state !== 'streaming' && props.entry.messages.length > 1);
const visibleMessages = computed(() => {
  if (props.entry.state === 'streaming' || expanded.value || !canCollapse.value) {
    return props.entry.messages;
  }
  return finalMessage.value ? [finalMessage.value] : [];
});
const triggerLabel = computed(() =>
  props.entry.trigger.kind === 'user' ? '来自用户消息' : '来自上一个 turn'
);
const toolSummaryLabel = computed(() => `${countTurnToolCalls(props.entry)} 个动作 ▸`);
const selfReply = computed<ComposerReplyPreview>(() => ({
  kind: 'turn',
  id: props.entry.turnId,
  author: props.entry.agentName,
  summary: summarizeTurnReply(props.entry),
}));

function formatEntryTime(timestamp: number) {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function summarizeTurnReply(entry: Turn) {
  const sample = entry.messages
    .flatMap((message) => message.timeline)
    .filter((timelineEntry): timelineEntry is Extract<AssistantTimelineEntry, { kind: 'text' }> => timelineEntry.kind === 'text')
    .map((timelineEntry) => timelineEntry.text)
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();

  if (!sample) {
    return `${entry.messages.length} 条消息 / ${entry.state}`;
  }

  return sample.length > 72 ? `${sample.slice(0, 72)}…` : sample;
}
</script>
