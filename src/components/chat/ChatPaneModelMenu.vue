<template>
  <div class="composer-menu-header">
    <span>可用模型</span>
    <span v-if="modelsRefreshing" class="composer-menu-status">刷新中…</span>
  </div>
  <div class="composer-menu-search-shell">
    <input
      ref="modelSearchRef"
      :value="modelSearchQuery"
      class="app-input composer-menu-search-input"
      type="text"
      placeholder="搜索模型…"
      @input="emit('update:modelSearchQuery', ($event.target as HTMLInputElement).value)"
    />
  </div>
  <div class="composer-menu-list">
    <div v-if="modelsLoading" class="composer-menu-empty">正在读取…</div>
    <template v-else-if="filteredProviderGroups.length">
      <div
        v-for="group in filteredProviderGroups"
        :key="group.providerCacheKey"
        class="border-b border-border/60 last:border-b-0"
      >
        <div class="composer-menu-header">
          <span>{{ group.providerName }}</span>
          <span class="composer-menu-status">{{ providerTypeLabel(group.providerType) }}</span>
        </div>
        <button
          v-for="model in group.filteredModels"
          :key="`${group.providerCacheKey}:${model}`"
          class="composer-menu-item composer-menu-item-model"
          :class="isModelActive(group.providerId, model) ? 'composer-menu-item-active' : ''"
          type="button"
          @mousedown.prevent="emit('selectModel', { providerId: group.providerId, model })"
        >
          <span>{{ model }}</span>
          <span v-if="isModelActive(group.providerId, model)">✓</span>
        </button>
      </div>
    </template>
    <div v-else-if="!providerGroups.length && !modelsLoading" class="composer-menu-empty">当前没有可读模型列表</div>
    <div v-else-if="!modelsLoading" class="composer-menu-empty">没有匹配的模型</div>
  </div>
</template>

<script setup lang="ts">
type FilteredProviderGroup = {
  providerId?: number | null;
  providerName: string;
  providerType: string;
  providerCacheKey: string;
  filteredModels: string[];
};

defineProps<{
  providerGroups: Array<{ providerCacheKey: string }>;
  filteredProviderGroups: FilteredProviderGroup[];
  modelSearchQuery: string;
  modelsLoading: boolean;
  modelsRefreshing: boolean;
  providerTypeLabel: (providerType: string) => string;
  isModelActive: (providerId: number | null | undefined, model: string) => boolean;
}>();

const emit = defineEmits<{
  'update:modelSearchQuery': [value: string];
  selectModel: [payload: { providerId?: number | null; model: string }];
}>();
</script>
