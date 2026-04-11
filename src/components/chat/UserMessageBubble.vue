<template>
  <article
    class="chat-row chat-row-user"
    :class="isHighlighted ? 'ring-1 ring-accent/45 rounded-2xl' : ''"
    :data-entry-key="entryKey"
  >
    <span class="message-avatar shrink-0">{{ entry.author.slice(0, 1) }}</span>

    <div class="message-stack items-end">
      <div class="message-meta justify-end">
        <span class="text-[11px] font-semibold text-text">{{ entry.author }}</span>
        <time class="font-mono text-[9px] text-text-dim">{{ formatEntryTime(entry.ts) }}</time>
        <button class="text-[10px] text-text-dim transition hover:text-text" type="button" @click="$emit('reply-entry', selfReply)">
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
    </div>
  </article>
</template>

<script setup lang="ts">
import { computed } from 'vue';
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
</script>
