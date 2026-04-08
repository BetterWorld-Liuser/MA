export const providerTypeOptions = [
  { value: 'openai_compat', label: 'OpenAI-compatible' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'gemini', label: 'Gemini' },
  { value: 'fireworks', label: 'Fireworks' },
  { value: 'together', label: 'Together' },
  { value: 'groq', label: 'Groq' },
  { value: 'mimo', label: 'Mimo' },
  { value: 'nebius', label: 'Nebius' },
  { value: 'xai', label: 'xAI' },
  { value: 'deepseek', label: 'DeepSeek' },
  { value: 'zai', label: 'ZAI' },
  { value: 'bigmodel', label: 'BigModel' },
  { value: 'cohere', label: 'Cohere' },
  { value: 'ollama', label: 'Ollama' },
] as const;

export const serverToolDefinitions = [
  {
    capability: 'web_search',
    label: 'Web Search',
    formats: ['anthropic', 'openai_responses', 'openai_chat_completions', 'gemini'],
  },
  {
    capability: 'code_execution',
    label: 'Code Execution',
    formats: ['anthropic', 'openai_responses', 'openai_chat_completions', 'gemini'],
  },
  { capability: 'file_search', label: 'File Search', formats: ['openai_responses'] },
] as const;

export const serverToolFormatLabels: Record<string, string> = {
  anthropic: 'Anthropic',
  openai_responses: 'OpenAI / Responses',
  openai_chat_completions: 'OpenAI-compatible / Chat Completions',
  gemini: 'Gemini',
};

export function providerTypeLabel(providerTypeValue: string) {
  return providerTypeOptions.find((option) => option.value === providerTypeValue)?.label ?? providerTypeValue;
}
