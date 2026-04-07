<template>
  <div class="composer-menu-header">
    <span>模型参数</span>
    <span class="composer-menu-status">{{ effectiveSelectedModel || '当前任务' }}</span>
  </div>
  <div class="space-y-3 px-3 py-2">
    <div class="dialog-field">
      <label class="dialog-label" for="composer-temperature">Temperature</label>
      <input
        id="composer-temperature"
        :value="temperatureDraft"
        class="app-input"
        type="number"
        min="0"
        max="2"
        step="0.1"
        placeholder="留空则使用默认值 0.2"
        @input="emit('update:temperatureDraft', ($event.target as HTMLInputElement).value)"
      />
      <p class="dialog-hint">控制输出发散度。`0` 更稳定，`2` 更开放。</p>
    </div>
    <div class="dialog-field">
      <label class="dialog-label" for="composer-max-output">Max Output Tokens</label>
      <input
        id="composer-max-output"
        :value="maxOutputTokensDraft"
        class="app-input"
        type="number"
        min="1"
        step="1"
        :placeholder="maxOutputTokensPlaceholder"
        @input="emit('update:maxOutputTokensDraft', ($event.target as HTMLInputElement).value)"
      />
      <p class="dialog-hint">留空时跟随模型默认上限与 provider 默认行为。</p>
    </div>
    <div class="space-y-3">
      <div class="dialog-field">
        <label class="dialog-label" for="composer-top-p">Top P</label>
        <input
          id="composer-top-p"
          :value="topPDraft"
          class="app-input"
          type="number"
          min="0"
          max="1"
          step="0.05"
          placeholder="留空则使用 provider 默认值"
          @input="emit('update:topPDraft', ($event.target as HTMLInputElement).value)"
        />
        <p class="dialog-hint">核采样范围。</p>
      </div>
      <div class="dialog-field">
        <label class="dialog-label" for="composer-presence-penalty">Presence Penalty</label>
        <input
          id="composer-presence-penalty"
          :value="presencePenaltyDraft"
          class="app-input"
          type="number"
          min="-2"
          max="2"
          step="0.1"
          placeholder="默认 0"
          @input="emit('update:presencePenaltyDraft', ($event.target as HTMLInputElement).value)"
        />
        <p class="dialog-hint">提高新话题倾向。</p>
      </div>
      <div class="dialog-field">
        <label class="dialog-label" for="composer-frequency-penalty">Frequency Penalty</label>
        <input
          id="composer-frequency-penalty"
          :value="frequencyPenaltyDraft"
          class="app-input"
          type="number"
          min="-2"
          max="2"
          step="0.1"
          placeholder="默认 0"
          @input="emit('update:frequencyPenaltyDraft', ($event.target as HTMLInputElement).value)"
        />
        <p class="dialog-hint">降低重复倾向。</p>
      </div>
    </div>
    <p v-if="modelSettingsError" class="text-[11px] text-error">{{ modelSettingsError }}</p>
  </div>
  <div class="composer-model-settings-footer">
    <div class="composer-model-settings-actions">
      <button class="app-button app-button-secondary composer-model-settings-button" type="button" @click="emit('reset')">
        重置
      </button>
      <button class="app-button app-button-primary composer-model-settings-button" type="button" @click="emit('apply')">
        应用
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  effectiveSelectedModel: string;
  temperatureDraft: string;
  topPDraft: string;
  presencePenaltyDraft: string;
  frequencyPenaltyDraft: string;
  maxOutputTokensDraft: string;
  maxOutputTokensPlaceholder: string;
  modelSettingsError: string;
}>();

const emit = defineEmits<{
  'update:temperatureDraft': [value: string];
  'update:topPDraft': [value: string];
  'update:presencePenaltyDraft': [value: string];
  'update:frequencyPenaltyDraft': [value: string];
  'update:maxOutputTokensDraft': [value: string];
  reset: [];
  apply: [];
}>();
</script>
