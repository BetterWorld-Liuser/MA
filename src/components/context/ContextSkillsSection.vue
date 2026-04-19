<template>
  <section class="context-section">
    <div class="flex items-center justify-between gap-3 px-2.5 py-0.5">
      <span class="text-[10px] text-text-dim" style="font-variant-numeric: tabular-nums;">
        {{ skills.length ? `${skills.length} available` : 'none' }}
      </span>
      <button
        class="task-header-icon-button h-6 w-6"
        type="button"
        :disabled="busy"
        aria-label="刷新可用技能"
        title="刷新可用技能"
        @click="$emit('refresh')"
      >
        <Icon :icon="refreshIcon" class="h-3.5 w-3.5" />
      </button>
    </div>

    <div v-if="orderedSkills.length" class="space-y-3">
      <div v-if="activeSkills.length">
        <div class="skills-group-heading">
          <span class="skills-group-label">Active</span>
          <span class="skills-group-count">{{ activeSkills.length }}</span>
        </div>
        <TransitionGroup name="skill-row" tag="div">
          <div
            v-for="skill in activeSkills"
            :key="skill.path"
            class="skill-row group flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-0.5 text-left outline-none transition hover:bg-bg-hover/70 focus-visible:bg-bg-hover/70"
            tabindex="0"
            @mouseenter="handleTriggerEnter(skill, $event)"
            @focusin="handleTriggerEnter(skill, $event)"
            @mouseleave="hideTooltip"
            @focusout="hideTooltip"
            @click="handleTriggerEnter(skill, $event)"
            @dblclick="handleOpenSkill(skill)"
          >
            <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-accent"></span>
            <p class="min-w-0 flex-1 truncate text-[12px] font-medium text-text">
              {{ skill.name }}
            </p>
            <div class="relative ml-auto flex h-3.5 w-3.5 shrink-0 items-center justify-center">
              <Icon
                v-if="isSkillLocked(skill)"
                :icon="lockIcon"
                class="h-3 w-3 text-text-dim"
                :title="`${skill.name} 已锁定，前往 Open files 解锁`"
              />
              <button
                v-else-if="getSkillOpenFile(skill)"
                class="flex h-3.5 w-3.5 items-center justify-center rounded text-text-dim opacity-0 transition hover:text-text group-hover:opacity-100 focus-visible:opacity-100 disabled:cursor-not-allowed"
                type="button"
                :disabled="busy"
                :aria-label="`Close ${skill.name}`"
                :title="`Close ${skill.name}`"
                @click.stop="handleCloseSkill(skill)"
              >
                <Icon :icon="xIcon" class="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        </TransitionGroup>
      </div>

      <div v-if="availableSkills.length">
        <div class="skills-group-heading">
          <span class="skills-group-label">Available</span>
          <span class="skills-group-count">{{ availableSkills.length }}</span>
        </div>
        <TransitionGroup name="skill-row" tag="div">
          <button
            v-for="skill in availableSkills"
            :key="skill.path"
            class="skill-row group flex w-full items-center gap-2 rounded-lg px-2.5 py-0.5 text-left outline-none transition hover:bg-bg-hover/70 focus-visible:bg-bg-hover/70"
            :class="isPending(skill) ? 'skill-row-pending' : ''"
            type="button"
            :disabled="busy || isPending(skill)"
            @mouseenter="handleTriggerEnter(skill, $event)"
            @focusin="handleTriggerEnter(skill, $event)"
            @mouseleave="hideTooltip"
            @focusout="hideTooltip"
            @click="handleTriggerEnter(skill, $event)"
            @dblclick="handleOpenSkill(skill)"
          >
            <span
              v-if="isPending(skill)"
              class="h-1.5 w-1.5 shrink-0 rounded-full bg-accent animate-pulse"
            ></span>
            <span
              v-else
              class="h-1.5 w-1.5 shrink-0 rounded-full bg-text-dim/40"
            ></span>
            <p class="min-w-0 flex-1 truncate text-[12px] font-medium text-text-muted">
              {{ skill.name }}
            </p>
            <Icon
              v-if="isPending(skill)"
              :icon="loaderIcon"
              class="h-3 w-3 shrink-0 animate-spin text-accent"
            />
          </button>
        </TransitionGroup>
      </div>
    </div>

    <div v-else class="compact-empty">No skills discovered for this workspace</div>
  </section>

  <Teleport to="body">
    <div
      v-if="activeSkill"
      ref="tooltipRef"
      class="pointer-events-none fixed z-[140] w-[300px] rounded-2xl border px-3 py-2.5 text-popover-foreground shadow-[var(--tooltip-shadow)] backdrop-blur-[16px]"
      :style="tooltipStyle"
    >
      <div class="space-y-2">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <p class="truncate font-mono text-[12px] font-semibold text-text">{{ activeSkill.name }}</p>
            <p class="mt-0.5 text-[9px] uppercase tracking-[0.16em] text-text-dim">Skill</p>
          </div>
          <span
            class="mt-1 h-2 w-2 shrink-0 rounded-full"
            :class="activeSkill.opened ? 'bg-accent' : 'bg-text-dim/40'"
            :title="activeSkill.opened ? 'Active' : 'Available'"
          ></span>
        </div>

        <p v-if="activeSkill.description" class="text-[11px] leading-5 text-text-muted">
          {{ activeSkill.description }}
        </p>

        <div class="rounded-xl border border-[color:var(--ma-line-soft)] bg-[color:var(--ma-panel-surface)] px-2.5 py-2">
          <p class="mb-1 text-[8px] uppercase tracking-[0.16em] text-text-dim">Path</p>
          <p class="break-all font-mono text-[10px] leading-4 text-text">{{ activeSkill.path }}</p>
        </div>

        <p class="text-[10px] text-text-dim">双击右侧技能项可直接加入 Open Files。</p>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import refreshIcon from '@iconify-icons/lucide/refresh-cw';
import lockIcon from '@iconify-icons/lucide/lock';
import loaderIcon from '@iconify-icons/lucide/loader-2';
import xIcon from '@iconify-icons/lucide/x';
import type { OpenFileItem, SkillItem } from '@/data/mock';

const props = withDefaults(defineProps<{
  skills: SkillItem[];
  openFiles?: OpenFileItem[];
  busy?: boolean;
}>(), {
  skills: () => [],
  openFiles: () => [],
  busy: false,
});

const emit = defineEmits<{
  refresh: [];
  'open-skill': [path: string];
  'close-file': [scope: string, path: string];
}>();

function normalizePath(path: string) {
  return path.replaceAll('\\', '/').toLowerCase();
}

const openFileLookup = computed(() => {
  const map = new Map<string, OpenFileItem>();
  for (const file of props.openFiles) {
    map.set(normalizePath(file.path), file);
  }
  return map;
});

function getSkillOpenFile(skill: SkillItem): OpenFileItem | undefined {
  return openFileLookup.value.get(normalizePath(skill.path));
}

function isSkillLocked(skill: SkillItem) {
  return getSkillOpenFile(skill)?.locked ?? false;
}

function handleCloseSkill(skill: SkillItem) {
  const file = getSkillOpenFile(skill);
  if (!file || file.locked) {
    return;
  }
  emit('close-file', file.scope, file.path);
}

const PENDING_TIMEOUT_MS = 8000;
const pendingSkills = ref(new Set<string>());
const pendingTimers = new Map<string, ReturnType<typeof setTimeout>>();

function clearPendingKey(key: string) {
  const timer = pendingTimers.get(key);
  if (timer) {
    clearTimeout(timer);
    pendingTimers.delete(key);
  }
  if (pendingSkills.value.has(key)) {
    const next = new Set(pendingSkills.value);
    next.delete(key);
    pendingSkills.value = next;
  }
}

function isPending(skill: SkillItem) {
  return pendingSkills.value.has(normalizePath(skill.path));
}

function handleOpenSkill(skill: SkillItem) {
  if (props.busy || skill.opened) {
    return;
  }
  const key = normalizePath(skill.path);
  if (pendingSkills.value.has(key)) {
    return;
  }
  const next = new Set(pendingSkills.value);
  next.add(key);
  pendingSkills.value = next;
  pendingTimers.set(
    key,
    setTimeout(() => clearPendingKey(key), PENDING_TIMEOUT_MS),
  );
  emit('open-skill', skill.path);
}

watch(
  () => props.skills,
  (nextSkills) => {
    if (!pendingSkills.value.size) {
      return;
    }
    const openedKeys = new Set(
      nextSkills.filter((s) => s.opened).map((s) => normalizePath(s.path)),
    );
    for (const key of [...pendingSkills.value]) {
      if (openedKeys.has(key)) {
        clearPendingKey(key);
      }
    }
  },
  { deep: true },
);

const orderedSkills = computed(() =>
  [...props.skills]
    .sort((a, b) => Number(b.opened) - Number(a.opened) || a.name.localeCompare(b.name)),
);
const activeSkills = computed(() => orderedSkills.value.filter((skill) => skill.opened));
const availableSkills = computed(() => orderedSkills.value.filter((skill) => !skill.opened));

const activeSkill = ref<SkillItem | null>(null);
const activeTrigger = ref<HTMLElement | null>(null);
const tooltipRef = ref<HTMLElement | null>(null);
const tooltipStyle = ref<Record<string, string>>({
  top: '0px',
  left: '0px',
  background: 'var(--ma-panel-popover)',
  borderColor: 'var(--ma-line-soft)',
});

function handleTriggerEnter(skill: SkillItem, event: MouseEvent | FocusEvent) {
  const trigger = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
  if (!trigger) {
    return;
  }

  activeSkill.value = skill;
  activeTrigger.value = trigger;
  void nextTick(updateTooltipPosition);
}

function hideTooltip() {
  activeSkill.value = null;
  activeTrigger.value = null;
}

async function updateTooltipPosition() {
  await nextTick();

  if (!activeTrigger.value || !tooltipRef.value) {
    return;
  }

  const triggerRect = activeTrigger.value.getBoundingClientRect();
  const tooltipRect = tooltipRef.value.getBoundingClientRect();
  const gap = 14;
  const viewportPadding = 12;

  let left = triggerRect.left - tooltipRect.width - gap;
  if (left < viewportPadding) {
    left = triggerRect.right + gap;
  }

  let top = triggerRect.top;
  if (top + tooltipRect.height > window.innerHeight - viewportPadding) {
    top = window.innerHeight - tooltipRect.height - viewportPadding;
  }
  if (top < viewportPadding) {
    top = viewportPadding;
  }

  tooltipStyle.value = {
    top: `${Math.round(top)}px`,
    left: `${Math.round(left)}px`,
    background: 'var(--ma-panel-popover)',
    borderColor: 'var(--ma-line-soft)',
  };
}

function handleViewportChange() {
  if (activeSkill.value) {
    hideTooltip();
  }
}

onMounted(() => {
  window.addEventListener('resize', handleViewportChange);
  window.addEventListener('scroll', handleViewportChange, true);
});

onBeforeUnmount(() => {
  window.removeEventListener('resize', handleViewportChange);
  window.removeEventListener('scroll', handleViewportChange, true);
  for (const timer of pendingTimers.values()) {
    clearTimeout(timer);
  }
  pendingTimers.clear();
});
</script>
