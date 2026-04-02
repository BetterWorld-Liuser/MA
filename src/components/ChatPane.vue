<template>
  <section class="panel flex min-h-0 overflow-hidden flex-col">
    <div class="panel-header flex items-center gap-3">
      <div class="text-[12px] text-text-dim">
        {{ chat.length ? `${chat.length} messages` : 'No messages yet' }}
      </div>
    </div>

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
              <summary class="cursor-pointer list-none text-[11px] uppercase tracking-[0.18em] text-text-muted">
                Tool summaries
              </summary>
              <ul class="mt-2 space-y-1">
                <li v-for="tool in message.tools" :key="`${tool.label}-${tool.summary}`" class="text-xs text-text-muted">
                  <span class="text-text">{{ tool.label }}</span>
                  <span class="text-text-dim"> - {{ tool.summary }}</span>
                </li>
              </ul>
            </details>
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
        </div>
      </article>

      <div ref="bottomAnchor" aria-hidden="true"></div>
    </div>

    <div class="shrink-0 p-3" style="border-top: 1px solid rgba(255, 255, 255, 0.08)">
      <div class="px-0.5">
        <label class="mb-2 block text-[10px] uppercase tracking-[0.22em] text-text-dim" for="message-input">
          Reply
        </label>
        <textarea
          id="message-input"
          ref="composerRef"
          v-model="draft"
          class="min-h-[76px] w-full resize-none rounded-xl bg-transparent px-3 py-2.5 text-[13px] text-text outline-hidden transition focus:bg-bg-secondary/30"
          style="border: 1px solid rgba(255, 255, 255, 0.14)"
          placeholder="Ask March to inspect code, rewrite a function, or explain a change..."
          :disabled="disabled || sending"
          @keydown.enter.exact.prevent="submit"
        ></textarea>
        <div class="mt-2.5 flex items-center justify-between gap-3">
          <p class="text-xs text-text-dim">
            <span class="font-medium">Shift+Enter</span> for a newline. <span class="font-mono">@path/to/file</span> will become open_file later.
          </p>
          <button
            v-if="sending"
            class="pill pill-primary min-w-10 justify-center px-3"
            type="button"
            disabled
            aria-label="Pause generation"
            title="Pause generation"
          >
            <Icon :icon="pauseIcon" class="h-3.5 w-3.5" />
          </button>
          <button v-else class="pill pill-primary" type="button" :disabled="disabled || !draft.trim()" @click="submit">
            Send
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import pauseIcon from '@iconify-icons/lucide/pause';
import MarkdownRender from 'markstream-vue';
import type { ChatMessage, LiveTurn } from '../data/mock';

const props = defineProps<{
  chat: ChatMessage[];
  liveTurn?: LiveTurn;
  disabled?: boolean;
  sending?: boolean;
}>();

const emit = defineEmits<{
  send: [content: string];
}>();

const draft = ref('');
const scrollContainer = ref<HTMLElement | null>(null);
const bottomAnchor = ref<HTMLElement | null>(null);
const composerRef = ref<HTMLTextAreaElement | null>(null);

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

watch(
  () => props.chat.length,
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

function submit() {
  const content = draft.value.trim();
  if (!content || props.disabled || props.sending) {
    return;
  }
  // 发送后立即清空输入框，避免等待后端回包期间仍保留旧内容。
  draft.value = '';
  emit('send', content);
}

function focusComposer() {
  composerRef.value?.focus();
}

defineExpose({
  focusComposer,
});
</script>
