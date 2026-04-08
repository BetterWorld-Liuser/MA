import { computed, ref, type Ref } from 'vue';
import { parseOptionalInteger, parseOptionalNumber } from './taskModelSelectorShared';

type UseModelSettingsDraftOptions = {
  resolvedCurrentTemperature: Ref<number | null>;
  resolvedCurrentTopP: Ref<number | null>;
  resolvedCurrentPresencePenalty: Ref<number | null>;
  resolvedCurrentFrequencyPenalty: Ref<number | null>;
  resolvedCurrentMaxOutputTokens: Ref<number | null>;
  resolvedModelDefaultMaxOutputTokens: Ref<number | null>;
  emitSetModelSettings: (settings: {
    temperature?: number | null;
    topP?: number | null;
    presencePenalty?: number | null;
    frequencyPenalty?: number | null;
    maxOutputTokens?: number | null;
  }) => void;
  closeModelSettingsMenu: () => void;
};

export function useModelSettingsDraft({
  resolvedCurrentTemperature,
  resolvedCurrentTopP,
  resolvedCurrentPresencePenalty,
  resolvedCurrentFrequencyPenalty,
  resolvedCurrentMaxOutputTokens,
  resolvedModelDefaultMaxOutputTokens,
  emitSetModelSettings,
  closeModelSettingsMenu,
}: UseModelSettingsDraftOptions) {
  const temperatureDraft = ref('');
  const topPDraft = ref('');
  const presencePenaltyDraft = ref('');
  const frequencyPenaltyDraft = ref('');
  const maxOutputTokensDraft = ref('');
  const modelSettingsError = ref('');

  const maxOutputTokensPlaceholder = computed(() =>
    resolvedModelDefaultMaxOutputTokens.value
      ? `留空则使用默认值 ${resolvedModelDefaultMaxOutputTokens.value}`
      : '留空则使用模型默认值',
  );

  function resetModelSettingsDraft() {
    temperatureDraft.value = resolvedCurrentTemperature.value == null ? '' : String(resolvedCurrentTemperature.value);
    topPDraft.value = resolvedCurrentTopP.value == null ? '' : String(resolvedCurrentTopP.value);
    presencePenaltyDraft.value = resolvedCurrentPresencePenalty.value == null ? '' : String(resolvedCurrentPresencePenalty.value);
    frequencyPenaltyDraft.value = resolvedCurrentFrequencyPenalty.value == null ? '' : String(resolvedCurrentFrequencyPenalty.value);
    maxOutputTokensDraft.value = resolvedCurrentMaxOutputTokens.value == null ? '' : String(resolvedCurrentMaxOutputTokens.value);
    modelSettingsError.value = '';
  }

  function applyModelSettings() {
    const parsedTemperature = parseOptionalNumber(temperatureDraft.value);
    const parsedTopP = parseOptionalNumber(topPDraft.value);
    const parsedPresencePenalty = parseOptionalNumber(presencePenaltyDraft.value);
    const parsedFrequencyPenalty = parseOptionalNumber(frequencyPenaltyDraft.value);
    const parsedMaxOutputTokens = parseOptionalInteger(maxOutputTokensDraft.value);

    if (parsedTemperature !== null && (parsedTemperature < 0 || parsedTemperature > 2)) {
      modelSettingsError.value = 'Temperature 需要在 0 到 2 之间。';
      return;
    }

    if (parsedMaxOutputTokens !== null && parsedMaxOutputTokens < 1) {
      modelSettingsError.value = 'Max output tokens 需要大于 0。';
      return;
    }

    if (parsedTopP !== null && (parsedTopP < 0 || parsedTopP > 1)) {
      modelSettingsError.value = 'Top P 需要在 0 到 1 之间。';
      return;
    }

    if (parsedPresencePenalty !== null && (parsedPresencePenalty < -2 || parsedPresencePenalty > 2)) {
      modelSettingsError.value = 'Presence penalty 需要在 -2 到 2 之间。';
      return;
    }

    if (parsedFrequencyPenalty !== null && (parsedFrequencyPenalty < -2 || parsedFrequencyPenalty > 2)) {
      modelSettingsError.value = 'Frequency penalty 需要在 -2 到 2 之间。';
      return;
    }

    modelSettingsError.value = '';
    emitSetModelSettings({
      temperature: parsedTemperature,
      topP: parsedTopP,
      presencePenalty: parsedPresencePenalty,
      frequencyPenalty: parsedFrequencyPenalty,
      maxOutputTokens: parsedMaxOutputTokens,
    });
    closeModelSettingsMenu();
  }

  return {
    temperatureDraft,
    topPDraft,
    presencePenaltyDraft,
    frequencyPenaltyDraft,
    maxOutputTokensDraft,
    modelSettingsError,
    maxOutputTokensPlaceholder,
    resetModelSettingsDraft,
    applyModelSettings,
  };
}
