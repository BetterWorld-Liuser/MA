<template>
  <div ref="anchorRef" class="settings-select-anchor">
    <button
      class="settings-select-button"
      type="button"
      :disabled="disabled"
      :aria-expanded="open"
      @click="toggle"
    >
      <span class="truncate" :class="selectedOption ? 'text-text' : 'text-text-dim'">
        {{ selectedOption?.label ?? placeholder }}
      </span>
      <Icon :icon="chevronDownIcon" class="h-4 w-4 shrink-0 text-text-dim" />
    </button>

    <Teleport v-if="teleportToBody" to="body">
      <div
        v-if="open"
        ref="panelRef"
        class="settings-select-menu"
        :style="panelStyle"
      >
        <div v-if="searchable" class="settings-select-search-wrap">
          <input
            ref="searchInputRef"
            v-model="searchQuery"
            class="settings-select-search"
            type="text"
            :placeholder="searchPlaceholder"
            @keydown.esc.stop.prevent="open = false"
          />
        </div>
        <button
          v-for="option in filteredOptions"
          :key="option.value"
          class="settings-select-option"
          :class="option.value === modelValue ? 'settings-select-option-active' : ''"
          type="button"
          @pointerdown.stop
          @click="select(option.value)"
        >
          <span class="truncate">{{ option.label }}</span>
          <span v-if="option.value === modelValue" class="text-accent">✓</span>
        </button>
        <div v-if="!filteredOptions.length" class="settings-select-empty">
          没有匹配的结果
        </div>
      </div>
    </Teleport>
    <div
      v-else-if="open"
      ref="panelRef"
      class="settings-select-menu absolute left-0 right-0 top-[calc(100%+0.5rem)] z-[90]"
      :style="inlinePanelStyle"
    >
      <div v-if="searchable" class="settings-select-search-wrap">
        <input
          ref="searchInputRef"
          v-model="searchQuery"
          class="settings-select-search"
          type="text"
          :placeholder="searchPlaceholder"
          @keydown.esc.stop.prevent="open = false"
        />
      </div>
      <button
        v-for="option in filteredOptions"
        :key="option.value"
        class="settings-select-option"
        :class="option.value === modelValue ? 'settings-select-option-active' : ''"
        type="button"
        @pointerdown.stop
        @click="select(option.value)"
      >
        <span class="truncate">{{ option.label }}</span>
        <span v-if="option.value === modelValue" class="text-accent">✓</span>
      </button>
      <div v-if="!filteredOptions.length" class="settings-select-empty">
        没有匹配的结果
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import chevronDownIcon from '@iconify-icons/lucide/chevron-down';

type OptionItem = {
  value: string;
  label: string;
};

const props = withDefaults(defineProps<{
  modelValue: string;
  options: OptionItem[];
  placeholder?: string;
  disabled?: boolean;
  searchable?: boolean;
  searchPlaceholder?: string;
  teleportToBody?: boolean;
}>(), {
  teleportToBody: true,
});

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

const anchorRef = ref<HTMLElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
const searchInputRef = ref<HTMLInputElement | null>(null);
const open = ref(false);
const panelStyle = ref<Record<string, string>>({});
const searchQuery = ref('');

const selectedOption = computed(() =>
  props.options.find((option) => option.value === props.modelValue),
);

const filteredOptions = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) {
    return props.options;
  }
  return props.options.filter((option) => option.label.toLowerCase().includes(query));
});

const inlinePanelStyle = computed(() => ({
  maxHeight: panelStyle.value.maxHeight ?? '320px',
}));

watch(open, async (nextOpen) => {
  if (!nextOpen) {
    searchQuery.value = '';
    return;
  }
  await nextTick();
  syncPosition();
  if (props.searchable) {
    searchInputRef.value?.focus();
    searchInputRef.value?.select();
  }
});

watch(
  () => props.options,
  () => {
    if (open.value) {
      void nextTick().then(syncPosition);
    }
  },
  { deep: true },
);

watch(searchQuery, () => {
  if (open.value) {
    void nextTick().then(syncPosition);
  }
});

onMounted(() => {
  window.addEventListener('resize', syncPosition);
  window.addEventListener('scroll', syncPosition, true);
  window.addEventListener('pointerdown', handlePointerDown, true);
});

onUnmounted(() => {
  window.removeEventListener('resize', syncPosition);
  window.removeEventListener('scroll', syncPosition, true);
  window.removeEventListener('pointerdown', handlePointerDown, true);
});

function toggle() {
  if (props.disabled) {
    return;
  }
  open.value = !open.value;
  if (open.value) {
    syncPosition();
  }
}

function select(value: string) {
  emit('update:modelValue', value);
  open.value = false;
  searchQuery.value = '';
}

function syncPosition() {
  if (!open.value || !anchorRef.value) {
    return;
  }

  if (!props.teleportToBody) {
    const viewportHeight = window.innerHeight;
    const rect = anchorRef.value.getBoundingClientRect();
    const viewportPadding = 20;
    const gap = 8;
    const availableHeight = Math.max(160, Math.min(320, viewportHeight - rect.bottom - viewportPadding - gap));
    panelStyle.value = {
      maxHeight: `${availableHeight}px`,
    };
    return;
  }

  const rect = anchorRef.value.getBoundingClientRect();
  const panelHeight = panelRef.value?.offsetHeight ?? 320;
  const viewportHeight = window.innerHeight;
  const viewportPadding = 20;
  const gap = 8;
  const spaceBelow = viewportHeight - rect.bottom - viewportPadding;
  const spaceAbove = rect.top - viewportPadding;
  const shouldOpenUpward = spaceBelow < 220 && spaceAbove > spaceBelow;
  const maxHeight = shouldOpenUpward
    ? Math.max(160, Math.min(320, spaceAbove - gap))
    : Math.max(160, Math.min(320, spaceBelow - gap));

  const top = shouldOpenUpward
    ? Math.max(viewportPadding, rect.top - Math.min(panelHeight, maxHeight) - gap)
    : rect.bottom + gap;

  panelStyle.value = {
    top: `${top}px`,
    left: `${rect.left}px`,
    width: `${rect.width}px`,
    maxHeight: `${maxHeight}px`,
  };
}

function handlePointerDown(event: Event) {
  if (!open.value) {
    return;
  }

  const target = event.target as Node | null;
  const clickedAnchor = !!(target && anchorRef.value?.contains(target));
  const clickedPanel = !!(target && panelRef.value?.contains(target));
  if (!clickedAnchor && !clickedPanel) {
    open.value = false;
  }
}
</script>
