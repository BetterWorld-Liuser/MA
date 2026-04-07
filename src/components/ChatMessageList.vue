<template>
  <div class="relative min-h-0 flex-1">
    <div ref="scrollContainer" class="min-h-0 h-full overflow-x-hidden overflow-y-auto px-2.5 py-2" @scroll="handleScroll">
      <div class="chat-stage">
      <Transition name="chat-empty-fade">
        <div v-if="!hasRenderableContent" class="empty-state-overlay">
          <div class="empty-state">
            <p class="text-[12px] text-text">No messages yet.</p>
            <p class="mt-1 text-[10px] text-text-dim">Start a task from here and March will persist the conversation into the active task.</p>
          </div>
        </div>
      </Transition>

      <div class="chat-content-layer" :class="hasRenderableContent ? 'chat-content-layer-visible' : ''">
        <TransitionGroup name="chat-history" tag="div">
          <article
            v-for="(message, index) in chat"
            :key="messageKey(message, index)"
            class="chat-row"
            :class="message.role === 'assistant' ? 'chat-row-assistant' : 'chat-row-user'"
          >
            <span class="message-avatar shrink-0">{{ message.author.slice(0, 1) }}</span>

            <div class="message-stack" :class="message.role === 'assistant' ? 'items-start' : 'items-end'">
              <div class="message-meta" :class="message.role === 'assistant' ? 'justify-start' : 'justify-end'">
                <span class="text-[11px] font-semibold text-text">{{ message.author }}</span>
                <time class="font-mono text-[9px] text-text-dim">{{ message.time }}</time>
                <span
                  v-if="message.variant === 'intermediate'"
                  class="message-meta-badge"
                >
                  过程
                </span>
              </div>

              <div
                class="message-bubble"
                :class="[
                  message.role === 'assistant' ? 'message-bubble-assistant' : 'message-bubble-user',
                  message.variant === 'intermediate' ? 'message-bubble-intermediate' : '',
                ]"
                @click.capture="message.role === 'assistant' ? handleMarkdownLinkClick($event) : undefined"
              >
                <div v-if="message.images?.length" class="message-image-grid">
                  <button
                    v-for="image in message.images"
                    :key="image.id"
                    class="message-image-card"
                    type="button"
                    @click="previewImage = image"
                  >
                    <img class="message-image-thumb" :src="image.previewUrl" :alt="image.name" />
                    <span class="message-image-caption">{{ image.name }}</span>
                  </button>
                </div>

                <MarkdownRender
                  v-if="message.role === 'assistant'"
                  custom-id="ma-chat-message"
                  :content="renderAssistantContent(message.content)"
                  :final="true"
                  :max-live-nodes="0"
                  :render-batch-size="16"
                  :render-batch-delay="8"
                />
                <p v-else class="whitespace-pre-wrap text-[12px] leading-[1.5] text-text">{{ message.content }}</p>

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
        </TransitionGroup>

        <article v-if="liveTurn" class="chat-row chat-row-assistant">
          <span class="message-avatar shrink-0">{{ liveTurn.author.slice(0, 1) }}</span>

          <div class="message-stack items-start">
            <div class="message-meta justify-start">
              <span class="text-[11px] font-semibold text-text">{{ liveTurn.author }}</span>
              <time class="font-mono text-[9px] text-text-dim">...</time>
            </div>

            <Transition name="live-turn-swap" mode="out-in">
              <div
                :key="liveTurnContentKey"
                class="message-bubble message-bubble-assistant opacity-90"
                :class="liveTurn.state === 'error' ? 'live-bubble-error' : ''"
                @click.capture="handleMarkdownLinkClick"
              >
                <div class="live-status-row" :class="liveTurn.state === 'error' ? 'live-status-row-error' : ''">
                  <span class="live-status-dots" :class="liveTurn.state === 'error' ? 'live-status-dots-error' : ''" aria-hidden="true">
                    <span></span>
                    <span></span>
                    <span></span>
                  </span>
                  <span class="live-status-label" :class="liveTurn.state === 'error' ? 'text-error' : ''">{{ liveTurn.statusLabel }}</span>
                </div>
                <MarkdownRender
                  v-if="liveTurn.content"
                  custom-id="ma-chat-streaming"
                  :content="renderAssistantContent(liveTurn.content)"
                  :final="liveTurn.state !== 'streaming'"
                  :max-live-nodes="0"
                  :render-batch-size="16"
                  :render-batch-delay="8"
                />
                <p v-else class="mt-1 text-[11px]" :class="liveTurn.state === 'error' ? 'text-error' : 'text-text-dim'">
                  {{ liveTurn.state === 'error' ? (liveTurn.errorMessage || '这轮没有成功完成。') : `${liveTurn.author} 正在处理这一轮请求。` }}
                </p>

                <p
                  v-if="liveTurn.state === 'error' && liveTurn.content && liveTurn.errorMessage"
                  class="mt-2 whitespace-pre-wrap text-[11px] text-error"
                >
                  {{ liveTurn.errorMessage }}
                </p>

                <div v-if="liveTurn.tools.length" class="live-tools" aria-label="Live tool summaries">
                  <div v-for="tool in liveTurn.tools" :key="tool.id" class="live-tool-item">
                    <span class="live-tool-state" :class="`live-tool-state-${tool.state}`"></span>
                    <span class="live-tool-text">{{ tool.summary || tool.label }}</span>
                  </div>
                </div>
              </div>
            </Transition>

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
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import { invoke } from '@tauri-apps/api/core';
import arrowDownIcon from '@iconify-icons/lucide/arrow-down';
import checkIcon from '@iconify-icons/lucide/check';
import copyIcon from '@iconify-icons/lucide/copy';
import MarkdownRender from 'markstream-vue';
import type { ChatImageAttachment, ChatMessage, LiveTurn } from '@/data/mock';

const props = defineProps<{
  chat: ChatMessage[];
  liveTurn?: LiveTurn;
  taskId?: number | null;
}>();

const scrollContainer = ref<HTMLElement | null>(null);
const copiedContent = ref('');
const previewImage = ref<ChatImageAttachment | null>(null);
const hasInitializedTaskPosition = ref(false);
const shouldStickToBottom = ref(true);
const distanceFromBottom = ref(0);
let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null;
const CODE_SPAN_PATTERN = /```[\s\S]*?```|`[^`\n]+`/g;
const BARE_URL_PATTERN = /https?:\/\/[^\s<]+/g;
const AUTO_FOLLOW_BOTTOM_THRESHOLD = 48;
const JUMP_TO_BOTTOM_BUTTON_THRESHOLD = 80;

const chatLength = computed(() => props.chat.length);
const hasRenderableContent = computed(() => props.chat.length > 0 || !!props.liveTurn);
const showJumpToBottomButton = computed(() => {
  if (!scrollContainer.value || !hasRenderableContent.value) {
    return false;
  }

  const hasScrollableOverflow = scrollContainer.value.scrollHeight - scrollContainer.value.clientHeight > 8;
  return hasScrollableOverflow && distanceFromBottom.value > JUMP_TO_BOTTOM_BUTTON_THRESHOLD;
});
const liveTurnContentKey = computed(() => {
  if (!props.liveTurn) {
    return 'live-turn-empty';
  }

  return [
    props.liveTurn.turnId,
    props.liveTurn.transitionKey ?? 0,
    props.liveTurn.state === 'error' ? 'error' : 'active',
  ].join('::');
});

watch(
  chatLength,
  async (length, previousLength) => {
    if (!hasInitializedTaskPosition.value) {
      return;
    }

    if ((previousLength ?? 0) >= length) {
      return;
    }

    if (!shouldStickToBottom.value) {
      return;
    }

    await nextTick();
    scrollToBottom('auto');
  },
);

watch(
  () => props.taskId,
  async () => {
    hasInitializedTaskPosition.value = false;
    await nextTick();
    shouldStickToBottom.value = true;
    scrollToBottom('auto');
    hasInitializedTaskPosition.value = true;
  },
  { immediate: true },
);

watch(
  () => props.liveTurn,
  async (turn, previousTurn) => {
    if (!turn) {
      return;
    }

    if (!shouldStickToBottom.value && previousTurn) {
      return;
    }

    await nextTick();
    scrollToBottom('auto');
  },
  { deep: true },
);

onMounted(async () => {
  await nextTick();
  updateStickToBottom();
});

onUnmounted(() => {
  if (copyFeedbackTimer) {
    clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = null;
  }
});

function scrollToBottom(behavior: ScrollBehavior = 'smooth') {
  if (scrollContainer.value) {
    scrollContainer.value.scrollTo({
      top: scrollContainer.value.scrollHeight,
      behavior,
    });
  }
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

  // 只在用户仍停留在底部附近时自动跟随，避免最终沉淀消息和输入区重排时“抢滚动”。
  distanceFromBottom.value =
    scrollContainer.value.scrollHeight - scrollContainer.value.scrollTop - scrollContainer.value.clientHeight;
  shouldStickToBottom.value = distanceFromBottom.value <= AUTO_FOLLOW_BOTTOM_THRESHOLD;
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

function renderAssistantContent(content: string) {
  return autolinkBareUrls(content);
}

function autolinkBareUrls(content: string) {
  let result = '';
  let lastIndex = 0;

  for (const match of content.matchAll(CODE_SPAN_PATTERN)) {
    const matchIndex = match.index ?? 0;
    result += autolinkTextSegment(content.slice(lastIndex, matchIndex));
    result += match[0];
    lastIndex = matchIndex + match[0].length;
  }

  result += autolinkTextSegment(content.slice(lastIndex));
  return result;
}

function autolinkTextSegment(segment: string) {
  return segment.replace(BARE_URL_PATTERN, (rawUrl, offset, source) => {
    const previousChar = offset > 0 ? source[offset - 1] : '';
    if (previousChar === '(' || previousChar === '[' || previousChar === '<' || previousChar === '"' || previousChar === '\'') {
      return rawUrl;
    }

    const trimmed = trimTrailingUrlPunctuation(rawUrl);
    if (!trimmed.url || !isOpenableExternalUrl(trimmed.url)) {
      return rawUrl;
    }

    return `<${trimmed.url}>${trimmed.trailing}`;
  });
}

function trimTrailingUrlPunctuation(rawUrl: string) {
  let url = rawUrl;
  let trailing = '';

  while (url.length > 0) {
    const lastChar = url[url.length - 1];
    if (!'),.!?;:'.includes(lastChar)) {
      break;
    }
    trailing = `${lastChar}${trailing}`;
    url = url.slice(0, -1);
  }

  return { url, trailing };
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

function messageKey(message: ChatMessage, index: number) {
  if (message.id) {
    return message.id;
  }

  return [
    message.role,
    message.author,
    message.timestamp ?? message.time,
    message.content,
    index,
  ].join('::');
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
