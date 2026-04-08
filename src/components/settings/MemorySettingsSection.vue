<template>
  <div class="space-y-5">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">记忆</h3>
          <p class="settings-section-copy">
            这里展示当前角色可见的全部长期记忆，不受本轮匹配结果限制，用来审查、清理和整理项目级与全局级知识。
          </p>
        </div>
        <Button size="sm" @click="emit('create-memory')">新建记忆</Button>
      </div>

      <div class="grid gap-3 md:grid-cols-[160px_160px_minmax(0,1fr)]">
        <div class="dialog-field">
          <label class="dialog-label">层级</label>
          <SettingsSelect v-model="selectedLevel" :options="levelOptions" placeholder="全部层级" />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">类型</label>
          <SettingsSelect v-model="selectedType" :options="typeOptions" placeholder="全部类型" />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">搜索</label>
          <Input
            v-model="searchQuery"
            placeholder="搜索标题、话题或 tags"
          />
        </div>
      </div>

      <div class="mt-4 grid gap-3 md:grid-cols-3">
        <article class="settings-info-card">
          <p class="settings-info-label">总数</p>
          <p class="settings-info-value">{{ props.memories.length }} 条</p>
        </article>
        <article class="settings-info-card">
          <p class="settings-info-label">Project</p>
          <p class="settings-info-value">{{ totalProjectCount }} 条</p>
        </article>
        <article class="settings-info-card">
          <p class="settings-info-label">Global</p>
          <p class="settings-info-value">{{ totalGlobalCount }} 条</p>
        </article>
      </div>

      <p class="mt-3 text-[12px] text-text-dim">
        当前筛选命中 {{ filteredMemories.length }} 条记忆。
      </p>

      <div v-if="loading" class="settings-empty mt-4">
        正在读取记忆列表…
      </div>

      <div v-else-if="!groupedMemories.length" class="settings-empty mt-4">
        当前筛选下还没有记忆。你可以新建一条项目事实、决策、模式或用户偏好。
      </div>

      <div v-else class="mt-5 space-y-5">
        <section v-for="group in groupedMemories" :key="group.topic" class="space-y-3">
          <header class="flex items-center justify-between gap-3 border-b border-[color:var(--ma-line-soft)] pb-2">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <h4 class="truncate text-[13px] font-semibold uppercase tracking-[0.16em] text-text">
                  {{ group.topic }}
                </h4>
                <span class="text-[11px] text-text-dim">{{ group.items.length }} 条</span>
                <span
                  v-if="group.items.length > 5"
                  class="rounded-full border border-[color:color-mix(in_srgb,var(--color-warning)_38%,transparent)] bg-[color:color-mix(in_srgb,var(--color-warning)_16%,transparent)] px-2 py-0.5 text-[10px] uppercase tracking-[0.14em] text-warning"
                >
                  建议合并
                </span>
              </div>
            </div>
          </header>

          <article
            v-for="memory in group.items"
            :key="memory.id"
            class="rounded-2xl border border-[color:var(--ma-line-soft)] px-4 py-3 transition hover:bg-[color:var(--ma-panel-surface-hover)]"
            :class="memory.skip_count >= 5 ? 'opacity-70' : ''"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2 text-[10px] uppercase tracking-[0.16em] text-text-dim">
                  <span class="font-mono">{{ memory.id }}</span>
                  <span class="rounded-full bg-[color:var(--ma-panel-surface-strong)] px-2 py-1 text-[9px] text-text">
                    {{ memory.memory_type }}
                  </span>
                  <span>{{ memory.level }}</span>
                  <span>{{ memory.scope }}</span>
                </div>
                <p class="mt-2 text-[14px] font-medium leading-6 text-text">{{ memory.title }}</p>
                <p class="mt-1 text-[12px] leading-5 text-text-muted">
                  {{ summarizeContent(memory.content) }}
                </p>
                <div v-if="memory.tags.length" class="mt-2 flex flex-wrap gap-1.5">
                  <span
                    v-for="tag in memory.tags"
                    :key="tag"
                    class="rounded-full border border-[color:var(--ma-line-soft)] px-2 py-0.5 text-[10px] text-text-dim"
                  >
                    {{ tag }}
                  </span>
                </div>
              </div>

              <div class="flex shrink-0 items-start gap-1">
                <div class="mr-2 text-right text-[11px] leading-5 text-text-dim">
                  <p title="access_count">↑{{ memory.access_count }}</p>
                  <p title="skip_count">×{{ memory.skip_count }}</p>
                </div>
                <Button variant="ghost" size="icon" :title="`编辑 ${memory.id}`" @click="emit('edit-memory', memory.id)">
                  <Icon :icon="pencilIcon" class="h-4 w-4" />
                </Button>
                <Button variant="ghost" size="icon" :title="`删除 ${memory.id}`" @click="emit('delete-memory', memory.id)">
                  <Icon :icon="trashIcon" class="h-4 w-4" />
                </Button>
              </div>
            </div>
          </article>
        </section>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { Icon } from '@iconify/vue';
import pencilIcon from '@iconify-icons/lucide/pencil';
import trashIcon from '@iconify-icons/lucide/trash-2';
import SettingsSelect from '@/components/SettingsSelect.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { BackendMemoryDetailView } from '@/data/mock';

const props = defineProps<{
  memories: BackendMemoryDetailView[];
  loading?: boolean;
}>();

const emit = defineEmits<{
  'create-memory': [];
  'edit-memory': [memoryId: string];
  'delete-memory': [memoryId: string];
}>();

const searchQuery = ref('');
const selectedLevel = ref('all');
const selectedType = ref('all');

const levelOptions = [
  { value: 'all', label: '全部层级' },
  { value: 'project', label: 'Project' },
  { value: 'global', label: 'Global' },
];

const typeOptions = computed(() => {
  const values = Array.from(new Set(props.memories.map((memory) => memory.memory_type.trim()).filter(Boolean))).sort();
  return [
    { value: 'all', label: '全部类型' },
    ...values.map((value) => ({
      value,
      label: value,
    })),
  ];
});

const filteredMemories = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();

  return props.memories.filter((memory) => {
    if (selectedLevel.value !== 'all' && memory.level !== selectedLevel.value) {
      return false;
    }
    if (selectedType.value !== 'all' && memory.memory_type !== selectedType.value) {
      return false;
    }
    if (!query) {
      return true;
    }

    return [
      memory.title,
      memory.topic,
      memory.tags.join(' '),
      memory.id,
    ].some((field) => field.toLowerCase().includes(query));
  });
});

const groupedMemories = computed(() => {
  const groups = new Map<string, BackendMemoryDetailView[]>();

  for (const memory of filteredMemories.value) {
    const topic = memory.topic.trim() || 'general';
    const existing = groups.get(topic) ?? [];
    existing.push(memory);
    groups.set(topic, existing);
  }

  return Array.from(groups.entries())
    .map(([topic, items]) => ({
      topic,
      items: [...items].sort((left, right) =>
        right.updated_at - left.updated_at || left.id.localeCompare(right.id),
      ),
    }))
    .sort((left, right) => left.topic.localeCompare(right.topic));
});

const totalProjectCount = computed(() => props.memories.filter((memory) => memory.level === 'project').length);
const totalGlobalCount = computed(() => props.memories.filter((memory) => memory.level === 'global').length);

function summarizeContent(content: string) {
  const normalized = content.replace(/\s+/g, ' ').trim();
  if (normalized.length <= 120) {
    return normalized;
  }
  return `${normalized.slice(0, 120)}…`;
}
</script>
