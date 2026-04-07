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
    <template v-else-if="filteredModelItems.length">
      <button
        v-for="item in filteredModelItems"
        :key="item.modelConfigId"
        class="composer-menu-item composer-menu-item-model flex-col items-start gap-1"
        :class="isModelActive(item.modelConfigId, item.modelId) ? 'composer-menu-item-active' : ''"
        type="button"
        @mousedown.prevent="emit('selectModel', { modelConfigId: item.modelConfigId, model: item.modelId })"
      >
        <span class="w-full text-left">{{ item.displayName }}</span>
        <span class="flex w-full items-center justify-between text-[11px] text-text-dim">
          <span>{{ item.providerName }} · {{ item.modelId }}</span>
          <span>
            {{ isModelActive(item.modelConfigId, item.modelId) ? '✓ ' : '' }}{{ providerTypeLabel(item.providerType) }}
          </span>
        </span>
      </button>
    </template>
    <div v-else-if="!modelItems.length && !modelsLoading" class="composer-menu-empty">当前没有可读模型列表</div>
    <div v-else-if="!modelsLoading" class="composer-menu-empty">没有匹配的模型</div>
  </div>
</template>

<script setup lang="ts">
type FlatModelItem = {
  modelConfigId: number;
  providerId: number;
  providerName: string;
  providerType: string;
  displayName: string;
  modelId: string;
};

defineProps<{
  modelItems: FlatModelItem[];
  filteredModelItems: FlatModelItem[];
  modelSearchQuery: string;
  modelsLoading: boolean;
  modelsRefreshing: boolean;
  providerTypeLabel: (providerType: string) => string;
  isModelActive: (modelConfigId: number, model: string) => boolean;
}>();

const emit = defineEmits<{
  'update:modelSearchQuery': [value: string];
  selectModel: [payload: { modelConfigId: number; model: string }];
}>();
</script>
