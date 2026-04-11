<template>
  <div class="space-y-2">
    <template v-for="entry in entries" :key="entry.kind === 'text' ? entry.textId : entry.toolCallId">
      <MarkdownRender
        v-if="entry.kind === 'text' && entry.text.trim()"
        custom-id="ma-chat-message"
        :content="renderAssistantContent(entry.text)"
        :final="final"
        :render-batch-size="16"
        :render-batch-delay="8"
      />
      <div v-else-if="entry.kind === 'tool'" class="live-tools">
        <div class="live-tool-item" :title="entry.preview || entry.toolName">
          <span class="live-tool-state" :class="`live-tool-state-${entry.status === 'ok' ? 'success' : entry.status === 'error' ? 'error' : 'running'}`"></span>
          <span class="live-tool-text">{{ entry.preview || entry.toolName }}</span>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import MarkdownRender from 'markstream-vue';
import type { AssistantTimelineEntry } from '@/data/mock';

defineProps<{
  entries: AssistantTimelineEntry[];
  final?: boolean;
}>();

const CODE_SPAN_PATTERN = /```[\s\S]*?```|`[^`\n]+`/g;
const BARE_URL_PATTERN = /https?:\/\/[^\s<]+/g;

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
</script>
