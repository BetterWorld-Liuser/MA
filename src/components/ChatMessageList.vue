<template>
  <div class="relative min-h-0 flex-1">
    <div ref="scrollContainer" class="min-h-0 h-full overflow-x-hidden overflow-y-auto" @scroll="handleScroll">
      <div class="chat-stage">
        <div class="chat-thread-shell">
          <Transition name="chat-empty-fade">
            <div v-if="!hasRenderableContent" class="empty-state-overlay">
              <div class="empty-state">
                <p class="text-[12px] text-text">No messages yet.</p>
                <p class="mt-1 text-[10px] text-text-dim">Start a task from here and March will persist the conversation into the active task.</p>
              </div>
            </div>
          </Transition>

          <div class="chat-content-layer" :class="hasRenderableContent ? 'chat-content-layer-visible' : ''" @click.capture="handleMarkdownLinkClick">
            <TransitionGroup name="chat-history" tag="div">
              <component
                :is="entry.kind === 'user_message' ? UserMessageBubble : TurnGroup"
                v-for="entry in timeline"
                :key="entry.kind === 'user_message' ? entry.userMessageId : entry.turnId"
                :entry="entry"
                :active-highlight-key="highlightedEntryKey"
                :reply-targets="replyTargets"
                @preview-image="previewImage = $event"
                @reply-entry="emit('reply-entry', $event)"
                @cancel-turn="emit('cancel-turn', $event)"
                @navigate-reply="navigateToReply"
              />
            </TransitionGroup>
          </div>
        </div>
      </div>
    </div>

    <button
      v-if="showJumpToBottomButton"
      class="jump-to-bottom-button absolute bottom-5 right-5 z-20 inline-flex h-8 w-8 items-center justify-center rounded-full transition focus:outline-none focus:ring-2 focus:ring-accent/35 focus:ring-offset-2 focus:ring-offset-base"
      type="button"
      aria-label="回到底部"
      title="回到底部"
      @click="jumpToBottom"
    >
      <Icon :icon="arrowDownIcon" class="jump-to-bottom-icon h-3.5 w-3.5" />
    </button>

    <Teleport to="body">
      <div v-if="previewImage" class="composer-image-preview-backdrop" @click="previewImage = null">
        <div class="composer-image-preview-panel" @click.stop>
          <button class="composer-image-preview-close" type="button" @click="previewImage = null">关闭</button>
          <img class="composer-image-preview-image" :src="previewImage.previewUrl" :alt="previewImage.name" />
          <p class="composer-image-preview-name">{{ previewImage.name }}</p>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import { invoke } from '@tauri-apps/api/core';
import arrowDownIcon from '@iconify-icons/lucide/arrow-down';
import type { AssistantTimelineEntry, ChatImageAttachment, TaskTimelineEntry } from '@/data/mock';
import type { ComposerReplyPreview } from '@/composables/workspaceApp/types';
import { debugChat } from '@/lib/chatDebug';
import TurnGroup from '@/components/chat/TurnGroup.vue';
import UserMessageBubble from '@/components/chat/UserMessageBubble.vue';

const props = defineProps<{
  timeline: TaskTimelineEntry[];
  taskId?: number | null;
}>();

const emit = defineEmits<{
  'reply-entry': [reply: ComposerReplyPreview];
  'cancel-turn': [turnId: string];
}>();

const scrollContainer = ref<HTMLElement | null>(null);
const previewImage = ref<ChatImageAttachment | null>(null);
const highlightedEntryKey = ref<string | null>(null);
let highlightResetTimer: ReturnType<typeof setTimeout> | null = null;
const hasInitializedTaskPosition = ref(false);
const shouldStickToBottom = ref(true);
const distanceFromBottom = ref(0);
const AUTO_FOLLOW_BOTTOM_THRESHOLD = 48;
const JUMP_TO_BOTTOM_BUTTON_THRESHOLD = 80;

const hasRenderableContent = computed(() => props.timeline.length > 0);
const replyTargets = computed<Record<string, ComposerReplyPreview>>(() =>
  Object.fromEntries(
    props.timeline.map((entry) => {
      if (entry.kind === 'user_message') {
        return [
          buildReplyKey('user_message', entry.userMessageId),
          {
            kind: 'user_message',
            id: entry.userMessageId,
            author: entry.author,
            summary: summarizeReplyText(entry.content),
          } satisfies ComposerReplyPreview,
        ];
      }

      return [
        buildReplyKey('turn', entry.turnId),
        {
          kind: 'turn',
          id: entry.turnId,
          author: entry.agentName,
          summary: summarizeTurnReply(entry),
        } satisfies ComposerReplyPreview,
      ];
    }),
  ),
);
const timelineSignature = computed(() =>
  props.timeline
    .map((entry) =>
      entry.kind === 'user_message'
        ? `${entry.userMessageId}:${entry.content.length}`
        : `${entry.turnId}:${entry.state}:${entry.messages.length}:${entry.messages.at(-1)?.timeline.length ?? 0}`,
    )
    .join('|'),
);

const showJumpToBottomButton = computed(() => {
  if (!scrollContainer.value || !hasRenderableContent.value) {
    return false;
  }

  const hasScrollableOverflow = scrollContainer.value.scrollHeight - scrollContainer.value.clientHeight > 8;
  return hasScrollableOverflow && distanceFromBottom.value > JUMP_TO_BOTTOM_BUTTON_THRESHOLD;
});

watch(
  timelineSignature,
  async () => {
    if (!hasInitializedTaskPosition.value || !shouldStickToBottom.value) {
      return;
    }

    await nextTick();
    scrollToBottom('auto');
  },
);

watch(
  () => props.taskId,
  async (taskId, previousTaskId) => {
    debugChat('chat-message-list', 'watch:task-id', {
      previousTaskId: previousTaskId ?? null,
      taskId: taskId ?? null,
      timelineLength: props.timeline.length,
    });
    hasInitializedTaskPosition.value = false;
    await nextTick();
    shouldStickToBottom.value = true;
    scrollToBottom('auto');
    hasInitializedTaskPosition.value = true;
  },
  { immediate: true },
);

onMounted(async () => {
  await nextTick();
  updateStickToBottom();
});

onUnmounted(() => {
  previewImage.value = null;
  if (highlightResetTimer) {
    clearTimeout(highlightResetTimer);
    highlightResetTimer = null;
  }
});

function scrollToBottom(behavior: ScrollBehavior = 'smooth') {
  if (!scrollContainer.value) {
    return;
  }

  scrollContainer.value.scrollTo({
    top: scrollContainer.value.scrollHeight,
    behavior,
  });
}

function handleScroll() {
  updateStickToBottom();
}

function jumpToBottom() {
  shouldStickToBottom.value = true;
  scrollToBottom('smooth');
}

function updateStickToBottom() {
  if (!scrollContainer.value) {
    distanceFromBottom.value = 0;
    shouldStickToBottom.value = true;
    return;
  }

  distanceFromBottom.value =
    scrollContainer.value.scrollHeight - scrollContainer.value.scrollTop - scrollContainer.value.clientHeight;
  shouldStickToBottom.value = distanceFromBottom.value <= AUTO_FOLLOW_BOTTOM_THRESHOLD;
}

function isOpenableExternalUrl(url: string) {
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}

async function handleMarkdownLinkClick(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Element)) {
    return;
  }

  const anchor = target.closest('a[href]');
  if (!(anchor instanceof HTMLAnchorElement)) {
    return;
  }

  const href = anchor.getAttribute('href')?.trim() ?? '';
  if (!isOpenableExternalUrl(href)) {
    return;
  }

  event.preventDefault();

  try {
    await invoke('open_external_url', { url: href });
  } catch {
    window.open(href, '_blank', 'noopener,noreferrer');
  }
}

async function navigateToReply(reply: ComposerReplyPreview) {
  const key = buildReplyKey(reply.kind, reply.id);
  highlightedEntryKey.value = key;
  if (highlightResetTimer) {
    clearTimeout(highlightResetTimer);
  }
  highlightResetTimer = setTimeout(() => {
    highlightedEntryKey.value = null;
    highlightResetTimer = null;
  }, 1800);

  await nextTick();
  const target = scrollContainer.value?.querySelector<HTMLElement>(`[data-entry-key="${cssEscape(key)}"]`);
  target?.scrollIntoView({
    block: 'center',
    behavior: 'smooth',
  });
}

function summarizeTurnReply(entry: Extract<TaskTimelineEntry, { kind: 'turn' }>) {
  const texts = entry.messages
    .flatMap((message) => message.timeline)
    .filter((timelineEntry): timelineEntry is Extract<AssistantTimelineEntry, { kind: 'text' }> => timelineEntry.kind === 'text');
  const sample = texts.map((timelineEntry) => timelineEntry.text).join(' ').trim();
  if (sample) {
    return summarizeReplyText(sample);
  }
  return `${entry.messages.length} 条消息 / ${entry.state}`;
}

function summarizeReplyText(text: string) {
  const compact = text.replace(/\s+/g, ' ').trim();
  if (!compact) {
    return '无文本内容';
  }
  return compact.length > 72 ? `${compact.slice(0, 72)}…` : compact;
}

function buildReplyKey(kind: ComposerReplyPreview['kind'], id: string) {
  return `${kind}:${id}`;
}

function cssEscape(value: string) {
  if (typeof CSS !== 'undefined' && typeof CSS.escape === 'function') {
    return CSS.escape(value);
  }
  return value.replace(/["\\]/g, '\\$&');
}
</script>

<style scoped>
.jump-to-bottom-button {
  border: 1px solid var(--ma-line-strong);
  background: color-mix(in srgb, var(--ma-panel-elevated) 88%, transparent);
  color: color-mix(in srgb, var(--ma-text-muted) 82%, transparent);
  box-shadow:
    0 10px 22px rgba(0, 0, 0, 0.08),
    inset 0 1px 0 rgba(255, 255, 255, 0.2);
  backdrop-filter: blur(10px);
}

.jump-to-bottom-button:hover {
  border-color: var(--ma-line-strong);
  background: color-mix(in srgb, var(--ma-panel-elevated-strong) 94%, transparent);
  color: var(--ma-text);
}

.jump-to-bottom-icon {
  stroke-width: 2.1;
}

:global(:root:not([data-theme='light'])) .jump-to-bottom-button {
  box-shadow:
    0 10px 24px rgba(0, 0, 0, 0.22),
    inset 0 1px 0 rgba(255, 255, 255, 0.05);
}
</style>
