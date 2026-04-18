<template>
  <section class="context-section">
    <div class="context-section-summary">
      <div class="context-section-meta">
        <span>{{ skills.length ? `${skills.length} available` : 'none' }}</span>
      </div>
      <div class="flex items-center gap-2 text-[9px] text-text-dim">
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
    </div>

    <div v-if="orderedSkills.length" class="space-y-2">
      <div v-if="activeSkills.length" class="space-y-0.5">
        <p class="px-2.5 text-[9px] font-mono uppercase tracking-[0.16em] text-text-dim">Active</p>
        <button
          v-for="skill in activeSkills"
          :key="skill.path"
          class="group flex w-full items-start gap-2 rounded-lg px-2.5 py-1.5 text-left outline-none transition hover:bg-bg-hover/70 focus-visible:bg-bg-hover/70"
          type="button"
          :disabled="busy"
          @mouseenter="handleTriggerEnter(skill, $event)"
          @focusin="handleTriggerEnter(skill, $event)"
          @mouseleave="hideTooltip"
          @focusout="hideTooltip"
          @click="handleTriggerEnter(skill, $event)"
          @dblclick="$emit('open-skill', skill.path)"
        >
          <span class="mt-0.5 h-1.5 w-1.5 shrink-0 rounded-full bg-accent"></span>
          <div class="min-w-0 flex-1">
            <p class="truncate text-[11px] font-medium text-text">
              {{ skill.name }}
            </p>
          </div>
        </button>
      </div>

      <div v-if="availableSkills.length" class="space-y-0.5">
        <p class="px-2.5 text-[9px] font-mono uppercase tracking-[0.16em] text-text-dim">Available</p>
        <button
          v-for="skill in availableSkills"
          :key="skill.path"
          class="group flex w-full items-start gap-2 rounded-lg px-2.5 py-1.5 text-left outline-none transition hover:bg-bg-hover/70 focus-visible:bg-bg-hover/70"
          type="button"
          :disabled="busy"
          @mouseenter="handleTriggerEnter(skill, $event)"
          @focusin="handleTriggerEnter(skill, $event)"
          @mouseleave="hideTooltip"
          @focusout="hideTooltip"
          @click="handleTriggerEnter(skill, $event)"
          @dblclick="$emit('open-skill', skill.path)"
        >
          <span class="mt-0.5 h-1.5 w-1.5 shrink-0 rounded-full bg-text-dim/40"></span>
          <div class="min-w-0 flex-1 space-y-0.5">
            <p class="min-w-0 truncate text-[11px] font-medium text-text-muted">
              {{ skill.name }}
            </p>
            <p v-if="skill.description" class="truncate text-[10px] text-text-dim">
              {{ skill.description }}
            </p>
          </div>
        </button>
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
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { Icon } from '@iconify/vue';
import refreshIcon from '@iconify-icons/lucide/refresh-cw';
import type { SkillItem } from '@/data/mock';

const props = withDefaults(defineProps<{
  skills: SkillItem[];
  busy?: boolean;
}>(), {
  skills: () => [],
  busy: false,
});

defineEmits<{
  refresh: [];
  'open-skill': [path: string];
}>();

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
});
</script>
