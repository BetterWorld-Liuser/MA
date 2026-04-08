import type { TaskModelSelectorView } from '@/data/mock';

export type FlatModelItem = {
  modelConfigId: number;
  providerId: number;
  providerName: string;
  providerType: string;
  displayName: string;
  modelId: string;
};

export type CachedTaskModelSelector = {
  currentModelConfigId?: number | null;
  currentModel: string;
  currentTemperature?: number | null;
  currentTopP?: number | null;
  currentPresencePenalty?: number | null;
  currentFrequencyPenalty?: number | null;
  currentMaxOutputTokens?: number | null;
  currentModelDefaultMaxOutputTokens?: number | null;
  models: FlatModelItem[];
};

export const taskModelSelectorCache = new Map<number, CachedTaskModelSelector>();

export function buildCachedTaskModelSelector(response: TaskModelSelectorView): CachedTaskModelSelector {
  return {
    currentModelConfigId: response.currentModelConfigId ?? null,
    currentModel: response.currentModel,
    currentTemperature: response.currentTemperature ?? null,
    currentTopP: response.currentTopP ?? null,
    currentPresencePenalty: response.currentPresencePenalty ?? null,
    currentFrequencyPenalty: response.currentFrequencyPenalty ?? null,
    currentMaxOutputTokens: response.currentMaxOutputTokens ?? null,
    currentModelDefaultMaxOutputTokens: response.currentModelCapabilities.maxOutputTokens ?? null,
    models: response.models.map((model) => ({
      modelConfigId: model.modelConfigId,
      providerId: model.providerId,
      providerName: model.providerName,
      providerType: model.providerType,
      displayName: model.displayName,
      modelId: model.modelId,
    })),
  };
}

export function providerTypeLabel(providerType: string) {
  const labels: Record<string, string> = {
    anthropic: 'Anthropic',
    openai: 'OpenAI',
    gemini: 'Gemini',
    openai_compat: 'OpenAI 兼容',
    ollama: 'Ollama',
    env: '环境',
  };
  return labels[providerType] ?? providerType;
}

export function normalizePath(path?: string) {
  if (!path) {
    return '';
  }

  const normalized = path.replaceAll('\\', '/');
  if (normalized.startsWith('//?/UNC/')) {
    return `//${normalized.slice('//?/UNC/'.length)}`;
  }
  if (normalized.startsWith('//?/')) {
    return normalized.slice('//?/'.length);
  }
  return normalized;
}

export function parseOptionalNumber(value: string | number | null | undefined) {
  if (value == null) {
    return null;
  }
  const normalized = typeof value === 'string' ? value.trim() : String(value);
  if (!normalized) {
    return null;
  }
  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

export function parseOptionalInteger(value: string | number | null | undefined) {
  if (value == null) {
    return null;
  }
  const normalized = typeof value === 'string' ? value.trim() : String(value);
  if (!normalized) {
    return null;
  }
  const parsed = Number(normalized);
  return Number.isInteger(parsed) ? parsed : null;
}
