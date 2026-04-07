<template>
  <div class="space-y-5">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">主题</h3>
          <p class="settings-section-copy">目前提供深色与浅色两套主题。切换后立即作用于整套应用壳层与面板组件。</p>
        </div>
      </div>

      <div class="grid gap-4 lg:grid-cols-2">
        <button
          v-for="option in themeOptions"
          :key="option.value"
          type="button"
          class="theme-card"
          :class="theme === option.value ? 'theme-card-active' : ''"
          @click="emit('updateTheme', option.value)"
        >
          <div class="flex items-start justify-between gap-4">
            <div>
              <div class="flex items-center gap-2">
                <Icon :icon="option.icon" class="h-4 w-4 text-accent" />
                <h4 class="text-[15px] font-medium text-text">{{ option.label }}</h4>
              </div>
              <p class="mt-2 text-[12px] leading-5 text-text-muted">{{ option.description }}</p>
            </div>
            <span class="theme-card-check">
              <Icon v-if="theme === option.value" :icon="checkIcon" class="h-3.5 w-3.5" />
            </span>
          </div>

          <div class="theme-preview" :data-preview-theme="option.value">
            <div class="theme-preview-titlebar">
              <span class="theme-preview-logo">M</span>
              <span class="text-[10px] font-medium">March</span>
            </div>
            <div class="theme-preview-body">
              <div class="theme-preview-sidebar">
                <span class="theme-preview-chip theme-preview-chip-active"></span>
                <span class="theme-preview-chip"></span>
                <span class="theme-preview-chip"></span>
              </div>
              <div class="theme-preview-main">
                <div class="theme-preview-message"></div>
                <div class="theme-preview-message theme-preview-message-secondary"></div>
                <div class="theme-preview-input"></div>
              </div>
            </div>
          </div>
        </button>
      </div>
    </section>

    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">外观说明</h3>
          <p class="settings-section-copy">主题切换只影响 UI 呈现，不会触发任务、上下文或 provider 的运行时变更。</p>
        </div>
      </div>

      <div class="grid gap-3 md:grid-cols-3">
        <article class="settings-info-card">
          <p class="settings-info-label">持久化</p>
          <p class="settings-info-value">保存在当前设备本地</p>
        </article>
        <article class="settings-info-card">
          <p class="settings-info-label">生效方式</p>
          <p class="settings-info-value">即时切换，无需重启</p>
        </article>
        <article class="settings-info-card">
          <p class="settings-info-label">默认主题</p>
          <p class="settings-info-value">深色，保持当前视觉延续</p>
        </article>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue';
import checkIcon from '@iconify-icons/lucide/check';
import type { ThemeMode } from '@/composables/useAppearanceSettings';

defineProps<{
  theme: ThemeMode;
  themeOptions: Array<{
    value: ThemeMode;
    label: string;
    description: string;
    icon: string | object;
  }>;
}>();

const emit = defineEmits<{
  updateTheme: [theme: ThemeMode];
}>();
</script>
