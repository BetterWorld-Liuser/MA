<template>
  <div ref="anchorRef" class="settings-select-anchor">
    <div
      class="settings-combobox-shell"
      :class="[
        disabled ? 'settings-combobox-shell-disabled' : '',
        open ? 'settings-combobox-shell-open' : '',
      ]"
    >
      <Input
        ref="inputRef"
        :model-value="modelValue"
        class="settings-combobox-input"
        :placeholder="placeholder"
        :disabled="disabled"
        @focus="handleFocus"
        @input="handleInput"
        @keydown.down.prevent="openMenu"
        @keydown.esc.stop.prevent="closeMenu"
      />
      <button
        class="settings-combobox-toggle"
        type="button"
        :disabled="disabled"
        :aria-expanded="open"
        @mousedown.prevent
        @click="toggleMenu"
      >
        <Icon :icon="chevronDownIcon" class="h-4 w-4" />
      </button>
    </div>

    <Teleport v-if="teleportToBody" to="body">
      <div
        v-if="open"
        ref="panelRef"
        class="settings-select-menu"
        :style="panelStyle"
      >
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
          {{ emptyText }}
        </div>
      </div>
    </Teleport>
    <div
      v-else-if="open"
      ref="panelRef"
      class="settings-select-menu absolute left-0 right-0 top-[calc(100%+0.5rem)] z-[90]"
      :style="inlinePanelStyle"
    >
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
        {{ emptyText }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import chevronDownIcon from '@iconify-icons/lucide/chevron-down';
import { Input } from '@/components/ui/input';

type OptionItem = {
  value: string;
  label: string;
};

const props = withDefaults(defineProps<{
  modelValue: string;
  options: OptionItem[];
  placeholder?: string;
  disabled?: boolean;
  emptyText?: string;
  teleportToBody?: boolean;
}>(), {
  teleportToBody: true,
});

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

const anchorRef = ref<HTMLElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
const inputRef = ref<InstanceType<typeof Input> | null>(null);
const open = ref(false);
const panelStyle = ref<Record<string, string>>({});

const filteredOptions = computed(() => {
  const query = props.modelValue.trim().toLowerCase();
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
    return;
  }
  await nextTick();
  syncPosition();
});

watch(
  () => [props.options, props.modelValue],
  () => {
    if (open.value) {
      void nextTick().then(syncPosition);
    }
  },
  { deep: true },
);

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

function handleFocus() {
  if (!props.modelValue.trim()) {
    openMenu();
  }
}

function handleInput(event: Event) {
  const nextValue = (event.target as HTMLInputElement | null)?.value ?? '';
  emit('update:modelValue', nextValue);
  openMenu();
}

function toggleMenu() {
  if (props.disabled) {
    return;
  }
  open.value = !open.value;
  if (open.value) {
    syncPosition();
    inputRef.value?.focus();
  }
}

function openMenu() {
  if (props.disabled) {
    return;
  }
  open.value = true;
  syncPosition();
}

function closeMenu() {
  open.value = false;
}

function select(value: string) {
  emit('update:modelValue', value);
  closeMenu();
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
    closeMenu();
  }
}
</script>
