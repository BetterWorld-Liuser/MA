import { computed, ref, type Ref } from 'vue';
import type { ProviderSettingsView } from '@/data/mock';

type ReadonlyRef<T> = Readonly<Ref<T>>;
type AgentItem = ProviderSettingsView['agents'][number];

type SaveAgentInput = {
  name: string;
  displayName: string;
  description: string;
  systemPrompt: string;
  avatarColor?: string;
  providerId?: number | null;
  modelId?: string | null;
  useCustomMarchPrompt?: boolean;
};

export function useSettingsAgentForm({
  settings,
  onSaveAgent,
}: {
  settings: ReadonlyRef<ProviderSettingsView | null>;
  onSaveAgent: (input: SaveAgentInput) => void;
}) {
  const activeAgentName = ref('');
  const agentName = ref('');
  const agentDisplayName = ref('');
  const agentDescription = ref('');
  const agentAvatarColor = ref('#64748B');
  const agentProviderIdString = ref('');
  const agentModelId = ref('');
  const agentSystemPrompt = ref('');

  const editingBuiltInMarch = computed(() => activeAgentName.value === 'march');

  const agentProviderOptions = computed(() => [
    { value: '', label: '跟随任务默认' },
    ...(settings.value?.providers ?? []).map((provider) => ({
      value: String(provider.id),
      label: provider.name,
    })),
  ]);

  const resolvedAgentName = computed(() => {
    if (editingBuiltInMarch.value) {
      return 'march';
    }
    const normalized = agentName.value.trim().toLowerCase().replaceAll(' ', '-');
    return normalized || '';
  });

  const selectedAgentProvider = computed(() => {
    const providerId = Number(agentProviderIdString.value);
    if (!Number.isFinite(providerId) || providerId <= 0) {
      return null;
    }
    return settings.value?.providers.find((provider) => provider.id === providerId) ?? null;
  });

  const agentModelOptions = computed(() => {
    const provider = selectedAgentProvider.value;
    if (!provider) {
      return [];
    }
    return [
      { value: '', label: '跟随任务默认' },
      ...provider.models.map((model) => ({
        value: model.modelId,
        label: model.displayName || model.modelId,
      })),
    ];
  });

  function applyAgentEditorState(agent?: AgentItem) {
    activeAgentName.value = agent?.name ?? '';
    agentName.value = agent?.name ?? '';
    agentDisplayName.value = agent?.displayName ?? '';
    agentDescription.value = agent?.description ?? '';
    agentAvatarColor.value = agent?.avatarColor || '#64748B';
    agentProviderIdString.value = agent?.providerId ? String(agent.providerId) : '';
    agentModelId.value = agent?.modelId ?? '';
    agentSystemPrompt.value = agent?.systemPrompt ?? '';
  }

  function startCreateAgent() {
    applyAgentEditorState();
  }

  function startEditAgent(agent: AgentItem) {
    applyAgentEditorState(agent);
  }

  function resetAgentForm() {
    if (activeAgentName.value) {
      const agent = settings.value?.agents.find((item) => item.name === activeAgentName.value);
      if (agent) {
        applyAgentEditorState(agent);
        return;
      }
    }
    startCreateAgent();
  }

  function submitAgent() {
    if (!resolvedAgentName.value) {
      return;
    }

    onSaveAgent({
      name: resolvedAgentName.value,
      displayName: agentDisplayName.value,
      description: editingBuiltInMarch.value ? '' : agentDescription.value,
      systemPrompt: agentSystemPrompt.value,
      avatarColor: agentAvatarColor.value,
      providerId: agentProviderIdString.value ? Number(agentProviderIdString.value) : null,
      modelId: agentModelId.value.trim() || null,
      useCustomMarchPrompt: editingBuiltInMarch.value ? true : undefined,
    });
  }

  function formatAgentBinding(providerId?: number | null, modelId?: string | null) {
    if (!providerId || !modelId) {
      return '模型：跟随任务默认';
    }
    const provider = settings.value?.providers.find((item) => item.id === providerId);
    return `模型：${provider?.name ?? providerId} / ${modelId}`;
  }

  function formatAgentSource(source: string) {
    if (source === 'project') {
      return '来源：项目';
    }
    if (source === 'built_in') {
      return '来源：内置';
    }
    return '来源：用户';
  }

  return {
    activeAgentName,
    agentName,
    agentDisplayName,
    agentDescription,
    agentAvatarColor,
    agentProviderIdString,
    agentModelId,
    agentSystemPrompt,
    editingBuiltInMarch,
    agentProviderOptions,
    resolvedAgentName,
    agentModelOptions,
    startCreateAgent,
    startEditAgent,
    resetAgentForm,
    submitAgent,
    formatAgentBinding,
    formatAgentSource,
  };
}
