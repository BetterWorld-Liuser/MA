<template>
  <div class="space-y-5">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div class="min-w-0">
          <h3 class="settings-section-title">记忆</h3>
          <p class="settings-section-copy">
            当前角色可见的全部长期记忆，用于审查、清理和整理项目级与全局级知识。
          </p>
          <div class="memory-stat-strip">
            <span><span class="memory-stat-label">总数</span><span class="memory-stat-value">{{ props.memories.length }}</span></span>
            <span class="memory-stat-divider" aria-hidden="true"></span>
            <span><span class="memory-stat-label">Project</span><span class="memory-stat-value">{{ totalProjectCount }}</span></span>
            <span class="memory-stat-divider" aria-hidden="true"></span>
            <span><span class="memory-stat-label">Global</span><span class="memory-stat-value">{{ totalGlobalCount }}</span></span>
          </div>
        </div>
        <Button size="sm" @click="emit('create-memory')">新建记忆</Button>
      </div>

      <div class="memory-filter-bar">
        <div class="grid flex-1 gap-3 md:grid-cols-[160px_160px_minmax(0,1fr)]">
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
      </div>

      <p class="memory-result-count">
        当前筛选命中 <span class="text-text">{{ filteredMemories.length }}</span> 条记忆
      </p>

      <div v-if="loading" class="settings-empty mt-4">
        正在读取记忆列表…
      </div>

      <div v-else-if="!groupedMemories.length" class="settings-empty mt-4">
        当前筛选下还没有记忆。你可以新建一条项目事实、决策、模式或用户偏好。
      </div>

      <div v-else class="mt-4 space-y-6">
        <section v-for="group in groupedMemories" :key="group.topic" class="memory-topic-group">
          <header class="memory-topic-header">
            <h4 class="memory-topic-title">{{ group.topic }}</h4>
            <span class="memory-topic-count">{{ group.items.length }} 条</span>
            <span
              v-if="group.items.length > 5"
              class="memory-topic-warn"
            >
              建议合并
            </span>
          </header>

          <article
            v-for="memory in group.items"
            :key="memory.id"
            class="memory-card group"
            :class="memory.skip_count >= 5 ? 'memory-card-faded' : ''"
          >
            <div class="memory-card-top">
              <p class="memory-card-title">{{ memory.title }}</p>
              <div class="memory-card-actions">
                <Button variant="ghost" size="icon" :title="`编辑 ${memory.id}`" @click="emit('edit-memory', memory.id)">
                  <Icon :icon="pencilIcon" class="h-4 w-4" />
                </Button>
                <Button variant="ghost" size="icon" :title="`删除 ${memory.id}`" @click="emit('delete-memory', memory.id)">
                  <Icon :icon="trashIcon" class="h-4 w-4" />
                </Button>
              </div>
            </div>

            <p class="memory-card-content">
              {{ summarizeContent(memory.content) }}
            </p>

            <div class="memory-card-meta">
              <span class="memory-chip memory-chip-type">{{ memory.memory_type }}</span>
              <span class="memory-chip">{{ memory.level }}</span>
              <span class="memory-chip">{{ memory.scope }}</span>
              <span
                v-for="tag in memory.tags"
                :key="tag"
                class="memory-chip memory-chip-tag"
              >
                {{ tag }}
              </span>
              <span class="memory-card-meta-spacer"></span>
              <span class="memory-card-id" :title="memory.id">{{ memory.id }}</span>
              <span class="memory-card-counter" :title="`被引用 ${memory.access_count} 次 / 被跳过 ${memory.skip_count} 次`">
                ↑{{ memory.access_count }} · ×{{ memory.skip_count }}
              </span>
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
