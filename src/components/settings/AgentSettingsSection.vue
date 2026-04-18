<template>
  <div class="settings-grid">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">角色</h3>
          <p class="settings-section-copy">管理 March 和可复用的自定义角色。它们会出现在聊天里的 `@agent` 召唤链路中。</p>
        </div>
        <Button variant="outline" size="sm" @click="emit('startCreateAgent')">新增角色</Button>
      </div>

      <div v-if="settings?.agents.length" class="space-y-3">
        <article
          v-for="agent in settings.agents"
          :key="agent.name"
          class="settings-provider-card"
          :class="agent.name === activeAgentName ? 'settings-provider-card-active' : ''"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="h-3 w-3 rounded-full" :style="{ background: agent.avatarColor }"></span>
                <h4 class="truncate text-[14px] font-medium text-text">{{ agent.displayName }}</h4>
                <span v-if="agent.isBuiltIn" class="settings-default-badge">March</span>
              </div>
              <p class="mt-1 font-mono text-[11px] text-text-dim">@{{ agent.name }}</p>
              <p class="mt-2 text-[12px] leading-5 text-text-muted">{{ agent.description }}</p>
              <p class="mt-2 line-clamp-2 text-[11px] leading-5 text-text-dim">{{ agent.systemPrompt }}</p>
              <p class="mt-2 text-[11px] text-text-dim">
                {{ formatAgentBinding(agent.providerId, agent.modelId) }} · {{ formatAgentSource(agent.source) }}
              </p>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <Button variant="ghost" size="sm" @click="emit('startEditAgent', agent)">编辑</Button>
              <Button
                v-if="agent.isBuiltIn"
                variant="ghost"
                size="sm"
                @click="emit('restoreMarchPrompt')"
              >
                恢复默认
              </Button>
              <Button
                v-else-if="agent.source === 'user'"
                variant="ghost"
                size="sm"
                class="text-[color:var(--destructive-text)] hover:text-[color:var(--destructive-text)]"
                @click="emit('deleteAgent', agent.name)"
              >
                删除
              </Button>
            </div>
          </div>
        </article>
      </div>
      <div v-else class="settings-empty">
        还没有角色配置。你可以保留默认 March，也可以再加 reviewer、architect 之类的辅助角色。
      </div>
    </section>

    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">{{ editingBuiltInMarch ? '编辑 March' : activeAgentName ? '编辑角色' : '新增角色' }}</h3>
          <p class="settings-section-copy">角色提示词定义它的职责和风格；模型绑定可选，留空时跟随任务默认模型。</p>
        </div>
      </div>

      <form class="space-y-4" @submit.prevent="emit('submitAgent')">
        <div class="grid gap-4 md:grid-cols-2">
          <div class="dialog-field">
            <label class="dialog-label">角色名</label>
            <Input v-model="agentName" :disabled="editingBuiltInMarch || !!activeAgentName" placeholder="reviewer" />
          </div>
          <div class="dialog-field">
            <label class="dialog-label">显示名</label>
            <Input v-model="agentDisplayName" placeholder="代码审查员" />
          </div>
        </div>

        <div class="dialog-field">
          <label class="dialog-label">短描述</label>
          <Input
            v-model="agentDescription"
            :disabled="editingBuiltInMarch"
            placeholder="一句话说明这个角色主要负责什么"
          />
          <p class="dialog-hint">
            用于 `@` 面板、角色列表和 prompt 里的 agent roster。尽量保持简短稳定。
          </p>
        </div>

        <div class="grid gap-4 md:grid-cols-2">
          <div class="dialog-field">
            <label class="dialog-label">头像颜色</label>
            <Input v-model="agentAvatarColor" placeholder="#3B82F6" />
          </div>
          <div class="dialog-field">
            <label class="dialog-label">绑定 Provider</label>
            <SettingsSelect
              v-model="agentProviderIdString"
              :options="agentProviderOptions"
              placeholder="跟随任务默认"
            />
          </div>
        </div>

        <div class="dialog-field">
          <label class="dialog-label">绑定模型</label>
          <SettingsSelect
            v-if="agentModelOptions.length"
            v-model="agentModelId"
            :options="agentModelOptions"
            placeholder="跟随任务默认"
            searchable
            search-placeholder="搜索模型…"
          />
          <Input v-else v-model="agentModelId" placeholder="留空则跟随任务默认" />
        </div>

        <div class="dialog-field">
          <label class="dialog-label">System Prompt</label>
          <Textarea v-model="agentSystemPrompt" class="min-h-[220px]" placeholder="描述这个角色的职责、风格和边界…" />
        </div>

        <div class="flex items-center justify-end gap-2">
          <Button variant="ghost" type="button" @click="emit('resetAgentForm')">清空</Button>
          <Button
            type="submit"
            :disabled="busy || !agentDisplayName.trim() || (!editingBuiltInMarch && !agentDescription.trim()) || !agentSystemPrompt.trim() || !resolvedAgentName"
          >
            {{ editingBuiltInMarch || activeAgentName ? '保存角色' : '创建角色' }}
          </Button>
        </div>
      </form>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import SettingsSelect from '@/components/SettingsSelect.vue';
import type { ProviderSettingsView } from '@/data/mock';

const props = defineProps<{
  settings: ProviderSettingsView | null;
  busy?: boolean;
  activeAgentName: string;
  editingBuiltInMarch: boolean;
  agentName: string;
  agentDisplayName: string;
  agentDescription: string;
  agentAvatarColor: string;
  agentProviderIdString: string;
  agentModelId: string;
  agentSystemPrompt: string;
  resolvedAgentName: string;
  agentProviderOptions: Array<{ value: string; label: string }>;
  agentModelOptions: Array<{ value: string; label: string }>;
  formatAgentBinding: (providerId?: number | null, modelId?: string | null) => string;
  formatAgentSource: (source: string) => string;
}>();

const emit = defineEmits<{
  startCreateAgent: [];
  startEditAgent: [agent: ProviderSettingsView['agents'][number]];
  restoreMarchPrompt: [];
  deleteAgent: [name: string];
  'update:agentName': [value: string];
  'update:agentDisplayName': [value: string];
  'update:agentDescription': [value: string];
  'update:agentAvatarColor': [value: string];
  'update:agentProviderIdString': [value: string];
  'update:agentModelId': [value: string];
  'update:agentSystemPrompt': [value: string];
  submitAgent: [];
  resetAgentForm: [];
}>();

const agentName = computed({
  get: () => props.agentName,
  set: (value: string) => emit('update:agentName', value),
});
const agentDisplayName = computed({
  get: () => props.agentDisplayName,
  set: (value: string) => emit('update:agentDisplayName', value),
});
const agentDescription = computed({
  get: () => props.agentDescription,
  set: (value: string) => emit('update:agentDescription', value),
});
const agentAvatarColor = computed({
  get: () => props.agentAvatarColor,
  set: (value: string) => emit('update:agentAvatarColor', value),
});
const agentProviderIdString = computed({
  get: () => props.agentProviderIdString,
  set: (value: string) => emit('update:agentProviderIdString', value),
});
const agentModelId = computed({
  get: () => props.agentModelId,
  set: (value: string) => emit('update:agentModelId', value),
});
const agentSystemPrompt = computed({
  get: () => props.agentSystemPrompt,
  set: (value: string) => emit('update:agentSystemPrompt', value),
});
</script>
