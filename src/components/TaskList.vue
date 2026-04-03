<template>
  <aside class="panel task-column-divider flex min-h-0 flex-col overflow-hidden">
    <div class="panel-header flex items-center justify-between gap-3">
      <p class="truncate text-[13px] font-semibold text-text">{{ title }}</p>
      <button class="pill px-1.5" type="button" :disabled="busy" title="新建任务" @click="$emit('create')">
        + New Task
      </button>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto p-2">
      <div class="space-y-1">
        <div
          v-for="task in tasks"
          :key="task.id"
          class="task-item group"
          :class="task.id === activeTaskId ? 'task-item-active' : ''"
        >
          <button
            type="button"
            class="min-w-0 flex flex-1 items-center gap-2 text-left"
            :disabled="busy"
            @click="$emit('select', task.id)"
          >
            <span class="min-w-0 flex-1 truncate text-[13px] font-medium leading-5">{{ task.name }}</span>
            <span class="task-item-meta group-hover:hidden">{{ task.updatedAt }}</span>
          </button>
          <button
            class="task-item-delete hidden group-hover:inline-flex"
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

    <div class="border-t border-white/8 p-2">
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
import settingsIcon from '@iconify-icons/lucide/settings-2';
import xIcon from '@iconify-icons/lucide/x';
import type { TaskItem } from '../data/mock';

defineProps<{
  title: string;
  tasks: TaskItem[];
  activeTaskId: string;
  busy?: boolean;
}>();

defineEmits<{
  select: [taskId: string];
  create: [];
  delete: [taskId: string];
  openSettings: [];
}>();
</script>
