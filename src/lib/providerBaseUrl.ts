const providerBaseUrlDefaults: Record<string, string> = {
  openai_compat: 'https://api.openai.com/v1',
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com/v1',
  gemini: 'https://generativelanguage.googleapis.com/v1beta',
  fireworks: 'https://api.fireworks.ai/inference/v1',
  together: 'https://api.together.xyz/v1',
  groq: 'https://api.groq.com/openai/v1',
  mimo: 'https://api.mimo.org/v1',
  nebius: 'https://api.studio.nebius.com/v1',
  xai: 'https://api.x.ai/v1',
  deepseek: 'https://api.deepseek.com/v1',
  zai: 'https://api.z.ai/api/paas/v4',
  bigmodel: 'https://open.bigmodel.cn/api/paas/v4',
  cohere: 'https://api.cohere.com/v2',
  ollama: 'http://localhost:11434/v1',
};

const providerRequestPathPreview: Record<string, string> = {
  openai: '/responses',
  anthropic: '/messages',
  gemini: '/models/{model}:generateContent',
};

export function defaultProviderBaseUrl(providerType: string) {
  return providerBaseUrlDefaults[providerType] ?? 'https://api.example.com/v1';
}

export function normalizeProviderBaseUrl(providerType: string, rawValue: string) {
  const trimmed = rawValue.trim().replace(/\/+$/, '');
  if (!trimmed) {
    return '';
  }

  const defaultBaseUrl = providerBaseUrlDefaults[providerType];
  if (!defaultBaseUrl) {
    return trimmed;
  }

  try {
    const parsedDefault = new URL(defaultBaseUrl);
    const defaultPath = parsedDefault.pathname.replace(/\/+$/, '');
    if (!defaultPath || defaultPath === '/') {
      return trimmed;
    }

    const parsedInput = new URL(trimmed);
    const inputPath = parsedInput.pathname.replace(/\/+$/, '');
    if (!inputPath || inputPath === '/') {
      parsedInput.pathname = defaultPath;
      return parsedInput.toString().replace(/\/+$/, '');
    }
  } catch {
    return trimmed;
  }

  return trimmed;
}

export function resolveProviderRequestPreview(providerType: string, rawValue: string) {
  const normalizedBaseUrl = normalizeProviderBaseUrl(providerType, rawValue);
  if (!normalizedBaseUrl) {
    return '';
  }

  const requestPath = providerRequestPathPreview[providerType] ?? '/chat/completions';
  return `${normalizedBaseUrl}${requestPath}`;
}
