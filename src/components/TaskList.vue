<template>
  <aside class="panel task-column-divider flex min-h-0 flex-col overflow-hidden">
    <div class="panel-header flex items-center justify-between gap-3">
      <p class="truncate text-[13px] font-semibold tracking-[0.01em] text-text">{{ title }}</p>
      <div class="flex items-center gap-1">
        <button
          class="task-header-icon-button"
          type="button"
          :disabled="busy"
          aria-label="收起任务栏"
          title="收起任务栏"
          @click="$emit('collapse')"
        >
          <Icon :icon="panelLeftCloseIcon" class="h-4 w-4" />
        </button>
        <button
          class="task-header-icon-button"
          type="button"
          :disabled="busy"
          aria-label="新建任务"
          title="新建任务"
          @click="$emit('create')"
        >
          <Icon :icon="plusIcon" class="h-4 w-4" />
        </button>
      </div>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto p-1.5">
      <div class="space-y-1.5">
        <div
          v-for="task in tasks"
          :key="task.id"
          class="task-item group relative"
          :class="task.id === activeTaskId ? 'task-item-active' : ''"
          @click="!busy && $emit('select', task.id)"
        >
          <button
            type="button"
            class="min-w-0 flex flex-1 items-center gap-2 text-left"
            :disabled="busy"
            @click.stop="$emit('select', task.id)"
          >
            <span
              v-if="task.activityStatus"
              class="task-item-status-dot"
              :class="task.activityStatus === 'working' ? 'task-item-status-dot-working' : 'task-item-status-dot-review'"
              :title="task.activityStatus === 'working' ? '任务仍在工作' : '任务已完成，等待审阅'"
            ></span>
            <span
              class="min-w-0 flex-1 truncate leading-[1.4]"
              :class="task.id === activeTaskId ? 'text-[12px] font-semibold text-text' : 'text-[12px] font-medium text-text-muted'"
            >
              {{ task.name }}
            </span>
            <span class="task-item-meta-slot">
              <span class="task-item-meta" :class="!busy ? 'group-hover:opacity-0' : ''">{{ task.updatedAt }}</span>
            </span>
          </button>
          <button
            class="task-item-delete"
            :class="busy ? 'invisible' : 'group-hover:opacity-100 group-hover:pointer-events-auto group-hover:text-text group-hover:visible'"
            type="button"
            :disabled="busy"
            :aria-label="`删除 ${task.name}`"
            :title="`删除 ${task.name}`"
            @click.stop="$emit('delete', task.id)"
          >
            <Icon :icon="xIcon" class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>

    <div class="border-t border-[color:var(--border-subtle)] p-1.5">
      <button
        class="task-settings-button"
        type="button"
        :disabled="busy"
        title="打开设置"
        @click="$emit('open-settings')"
      >
        <Icon :icon="settingsIcon" class="h-4 w-4" />
      </button>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue';
import panelLeftCloseIcon from '@iconify-icons/lucide/panel-left-close';
import plusIcon from '@iconify-icons/lucide/plus';
import settingsIcon from '@iconify-icons/lucide/settings-2';
import xIcon from '@iconify-icons/lucide/x';
import type { TaskItem } from '../data/mock';

defineProps<{
  title: string;
  tasks: TaskItem[];
  activeTaskId: string;
  busy?: boolean;
}>();

defineEmits(['select', 'create', 'delete', 'open-settings', 'collapse']);
</script>
