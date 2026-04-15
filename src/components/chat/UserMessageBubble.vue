<template>
  <article
    class="chat-row chat-row-user message-row-user"
    :class="isHighlighted ? 'ring-1 ring-accent/45 rounded-2xl' : ''"
    :data-entry-key="entryKey"
  >
    <span class="message-avatar shrink-0">{{ entry.author.slice(0, 1) }}</span>

    <div class="message-stack items-end">
      <div class="message-meta justify-end">
        <span class="text-[12px] font-semibold text-text">{{ entry.author }}</span>
        <time class="font-mono text-[10px] text-text-dim">{{ formatEntryTime(entry.ts) }}</time>
        <button class="text-[10px] text-text-dim/80 transition hover:text-text" type="button" @click="$emit('reply-entry', selfReply)">
          引用
        </button>
      </div>

      <div class="message-bubble message-bubble-user">
        <div v-if="resolvedReplies.length" class="mb-3 space-y-2">
          <button
            v-for="reply in resolvedReplies"
            :key="`${reply.kind}:${reply.id}`"
            class="reply-preview-card"
            type="button"
            @click="$emit('navigate-reply', reply)"
          >
            <span class="reply-preview-meta">{{ reply.author }}</span>
            <span class="reply-preview-text">{{ reply.summary }}</span>
          </button>
        </div>

        <div v-if="entry.images?.length" class="message-image-grid">
          <button
            v-for="image in entry.images"
            :key="image.id"
            class="message-image-card"
            type="button"
            @click="$emit('preview-image', image)"
          >
            <img class="message-image-thumb" :src="image.previewUrl" :alt="image.name" />
            <span class="message-image-caption">{{ image.name }}</span>
          </button>
        </div>

        <p class="whitespace-pre-wrap text-[12px] leading-[1.5] text-text">{{ entry.content }}</p>
      </div>

      <div
        class="message-actions message-actions-user"
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
          :title="copyButtonTitle"
          :aria-label="copyButtonTitle"
          :disabled="!canCopyMessage"
          @click="copyUserMessage"
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
import checkIcon from '@iconify-icons/lucide/check';
import copyIcon from '@iconify-icons/lucide/copy';
import type { ChatImageAttachment, UserMessage } from '@/data/mock';
import type { ComposerReplyPreview } from '@/composables/workspaceApp/types';

const props = defineProps<{
  entry: UserMessage;
  activeHighlightKey?: string | null;
  replyTargets?: Record<string, ComposerReplyPreview>;
}>();

defineEmits<{
  'preview-image': [image: ChatImageAttachment];
  'reply-entry': [reply: ComposerReplyPreview];
  'navigate-reply': [reply: ComposerReplyPreview];
}>();

const entryKey = computed(() => `user_message:${props.entry.userMessageId}`);
const isHighlighted = computed(() => props.activeHighlightKey === entryKey.value);
const copied = ref(false);
const isActionBarVisible = ref(false);
const canCopyMessage = computed(() => props.entry.content.trim().length > 0);
const copyFeedbackIcon = computed(() => (copied.value ? checkIcon : copyIcon));
const copyButtonTitle = computed(() => {
  if (!canCopyMessage.value) {
    return '暂无可复制内容';
  }

  return copied.value ? '已复制' : '复制消息';
});
const selfReply = computed<ComposerReplyPreview>(() => ({
  kind: 'user_message',
  id: props.entry.userMessageId,
  author: props.entry.author,
  summary: summarizeReplyText(props.entry.content),
}));
const resolvedReplies = computed(() =>
  props.entry.replies
    .map((reply) => props.replyTargets?.[`${reply.kind}:${reply.id}`])
    .filter((reply): reply is ComposerReplyPreview => !!reply),
);
let copiedResetTimer: number | null = null;

function formatEntryTime(timestamp: number) {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function summarizeReplyText(text: string) {
  const compact = text.replace(/\s+/g, ' ').trim();
  if (!compact) {
    return '无文本内容';
  }
  return compact.length > 72 ? `${compact.slice(0, 72)}…` : compact;
}

async function copyUserMessage() {
  if (!canCopyMessage.value) {
    return;
  }

  await navigator.clipboard.writeText(props.entry.content.trim());
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
