<template>
  <article
    class="chat-row chat-row-assistant message-row-assistant"
    :class="isHighlighted ? 'ring-1 ring-accent/45 rounded-2xl' : ''"
    :data-entry-key="entryKey"
  >
    <span class="message-avatar shrink-0">{{ entry.agentName.slice(0, 1) }}</span>

    <div class="message-stack items-start">
      <div class="message-meta justify-start">
        <span class="text-[12px] font-semibold text-text">{{ entry.agentName }}</span>
        <time class="text-[10px] tabular-nums text-text-dim">{{ formatEntryTime(entry.ts) }}</time>
        <span class="text-[10px] text-text-dim/40 transition hover:text-text-dim/80">← {{ triggerLabel }}</span>
        <button
          v-if="entry.state === 'streaming'"
          class="inline-flex h-5 w-5 items-center justify-center rounded-md text-text-dim transition hover:bg-bg-hover hover:text-text"
          type="button"
          title="中断这个 turn"
          @click="$emit('cancel-turn', entry.turnId)"
        >
          <Icon :icon="pauseIcon" class="h-3 w-3" />
        </button>
      </div>

      <div class="message-bubble message-bubble-assistant opacity-95">
        <p v-if="entry.statusLabel && entry.state === 'streaming' && !entry.messages.length" class="text-[11px] text-text-dim">
          {{ entry.statusLabel }}
        </p>

        <div v-else-if="showStreamingPlaceholder" class="assistant-pending-placeholder" aria-label="AI 正在回复">
          <span class="assistant-pending-placeholder-label">正在思考</span>
          <span class="assistant-pending-placeholder-dots" aria-hidden="true">
            <span></span>
            <span></span>
            <span></span>
          </span>
        </div>

        <template v-else-if="entry.state === 'streaming' || !canCollapse">
          <div v-for="message in visibleMessages" :key="message.messageId" class="space-y-2">
            <p v-if="message.reasoning" class="whitespace-pre-wrap text-[11px] leading-[1.5] text-text-dim">{{ message.reasoning }}</p>
            <TimelineRenderer :entries="message.timeline" :final="entry.state !== 'streaming'" />
          </div>
        </template>

        <template v-else>
          <button
            class="inline-flex items-center gap-1 text-[11px] text-text-dim transition hover:text-text"
            type="button"
            @click="expanded = !expanded"
          >
            <span class="grid">
              <span class="col-start-1 row-start-1" :class="expanded ? 'invisible' : ''">{{ toolSummaryCountLabel }}</span>
              <span class="col-start-1 row-start-1" :class="expanded ? '' : 'invisible'">收起</span>
            </span>
            <Icon
              :icon="chevronRightIcon"
              class="h-3 w-3 transition-transform"
              :class="expanded ? 'rotate-90' : ''"
            />
          </button>
          <div v-show="expanded && processMessages.length" class="space-y-3">
            <div v-for="message in processMessages" :key="message.messageId" class="space-y-2">
              <p v-if="message.reasoning" class="whitespace-pre-wrap text-[11px] leading-[1.5] text-text-dim">{{ message.reasoning }}</p>
              <TimelineRenderer :entries="message.timeline" final />
            </div>
          </div>
          <div v-if="entry.state === 'done' && finalMessage" class="mt-3 space-y-3">
            <div v-show="expanded" class="text-[11px] text-text-dim">最终消息</div>
            <TimelineRenderer :entries="finalMessage.timeline" final />
          </div>
          <p v-if="entry.state !== 'done' && entry.errorMessage" class="mt-2 whitespace-pre-wrap text-[11px] text-error">
            {{ entry.errorMessage }}
          </p>
        </template>

        <p v-if="entry.state !== 'done' && entry.errorMessage && (expanded || !canCollapse)" class="mt-3 whitespace-pre-wrap text-[11px] text-error">
          {{ entry.errorMessage }}
        </p>
      </div>

      <div
        class="message-actions"
        :class="isActionBarVisible ? 'message-actions-active' : ''"
        @mouseenter="isActionBarVisible = true"
        @mouseleave="isActionBarVisible = false"
        @focusin="isActionBarVisible = true"
        @focusout="handleFocusOut"
      >
        <button
          class="message-copy-button"
          :class="isActionBarVisible ? 'message-copy-button-visible' : ''"
          type="button"
          title="引用此消息"
          aria-label="引用此消息"
          @click="$emit('reply-entry', selfReply)"
        >
          <Icon :icon="quoteIcon" class="message-copy-icon" />
        </button>
        <button
          class="message-copy-button"
          :class="isActionBarVisible ? 'message-copy-button-visible' : ''"
          type="button"
          :title="copyButtonTitle"
          :aria-label="copyButtonTitle"
          :disabled="!canCopyMessage"
          @click="copyAssistantMessage"
        >
          <Icon :icon="copyFeedbackIcon" class="message-copy-icon" />
        </button>
      </div>
    </div>
  </article>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue';
import { Icon } from '@iconify/vue';
import pauseIcon from '@iconify-icons/lucide/pause';
import chevronRightIcon from '@iconify-icons/lucide/chevron-right';
import checkIcon from '@iconify-icons/lucide/check';
import copyIcon from '@iconify-icons/lucide/copy';
import quoteIcon from '@iconify-icons/lucide/quote';
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
const copied = ref(false);
const isActionBarVisible = ref(false);
const entryKey = computed(() => `turn:${props.entry.turnId}`);
const isHighlighted = computed(() => props.activeHighlightKey === entryKey.value);

const finalMessage = computed(() => props.entry.messages.at(-1));
const processMessages = computed(() => (canCollapse.value ? props.entry.messages.slice(0, -1) : props.entry.messages));
const canCollapse = computed(() => props.entry.state !== 'streaming' && props.entry.messages.length > 1);
const showStreamingPlaceholder = computed(() => props.entry.state === 'streaming' && !hasRenderableStreamingContent(props.entry));
const visibleMessages = computed(() => {
  if (props.entry.state === 'streaming' || !canCollapse.value) {
    return props.entry.messages;
  }
  return finalMessage.value ? [finalMessage.value] : [];
});
const triggerLabel = computed(() =>
  props.entry.trigger.kind === 'user' ? '来自用户消息' : '来自上一个 turn'
);
const toolSummaryCountLabel = computed(() => `${countTurnToolCalls(props.entry)}个动作`);
const copyableMessageText = computed(() => {
  if (props.entry.state === 'streaming') {
    return '';
  }

  return props.entry.messages
    .flatMap((message) => message.timeline)
    .filter((timelineEntry): timelineEntry is Extract<AssistantTimelineEntry, { kind: 'text' }> => timelineEntry.kind === 'text')
    .map((timelineEntry) => timelineEntry.text.trimEnd())
    .join('')
    .trim();
});
const canCopyMessage = computed(() => copyableMessageText.value.length > 0);
const copyFeedbackIcon = computed(() => (copied.value ? checkIcon : copyIcon));
const copyButtonTitle = computed(() => {
  if (!canCopyMessage.value) {
    return props.entry.state === 'streaming' ? '消息生成中，暂不可复制' : '暂无可复制内容';
  }

  return copied.value ? '已复制' : '复制消息';
});
const selfReply = computed<ComposerReplyPreview>(() => ({
  kind: 'turn',
  id: props.entry.turnId,
  author: props.entry.agentName,
  summary: summarizeTurnReply(props.entry),
}));
let copiedResetTimer: number | null = null;

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

function hasRenderableStreamingContent(entry: Turn) {
  return entry.messages.some((message) => {
    if (message.reasoning.trim()) {
      return true;
    }

    return message.timeline.some((timelineEntry) => {
      if (timelineEntry.kind === 'tool') {
        return true;
      }

      return timelineEntry.text.trim().length > 0;
    });
  });
}

async function copyAssistantMessage() {
  if (!canCopyMessage.value) {
    return;
  }

  await navigator.clipboard.writeText(copyableMessageText.value);
  copied.value = true;
  resetCopiedStateSoon();
}

function resetCopiedStateSoon() {
  if (copiedResetTimer !== null) {
    window.clearTimeout(copiedResetTimer);
  }

  copiedResetTimer = window.setTimeout(() => {
    copied.value = false;
    copiedResetTimer = null;
  }, 1400);
}

function handleFocusOut(event: FocusEvent) {
  const nextTarget = event.relatedTarget;
  if (nextTarget instanceof Node && event.currentTarget instanceof Node && event.currentTarget.contains(nextTarget)) {
    return;
  }

  isActionBarVisible.value = false;
}

onBeforeUnmount(() => {
  if (copiedResetTimer !== null) {
    window.clearTimeout(copiedResetTimer);
  }
});
</script>
