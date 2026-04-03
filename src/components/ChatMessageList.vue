<template>
  <div ref="scrollContainer" class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
    <div v-if="!chat.length" class="empty-state">
      <p class="text-sm text-text">No messages yet.</p>
      <p class="mt-1 text-xs text-text-dim">Start a task from here and March will persist the conversation into the active task.</p>
    </div>

    <article
      v-for="message in chat"
      :key="`${message.role}-${message.time}-${message.author}`"
      class="chat-row"
      :class="message.role === 'assistant' ? 'chat-row-assistant' : 'chat-row-user'"
    >
      <span class="message-avatar shrink-0">{{ message.author.slice(0, 1) }}</span>

      <div class="message-stack" :class="message.role === 'assistant' ? 'items-start' : 'items-end'">
        <div class="message-meta" :class="message.role === 'assistant' ? 'justify-start' : 'justify-end'">
          <span class="text-[13px] font-semibold text-text">{{ message.author }}</span>
          <time class="font-mono text-[11px] text-text-dim">{{ message.time }}</time>
        </div>

        <div
          class="message-bubble"
          :class="message.role === 'assistant' ? 'message-bubble-assistant' : 'message-bubble-user'"
        >
          <MarkdownRender
            v-if="message.role === 'assistant'"
            custom-id="ma-chat-message"
            :content="message.content"
            :final="true"
            :max-live-nodes="0"
            :render-batch-size="16"
            :render-batch-delay="8"
          />
          <p v-else class="whitespace-pre-wrap text-text">{{ message.content }}</p>

          <details v-if="message.tools?.length" class="message-tools">
            <summary class="message-tools-summary">
              <span>{{ formatToolSummaryLabel(message.tools.length) }}</span>
              <span class="message-tools-summary-action">查看</span>
            </summary>
            <ul class="message-tools-list">
              <li v-for="tool in message.tools" :key="`${tool.label}-${tool.summary}`" class="message-tools-item">
                <span class="message-tools-item-label">{{ tool.label }}</span>
                <span class="message-tools-item-separator">·</span>
                <span class="message-tools-item-summary">{{ tool.summary }}</span>
              </li>
            </ul>
          </details>
        </div>

        <div class="message-actions" :class="message.role === 'assistant' ? 'justify-start' : 'justify-end'">
          <button
            class="message-copy-button"
            type="button"
            :aria-label="getCopyButtonLabel(message.content)"
            :title="getCopyButtonLabel(message.content)"
            @click="copyMessage(message.content)"
          >
            <Icon :icon="copiedContent === normalizeCopyContent(message.content) ? checkIcon : copyIcon" class="message-copy-icon" />
          </button>
        </div>
      </div>
    </article>

    <article v-if="liveTurn" class="chat-row chat-row-assistant">
      <span class="message-avatar shrink-0">M</span>

      <div class="message-stack items-start">
        <div class="message-meta justify-start">
          <span class="text-[13px] font-semibold text-text">March</span>
          <time class="font-mono text-[11px] text-text-dim">...</time>
        </div>

        <div class="message-bubble message-bubble-assistant opacity-90">
          <div class="live-status-row">
            <span class="live-status-dots" aria-hidden="true">
              <span></span>
              <span></span>
              <span></span>
            </span>
            <span class="live-status-label">{{ liveTurn.statusLabel }}</span>
          </div>
          <MarkdownRender
            v-if="liveTurn.content"
            custom-id="ma-chat-streaming"
            :content="liveTurn.content"
            :final="liveTurn.state !== 'streaming'"
            :max-live-nodes="0"
            :render-batch-size="16"
            :render-batch-delay="8"
          />
          <p v-else class="mt-2 text-[13px] text-text-dim">
            {{ liveTurn.state === 'error' ? '这轮没有成功完成。' : 'March 正在处理这一轮请求。' }}
          </p>

          <div v-if="liveTurn.tools.length" class="live-tools" aria-label="Live tool summaries">
            <div v-for="tool in liveTurn.tools" :key="tool.id" class="live-tool-item">
              <span class="live-tool-state" :class="`live-tool-state-${tool.state}`"></span>
              <span class="live-tool-text">{{ tool.summary || tool.label }}</span>
            </div>
          </div>
        </div>

        <div class="message-actions justify-start">
          <button
            class="message-copy-button"
            type="button"
            aria-label="拷贝当前回复内容"
            title="拷贝当前回复内容"
            :disabled="!liveTurn.content"
            @click="copyMessage(liveTurn.content)"
          >
            <Icon :icon="copiedContent === normalizeCopyContent(liveTurn.content) && liveTurn.content ? checkIcon : copyIcon" class="message-copy-icon" />
          </button>
        </div>
      </div>
    </article>

    <div ref="bottomAnchor" aria-hidden="true"></div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import checkIcon from '@iconify-icons/lucide/check';
import copyIcon from '@iconify-icons/lucide/copy';
import MarkdownRender from 'markstream-vue';
import type { ChatMessage, LiveTurn } from '@/data/mock';

const props = defineProps<{
  chat: ChatMessage[];
  liveTurn?: LiveTurn;
}>();

const scrollContainer = ref<HTMLElement | null>(null);
const bottomAnchor = ref<HTMLElement | null>(null);
const copiedContent = ref('');
let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null;

const chatLength = computed(() => props.chat.length);

watch(
  chatLength,
  async () => {
    await nextTick();
    scrollToBottom('smooth');
  },
);

watch(
  () => props.liveTurn,
  async (turn, previousTurn) => {
    if (!turn) {
      return;
    }

    await nextTick();
    scrollToBottom(previousTurn ? 'auto' : 'smooth');
  },
  { deep: true },
);

onUnmounted(() => {
  if (copyFeedbackTimer) {
    clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = null;
  }
});

function scrollToBottom(behavior: ScrollBehavior = 'smooth') {
  if (bottomAnchor.value) {
    bottomAnchor.value.scrollIntoView({ behavior, block: 'end' });
    return;
  }

  if (scrollContainer.value) {
    scrollContainer.value.scrollTo({
      top: scrollContainer.value.scrollHeight,
      behavior,
    });
  }
}

async function copyMessage(content: string) {
  const normalized = normalizeCopyContent(content);
  if (!normalized) {
    return;
  }

  await navigator.clipboard.writeText(normalized);
  copiedContent.value = normalized;
  if (copyFeedbackTimer) {
    clearTimeout(copyFeedbackTimer);
  }
  copyFeedbackTimer = setTimeout(() => {
    copiedContent.value = '';
    copyFeedbackTimer = null;
  }, 1600);
}

function getCopyButtonLabel(content: string) {
  if (!normalizeCopyContent(content)) {
    return '当前没有可拷贝内容';
  }
  return copiedContent.value === normalizeCopyContent(content) ? '已复制消息内容' : '拷贝消息内容';
}

function normalizeCopyContent(content: string) {
  return content.trim();
}

function formatToolSummaryLabel(count: number) {
  return `${count} 个动作`;
}
</script>
