<template>
  <section class="settings-shell">
    <header class="settings-header">
      <div class="flex items-start gap-3">
        <Button
          variant="ghost"
          size="icon"
          class="mt-0.5 rounded-xl border border-[color:var(--ma-line-soft)]"
          @click="emit('close')"
        >
          <Icon :icon="arrowLeftIcon" class="h-4 w-4" />
        </Button>
        <div>
          <p class="text-[11px] uppercase tracking-[0.18em] text-text-dim">Settings</p>
          <h2 class="mt-1 text-[22px] font-semibold tracking-[-0.02em] text-text">应用设置</h2>
          <p class="mt-2 max-w-[720px] text-[13px] leading-6 text-text-muted">
            外观和 provider 都放在这里。主题会立即生效并保存在本地，provider 配置仍然由用户目录下的设置库统一管理。
          </p>
        </div>
      </div>
    </header>

    <div class="settings-layout">
      <aside class="settings-sidebar">
        <div class="settings-sidebar-header">
          <p class="text-[10px] uppercase tracking-[0.18em] text-text-dim">Sections</p>
          <p class="mt-1 text-[12px] leading-5 text-text-muted">把全局外观和运行入口集中在一个固定位置。</p>
        </div>

        <nav class="space-y-2">
          <button
            v-for="section in sectionOptions"
            :key="section.value"
            type="button"
            class="settings-nav-item"
            :class="activeSection === section.value ? 'settings-nav-item-active' : ''"
            @click="activeSection = section.value"
          >
            <Icon :icon="section.icon" class="h-4 w-4 shrink-0" />
            <span class="min-w-0 flex-1 text-left">
              <span class="block truncate text-[13px] font-medium text-text">{{ section.label }}</span>
              <span class="mt-0.5 block truncate text-[11px] text-text-dim">{{ section.description }}</span>
            </span>
          </button>
        </nav>
      </aside>

      <div class="min-h-0 overflow-y-auto">
        <div v-if="activeSection === 'appearance'" class="space-y-5">
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

        <div v-else-if="activeSection === 'providers'" class="settings-grid">
          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">Providers</h3>
                <p class="settings-section-copy">全局配置，保存在用户目录下。</p>
              </div>
              <Button variant="outline" size="sm" @click="startCreate">新增 Provider</Button>
            </div>

            <div v-if="settings?.providers.length" class="space-y-3">
              <article
                v-for="provider in settings.providers"
                :key="provider.id"
                class="settings-provider-card"
                :class="provider.id === activeEditorId ? 'settings-provider-card-active' : ''"
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <h4 class="truncate text-[14px] font-medium text-text">{{ provider.name }}</h4>
                      <span v-if="provider.id === settings.defaultProviderId" class="settings-default-badge">默认</span>
                    </div>
                    <p class="mt-1 text-[11px] uppercase tracking-[0.12em] text-text-dim">{{ providerTypeLabel(provider.providerType) }}</p>
                    <p v-if="provider.baseUrl" class="mt-1 truncate font-mono text-[11px] text-text-dim">{{ provider.baseUrl }}</p>
                    <p class="mt-2 text-[12px] text-text-muted">Key: {{ provider.apiKeyHint }}</p>
                  </div>
                  <div class="flex shrink-0 items-center gap-1">
                    <Button variant="ghost" size="sm" @click="startEdit(provider)">编辑</Button>
                    <Button variant="ghost" size="sm" class="text-[#d44a4a] hover:text-[#d44a4a]" @click="emit('deleteProvider', provider.id)">
                      删除
                    </Button>
                  </div>
                </div>
              </article>
            </div>

            <div v-else class="settings-empty">
              还没有配置 provider。先新增一个 provider 类型和凭据，后面模型选择器就能接上它。
            </div>
          </section>

          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">{{ activeEditorId ? '编辑 Provider' : '新增 Provider' }}</h3>
                <p class="settings-section-copy">这里只负责维护单个 provider 的接入信息，不再承载全局默认模型配置。</p>
              </div>
            </div>

            <form class="space-y-4" @submit.prevent="submitProvider">
              <div class="dialog-field">
                <label class="dialog-label" for="provider-type">类型</label>
                <SettingsSelect v-model="providerType" :options="providerTypeOptions" placeholder="请选择 provider 类型" />
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="provider-name">名称</label>
                <Input id="provider-name" v-model="providerName" :placeholder="providerNamePlaceholder" />
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="provider-base-url">Base URL</label>
                <Input
                  id="provider-base-url"
                  v-model="providerBaseUrl"
                  :placeholder="baseUrlPlaceholder"
                />
                <p class="dialog-hint">
                  {{ baseUrlHint }}
                </p>
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="provider-api-key">API Key</label>
                <Input
                  id="provider-api-key"
                  v-model="providerApiKey"
                  type="password"
                  :placeholder="apiKeyPlaceholder"
                />
              </div>
              <div class="dialog-field">
                <div class="flex items-center justify-between gap-3">
                  <label class="dialog-label" for="provider-probe-model">Probe Model</label>
                  <Button
                    variant="ghost"
                    size="sm"
                    type="button"
                    :disabled="busy || probeModelsLoading"
                    @click="requestProbeModelsNow"
                  >
                    {{ probeModelsLoading ? '读取中…' : '刷新列表' }}
                  </Button>
                </div>
                <Input
                  id="provider-probe-model"
                  v-model="providerProbeModel"
                  :placeholder="probeModelPlaceholder"
                />
                <SettingsSelect
                  v-model="providerProbeModel"
                  class="mt-2"
                  :options="probeModelOptions"
                  :placeholder="probeModelSelectPlaceholder"
                  :disabled="busy || !probeModels.length"
                  searchable
                  search-placeholder="搜索 probe model…"
                />
                <div
                  v-if="!probeModels.length && probeModelsLoading"
                  class="mt-2 text-[11px] text-text-dim"
                >
                  正在读取供应商模型列表，读取完成后可直接从下拉中选择。
                </div>
                <div
                  v-else-if="!probeModels.length && probeSuggestedModels.length"
                  class="mt-2 flex flex-wrap gap-2"
                >
                  <button
                    v-for="model in probeSuggestedModels"
                    :key="model"
                    type="button"
                    class="rounded-full border border-[color:var(--ma-line-soft)] px-2.5 py-1 text-[11px] text-text-dim transition hover:bg-bg-hover hover:text-text"
                    @click="providerProbeModel = model"
                  >
                    {{ model }}
                  </button>
                </div>
                <p class="dialog-hint">
                  优先展示供应商 `/models` 返回的可搜索列表；若接口没返回数据，或你想测试一个未列出的模型，也可以继续手动填写。
                </p>
              </div>
              <div class="flex items-center justify-end gap-2">
                <Button variant="outline" type="button" :disabled="busy" @click="testProvider">
                  测试连通性
                </Button>
                <Button variant="ghost" type="button" @click="resetForm">清空</Button>
                <Button type="submit" :disabled="busy">{{ activeEditorId ? '保存修改' : '创建 Provider' }}</Button>
              </div>
              <p v-if="props.providerTestMessage" class="text-[12px]" :class="props.providerTestSuccess ? 'text-success' : 'text-error'">
                {{ props.providerTestMessage }}
              </p>
            </form>

            <div v-if="activeEditorProvider" class="mt-6 border-t border-[color:var(--ma-line-soft)] pt-5">
              <div class="settings-panel-header">
                <div>
                  <h3 class="settings-section-title">模型能力</h3>
                  <p class="settings-section-copy">这里维护该 provider 下的模型能力。OpenAI-compatible 依赖这份配置决定图片入口、工具能力与上下文预算；已知 provider 也可以在这里补充或覆盖新模型。</p>
                </div>
                <Button variant="outline" size="sm" type="button" @click="startCreateProviderModel">
                  添加模型
                </Button>
              </div>

              <div v-if="activeEditorProvider.models.length" class="space-y-3">
                <article
                  v-for="model in activeEditorProvider.models"
                  :key="model.id"
                  class="settings-provider-card"
                  :class="activeProviderModelId === model.id ? 'settings-provider-card-active' : ''"
                >
                  <div class="flex items-start justify-between gap-3">
                    <div class="min-w-0">
                      <div class="flex items-center gap-2">
                        <h4 class="truncate text-[14px] font-medium text-text">{{ model.displayName || model.modelId }}</h4>
                        <span class="rounded-full bg-bg-hover px-2 py-0.5 text-[10px] uppercase tracking-[0.12em] text-text-dim">{{ model.modelId }}</span>
                      </div>
                      <p class="mt-2 text-[12px] text-text-muted">
                        {{ formatCapabilitiesSummary(model.capabilities) }}
                      </p>
                    </div>
                    <div class="flex shrink-0 items-center gap-1">
                      <Button variant="ghost" size="sm" type="button" @click="startEditProviderModel(model)">编辑</Button>
                      <Button variant="ghost" size="sm" type="button" class="text-[#d44a4a] hover:text-[#d44a4a]" @click="emit('deleteProviderModel', model.id)">
                        删除
                      </Button>
                    </div>
                  </div>
                </article>
              </div>
              <div v-else class="settings-empty">
                这个 provider 还没有单独配置模型能力。
              </div>

              <form class="mt-4 space-y-4" @submit.prevent="submitProviderModel">
                <div class="grid gap-4 md:grid-cols-2">
                  <div class="dialog-field">
                    <label class="dialog-label" for="provider-model-id">模型 ID</label>
                    <template v-if="providerModelIdOptions.length">
                      <SettingsSelect
                        v-model="providerModelId"
                        :options="providerModelIdOptions"
                        placeholder="从已探测或已配置模型中选择"
                        searchable
                        search-placeholder="搜索模型 ID…"
                      />
                    </template>
                    <template v-else>
                      <Input id="provider-model-id" v-model="providerModelId" placeholder="gpt-4o-mini / qwen2.5-coder:32b" />
                    </template>
                    <Input
                      v-if="providerModelIdOptions.length"
                      v-model="providerModelId"
                      class="mt-2"
                      placeholder="也可以直接手填新的 model_id"
                    />
                  </div>
                  <div class="dialog-field">
                    <label class="dialog-label" for="provider-model-display-name">显示名称</label>
                    <Input id="provider-model-display-name" v-model="providerModelDisplayName" placeholder="可选，留空则界面显示 model_id" />
                  </div>
                  <div class="dialog-field">
                    <label class="dialog-label" for="provider-model-context-window">上下文窗口</label>
                    <Input id="provider-model-context-window" v-model="providerModelContextWindow" type="number" min="1" />
                  </div>
                  <div class="dialog-field">
                    <label class="dialog-label" for="provider-model-max-output">最大输出</label>
                    <Input id="provider-model-max-output" v-model="providerModelMaxOutputTokens" type="number" min="1" />
                  </div>
                </div>

                <div v-if="providerModelIdSuggestions.length" class="flex flex-wrap gap-2">
                  <button
                    v-for="model in providerModelIdSuggestions"
                    :key="model"
                    type="button"
                    class="rounded-full border border-[color:var(--ma-line-soft)] px-2.5 py-1 text-[11px] text-text-dim transition hover:bg-bg-hover hover:text-text"
                    @click="providerModelId = model"
                  >
                    {{ model }}
                  </button>
                </div>

                <div class="dialog-field">
                  <label class="dialog-label">能力</label>
                  <div class="grid gap-3 md:grid-cols-2">
                    <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                      <input v-model="providerModelSupportsToolUse" type="checkbox" />
                      <span>工具调用</span>
                    </label>
                    <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                      <input v-model="providerModelSupportsVision" type="checkbox" />
                      <span>图片输入</span>
                    </label>
                    <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                      <input v-model="providerModelSupportsAudio" type="checkbox" />
                      <span>音频输入</span>
                    </label>
                    <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                      <input v-model="providerModelSupportsPdf" type="checkbox" />
                      <span>PDF 输入</span>
                    </label>
                  </div>
                </div>

                <div class="dialog-field">
                  <label class="dialog-label">Server-side Tools</label>
                  <div class="space-y-3">
                    <div
                      v-for="tool in serverToolDefinitions"
                      :key="tool.capability"
                      class="grid gap-3 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-3 md:grid-cols-[minmax(0,1fr)_220px]"
                    >
                      <label class="flex items-center gap-2 text-[12px] text-text">
                        <input
                          :checked="isServerToolEnabled(tool.capability)"
                          type="checkbox"
                          @change="onServerToolToggle(tool.capability, $event)"
                        />
                        <span>{{ tool.label }}</span>
                      </label>
                      <SettingsSelect
                        :model-value="providerModelServerTools[tool.capability] ?? ''"
                        :options="serverToolFormatOptions(tool.capability)"
                        placeholder="选择格式"
                        :disabled="!isServerToolEnabled(tool.capability)"
                        @update:model-value="setServerToolFormat(tool.capability, $event)"
                      />
                    </div>
                  </div>
                  <p class="dialog-hint">
                    这些工具由 provider 侧执行，March 只负责保存能力配置并在后续请求翻译层中注入对应定义。
                  </p>
                </div>

                <div class="flex items-center justify-end gap-2">
                  <Button variant="ghost" type="button" @click="resetProviderModelForm">清空</Button>
                  <Button type="submit" :disabled="busy || !providerModelId.trim()">
                    {{ activeProviderModelId ? '保存模型能力' : '添加模型能力' }}
                  </Button>
                </div>
              </form>
            </div>
          </section>
        </div>

        <div v-else-if="activeSection === 'agents'" class="settings-grid">
          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">角色</h3>
                <p class="settings-section-copy">管理 March 和可复用的自定义角色。它们会出现在聊天里的 `@agent` 召唤链路中。</p>
              </div>
              <Button variant="outline" size="sm" @click="startCreateAgent">新增角色</Button>
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
                    <Button variant="ghost" size="sm" @click="startEditAgent(agent)">编辑</Button>
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
                      class="text-[#d44a4a] hover:text-[#d44a4a]"
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

            <form class="space-y-4" @submit.prevent="submitAgent">
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
                <Button variant="ghost" type="button" @click="resetAgentForm">清空</Button>
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

        <div v-else class="space-y-5">
          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">默认运行配置</h3>
                <p class="settings-section-copy">这是应用级默认值，用来决定新任务初始使用哪个 provider 与模型。</p>
              </div>
              <Button
                variant="outline"
                size="sm"
                :disabled="!defaultProviderIdLocal || modelsLoading"
                @click="requestModels"
              >
                {{ modelsLoading ? '刷新中…' : '刷新模型' }}
              </Button>
            </div>

            <div class="space-y-4">
              <div class="dialog-field">
                <label class="dialog-label" for="default-provider">默认 Provider</label>
                <SettingsSelect
                  v-model="defaultProviderIdString"
                  :options="providerOptions"
                  placeholder="请选择"
                />
                <p class="dialog-hint">这里选的是全局默认入口，只用于之后新建任务的初始 provider / model。</p>
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="default-model">默认模型</label>
                <template v-if="availableModels.length">
                  <SettingsSelect
                    v-model="defaultModelLocal"
                    :options="modelOptions"
                    placeholder="请选择模型"
                    searchable
                    search-placeholder="搜索模型…"
                  />
                </template>
                <template v-else>
                  <Input id="default-model" v-model="defaultModelLocal" placeholder="gpt-5.3-codex / qwen2.5-coder" />
                </template>
                <div v-if="!availableModels.length && suggestedModels.length" class="mt-2 flex flex-wrap gap-2">
                  <button
                    v-for="model in suggestedModels"
                    :key="model"
                    type="button"
                    class="rounded-full border border-[color:var(--ma-line-soft)] px-2.5 py-1 text-[11px] text-text-dim transition hover:bg-bg-hover hover:text-text"
                    @click="defaultModelLocal = model"
                  >
                    {{ model }}
                  </button>
                </div>
              </div>
              <div class="flex items-center justify-end">
                <Button :disabled="busy || !defaultProviderIdLocal || !defaultModelLocal.trim()" @click="submitDefaultProvider">
                  保存默认配置
                </Button>
              </div>
            </div>
          </section>

          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">说明</h3>
                <p class="settings-section-copy">默认运行配置是应用级入口，不与任何单个 Provider 绑定。</p>
              </div>
            </div>

            <div class="grid gap-3 md:grid-cols-3">
              <article class="settings-info-card">
                <p class="settings-info-label">作用范围</p>
                <p class="settings-info-value">只影响之后新建的任务；已有任务保持自己的 provider 与模型</p>
              </article>
              <article class="settings-info-card">
                <p class="settings-info-label">模型来源</p>
                <p class="settings-info-value">来自当前默认 Provider 的可读模型列表</p>
              </article>
              <article class="settings-info-card">
                <p class="settings-info-label">关系边界</p>
                <p class="settings-info-value">与 Provider 凭据编辑分离，避免混淆全局配置与接入配置</p>
              </article>
            </div>
          </section>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import arrowLeftIcon from '@iconify-icons/lucide/arrow-left';
import checkIcon from '@iconify-icons/lucide/check';
import moonIcon from '@iconify-icons/lucide/moon-star';
import slidersHorizontalIcon from '@iconify-icons/lucide/sliders-horizontal';
import serverIcon from '@iconify-icons/lucide/server-cog';
import sunIcon from '@iconify-icons/lucide/sun-medium';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import type { ThemeMode } from '@/composables/useAppearanceSettings';
import type { ProviderSettingsView } from '@/data/mock';
import SettingsSelect from './SettingsSelect.vue';

const props = defineProps<{
  theme: ThemeMode;
  settings: ProviderSettingsView | null;
  busy?: boolean;
  modelsLoading?: boolean;
  availableModels: string[];
  suggestedModels: string[];
  probeModels: string[];
  probeSuggestedModels: string[];
  probeModelsLoading?: boolean;
  providerTestMessage?: string;
  providerTestSuccess?: boolean;
}>();

const emit = defineEmits<{
  close: [];
  updateTheme: [theme: ThemeMode];
  saveProvider: [input: { id?: number; providerType: string; name: string; baseUrl: string; apiKey: string }];
  saveProviderModel: [input: {
    id?: number;
    providerId: number;
    modelId: string;
    displayName: string;
    contextWindow: number;
    maxOutputTokens: number;
    supportsToolUse: boolean;
    supportsVision: boolean;
    supportsAudio: boolean;
    supportsPdf: boolean;
    serverTools: Array<{
      capability: string;
      format: string;
    }>;
  }];
  testProvider: [input: { id?: number; providerType: string; name: string; baseUrl: string; apiKey: string; probeModel?: string }];
  deleteProvider: [providerId: number];
  deleteProviderModel: [providerModelId: number];
  saveAgent: [input: {
    name: string;
    displayName: string;
    description: string;
    systemPrompt: string;
    avatarColor?: string;
    providerId?: number | null;
    modelId?: string | null;
    useCustomMarchPrompt?: boolean;
  }];
  deleteAgent: [name: string];
  restoreMarchPrompt: [];
  saveDefaultProvider: [input: { providerId: number; model: string }];
  requestModels: [providerId: number];
  requestProbeModels: [input: { id?: number; providerType: string; baseUrl: string; apiKey: string; probeModel?: string }];
}>();

const activeSection = ref<'appearance' | 'providers' | 'agents' | 'defaults'>('appearance');
const activeEditorId = ref<number | null>(null);
const providerType = ref('openai_compat');
const providerName = ref('');
const providerBaseUrl = ref('');
const providerApiKey = ref('');
const providerProbeModel = ref('');
const activeProviderModelId = ref<number | null>(null);
const providerModelId = ref('');
const providerModelDisplayName = ref('');
const providerModelContextWindow = ref('131072');
const providerModelMaxOutputTokens = ref('4096');
const providerModelSupportsToolUse = ref(false);
const providerModelSupportsVision = ref(false);
const providerModelSupportsAudio = ref(false);
const providerModelSupportsPdf = ref(false);
const providerModelServerTools = ref<Record<string, string>>({});
const defaultProviderIdString = ref('');
const defaultModelLocal = ref('');
const activeAgentName = ref('');
const agentName = ref('');
const agentDisplayName = ref('');
const agentDescription = ref('');
const agentAvatarColor = ref('#64748B');
const agentProviderIdString = ref('');
const agentModelId = ref('');
const agentSystemPrompt = ref('');

const sectionOptions = [
  {
    value: 'appearance' as const,
    label: '外观',
    description: '主题与整体观感',
    icon: sunIcon,
  },
  {
    value: 'providers' as const,
    label: 'Providers',
    description: '模型入口与凭据',
    icon: serverIcon,
  },
  {
    value: 'agents' as const,
    label: '角色',
    description: 'March 与自定义 agent',
    icon: serverIcon,
  },
  {
    value: 'defaults' as const,
    label: '默认运行',
    description: '默认 provider 与模型',
    icon: slidersHorizontalIcon,
  },
];

const themeOptions = [
  {
    value: 'dark' as const,
    label: '深色主题',
    description: '保持 March 当前的低照度桌面感，适合长时间编码和夜间使用。',
    icon: moonIcon,
  },
  {
    value: 'light' as const,
    label: '浅色主题',
    description: '提供更轻盈的阅读层次，适合白天环境和文档密集型工作流。',
    icon: sunIcon,
  },
];

const defaultProviderIdLocal = computed(() => {
  const parsed = Number(defaultProviderIdString.value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
});

const activeEditorProvider = computed(() =>
  props.settings?.providers.find((provider) => provider.id === activeEditorId.value) ?? null,
);

const editingBuiltInMarch = computed(() => activeAgentName.value === 'march');

const providerOptions = computed(() =>
  (props.settings?.providers ?? []).map((provider) => ({
    value: String(provider.id),
    label: `${provider.name} · ${providerTypeLabel(provider.providerType)}`,
  })),
);

const providerTypeOptions = [
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
];

const serverToolDefinitions = [
  { capability: 'web_search', label: 'Web Search', formats: ['anthropic', 'openai', 'gemini'] },
  { capability: 'code_execution', label: 'Code Execution', formats: ['anthropic', 'openai', 'gemini'] },
  { capability: 'file_search', label: 'File Search', formats: ['openai'] },
] as const;

const serverToolFormatLabels: Record<string, string> = {
  anthropic: 'Anthropic',
  openai: 'OpenAI',
  gemini: 'Gemini',
};

const agentProviderOptions = computed(() => [
  { value: '', label: '跟随任务默认' },
  ...(props.settings?.providers ?? []).map((provider) => ({
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
  return props.settings?.providers.find((provider) => provider.id === providerId) ?? null;
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

const modelOptions = computed(() =>
  props.availableModels.map((model) => ({
    value: model,
    label: model,
  })),
);

const probeModelOptions = computed(() =>
  props.probeModels.map((model) => ({
    value: model,
    label: model,
  })),
);

const probeModelSelectPlaceholder = computed(() => {
  if (props.probeModelsLoading) {
    return '正在读取供应商模型列表…';
  }
  if (props.probeModels.length) {
    return '从供应商模型列表中选择';
  }
  return '暂无可选列表，仍可先手动填写';
});

const providerModelIdOptions = computed(() => {
  const configured = activeEditorProvider.value?.models.map((model) => model.modelId) ?? [];
  const merged = Array.from(new Set([...props.probeModels, ...configured]))
    .map((model) => model.trim())
    .filter(Boolean);

  return merged.map((model) => ({
    value: model,
    label: model,
  }));
});

const providerModelIdSuggestions = computed(() =>
  Array.from(
    new Set([
      ...props.probeSuggestedModels,
      ...props.probeModels.slice(0, 8),
      ...(activeEditorProvider.value?.models.map((model) => model.modelId) ?? []),
    ]),
  )
    .map((model) => model.trim())
    .filter(Boolean)
    .slice(0, 10),
);

const providerNamePlaceholder = computed(() => {
  if (providerType.value === 'openai_compat') {
    return 'OpenRouter / Local vLLM';
  }
  return providerTypeLabel(providerType.value);
});

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

const baseUrlPlaceholder = computed(
  () => providerBaseUrlDefaults[providerType.value] ?? 'https://api.example.com/v1',
);

const baseUrlHint = computed(() => {
  if (providerType.value === 'openai_compat') {
    return '这个类型通常需要显式填写自定义端点，例如 OpenRouter、硅基流动或自建网关。';
  }

  return '可选。留空时使用该 provider 的默认官方端点；填写后会改走你指定的兼容入口。';
});

const apiKeyPlaceholder = computed(() => {
  if (providerType.value === 'ollama') {
    return activeEditorId.value ? '留空即可，当前类型默认不需要 API key' : '可留空';
  }
  return activeEditorId.value ? '留空则保持当前 API key' : 'sk-...';
});

const probeModelPlaceholder = computed(() => {
  if (providerType.value === 'openai_compat') {
    return '例如 gpt-4o-mini / kimi-k2 / qwen2.5-coder';
  }
  return '留空则使用内置建议模型';
});

watch(
  () => props.settings,
  (settings) => {
    defaultProviderIdString.value = settings?.defaultProviderId ? String(settings.defaultProviderId) : '';
    defaultModelLocal.value = settings?.defaultModel ?? '';
  },
  { immediate: true },
);

watch(defaultProviderIdLocal, (providerId, previous) => {
  if (!providerId || providerId === previous) {
    return;
  }
  emit('requestModels', providerId);
});

watch(
  [activeSection, activeEditorId, providerType, providerBaseUrl, providerApiKey, providerProbeModel],
  () => {
    if (activeSection.value !== 'providers') {
      return;
    }
    scheduleProbeModelsRequest();
  },
);

let probeModelRequestTimer: ReturnType<typeof window.setTimeout> | null = null;

onUnmounted(() => {
  if (probeModelRequestTimer) {
    window.clearTimeout(probeModelRequestTimer);
  }
});

function startCreate() {
  activeSection.value = 'providers';
  activeEditorId.value = null;
  providerType.value = 'openai_compat';
  providerName.value = '';
  providerBaseUrl.value = '';
  providerApiKey.value = '';
  providerProbeModel.value = '';
  resetProviderModelForm();
}

function startCreateAgent() {
  activeSection.value = 'agents';
  activeAgentName.value = '';
  agentName.value = '';
  agentDisplayName.value = '';
  agentDescription.value = '';
  agentAvatarColor.value = '#64748B';
  agentProviderIdString.value = '';
  agentModelId.value = '';
  agentSystemPrompt.value = '';
}

function startEditAgent(agent: ProviderSettingsView['agents'][number]) {
  activeSection.value = 'agents';
  activeAgentName.value = agent.name;
  agentName.value = agent.name;
  agentDisplayName.value = agent.displayName;
  agentDescription.value = agent.description;
  agentAvatarColor.value = agent.avatarColor || '#64748B';
  agentProviderIdString.value = agent.providerId ? String(agent.providerId) : '';
  agentModelId.value = agent.modelId ?? '';
  agentSystemPrompt.value = agent.systemPrompt;
}

function startEdit(provider: ProviderSettingsView['providers'][number]) {
  activeSection.value = 'providers';
  activeEditorId.value = provider.id;
  providerType.value = provider.providerType;
  providerName.value = provider.name;
  providerBaseUrl.value = provider.baseUrl ?? '';
  providerApiKey.value = '';
  providerProbeModel.value = '';
  resetProviderModelForm();
}

function resetForm() {
  if (activeEditorId.value) {
    const provider = props.settings?.providers.find((item) => item.id === activeEditorId.value);
    if (provider) {
      startEdit(provider);
      return;
    }
  }
  startCreate();
}

function resetAgentForm() {
  if (activeAgentName.value) {
    const agent = props.settings?.agents.find((item) => item.name === activeAgentName.value);
    if (agent) {
      startEditAgent(agent);
      return;
    }
  }
  startCreateAgent();
}

function submitProvider() {
  emit('saveProvider', {
    id: activeEditorId.value ?? undefined,
    providerType: providerType.value,
    name: providerName.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
  });
}

function submitAgent() {
  if (!resolvedAgentName.value) {
    return;
  }

  emit('saveAgent', {
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

function startCreateProviderModel() {
  activeProviderModelId.value = null;
  providerModelId.value = '';
  providerModelDisplayName.value = '';
  providerModelContextWindow.value = '131072';
  providerModelMaxOutputTokens.value = '4096';
  providerModelSupportsToolUse.value = false;
  providerModelSupportsVision.value = false;
  providerModelSupportsAudio.value = false;
  providerModelSupportsPdf.value = false;
  providerModelServerTools.value = {};
}

function startEditProviderModel(model: NonNullable<typeof activeEditorProvider.value>['models'][number]) {
  activeProviderModelId.value = model.id;
  providerModelId.value = model.modelId;
  providerModelDisplayName.value = model.displayName ?? '';
  providerModelContextWindow.value = String(model.capabilities.contextWindow);
  providerModelMaxOutputTokens.value = String(model.capabilities.maxOutputTokens);
  providerModelSupportsToolUse.value = model.capabilities.supportsToolUse;
  providerModelSupportsVision.value = model.capabilities.supportsVision;
  providerModelSupportsAudio.value = model.capabilities.supportsAudio;
  providerModelSupportsPdf.value = model.capabilities.supportsPdf;
  providerModelServerTools.value = Object.fromEntries(
    model.capabilities.serverTools.map((tool) => [tool.capability, tool.format]),
  );
}

function resetProviderModelForm() {
  startCreateProviderModel();
}

function submitProviderModel() {
  if (!activeEditorProvider.value) {
    return;
  }

  const serverTools = serverToolDefinitions
    .map((tool) => ({
      capability: tool.capability,
      format: providerModelServerTools.value[tool.capability]?.trim() ?? '',
    }))
    .filter((tool) => tool.format);

  emit('saveProviderModel', {
    id: activeProviderModelId.value ?? undefined,
    providerId: activeEditorProvider.value.id,
    modelId: providerModelId.value,
    displayName: providerModelDisplayName.value,
    contextWindow: Math.max(1, Number(providerModelContextWindow.value) || 131072),
    maxOutputTokens: Math.max(1, Number(providerModelMaxOutputTokens.value) || 4096),
    supportsToolUse: providerModelSupportsToolUse.value || serverTools.length > 0,
    supportsVision: providerModelSupportsVision.value,
    supportsAudio: providerModelSupportsAudio.value,
    supportsPdf: providerModelSupportsPdf.value,
    serverTools,
  });
  resetProviderModelForm();
}

function testProvider() {
  emit('testProvider', {
    id: activeEditorId.value ?? undefined,
    providerType: providerType.value,
    name: providerName.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
    probeModel: providerProbeModel.value,
  });
}

function requestModels() {
  if (!defaultProviderIdLocal.value) {
    return;
  }
  emit('requestModels', defaultProviderIdLocal.value);
}

function submitDefaultProvider() {
  if (!defaultProviderIdLocal.value) {
    return;
  }
  emit('saveDefaultProvider', {
    providerId: defaultProviderIdLocal.value,
    model: defaultModelLocal.value,
  });
}

function providerTypeLabel(providerTypeValue: string) {
  return providerTypeOptions.find((option) => option.value === providerTypeValue)?.label ?? providerTypeValue;
}

function formatAgentBinding(providerId?: number | null, modelId?: string | null) {
  if (!providerId || !modelId) {
    return '模型：跟随任务默认';
  }
  const provider = props.settings?.providers.find((item) => item.id === providerId);
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

function requestProbeModelsNow() {
  if (probeModelRequestTimer) {
    window.clearTimeout(probeModelRequestTimer);
    probeModelRequestTimer = null;
  }
  emit('requestProbeModels', {
    id: activeEditorId.value ?? undefined,
    providerType: providerType.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
    probeModel: providerProbeModel.value,
  });
}

function scheduleProbeModelsRequest() {
  if (probeModelRequestTimer) {
    window.clearTimeout(probeModelRequestTimer);
  }
  probeModelRequestTimer = window.setTimeout(() => {
    requestProbeModelsNow();
  }, 350);
}

function serverToolFormatOptions(capability: string) {
  const definition = serverToolDefinitions.find((tool) => tool.capability === capability);
  return (definition?.formats ?? []).map((format) => ({
    value: format,
    label: serverToolFormatOptionLabel(capability, format),
  }));
}

function serverToolFormatOptionLabel(capability: string, format: string) {
  const providerLabel = serverToolFormatLabels[format] ?? format;
  if (capability === 'web_search' && format === 'openai') {
    return `${providerLabel} (web_search_preview)`;
  }
  if (capability === 'web_search' && format === 'anthropic') {
    return `${providerLabel} (web_search_20250305)`;
  }
  if (capability === 'web_search' && format === 'gemini') {
    return `${providerLabel} (google_search)`;
  }
  if (capability === 'code_execution' && format === 'openai') {
    return `${providerLabel} (code_interpreter)`;
  }
  if (capability === 'code_execution' && format === 'anthropic') {
    return `${providerLabel} (code_execution_20250522)`;
  }
  if (capability === 'code_execution' && format === 'gemini') {
    return `${providerLabel} (code_execution)`;
  }
  if (capability === 'file_search' && format === 'openai') {
    return `${providerLabel} (file_search)`;
  }
  return providerLabel;
}

function isServerToolEnabled(capability: string) {
  return Boolean(providerModelServerTools.value[capability]);
}

function toggleServerTool(capability: string, enabled: boolean) {
  if (enabled) {
    const [firstFormat] = serverToolFormatOptions(capability);
    if (firstFormat) {
      providerModelServerTools.value = {
        ...providerModelServerTools.value,
        [capability]: providerModelServerTools.value[capability] || firstFormat.value,
      };
      providerModelSupportsToolUse.value = true;
    }
    return;
  }

  const next = { ...providerModelServerTools.value };
  delete next[capability];
  providerModelServerTools.value = next;
}

function setServerToolFormat(capability: string, format: string) {
  if (!format) {
    toggleServerTool(capability, false);
    return;
  }
  providerModelServerTools.value = {
    ...providerModelServerTools.value,
    [capability]: format,
  };
}

function onServerToolToggle(capability: string, event: Event) {
  toggleServerTool(capability, (event.target as HTMLInputElement | null)?.checked ?? false);
}

function formatCapabilitiesSummary(capabilities: {
  contextWindow: number;
  maxOutputTokens: number;
  supportsToolUse: boolean;
  supportsVision: boolean;
  supportsAudio: boolean;
  supportsPdf: boolean;
  serverTools: Array<{
    capability: string;
    format: string;
  }>;
}) {
  const serverToolLabels = capabilities.serverTools.map((tool) => {
    if (tool.capability === 'web_search') {
      return '搜索';
    }
    if (tool.capability === 'code_execution') {
      return '代码执行';
    }
    if (tool.capability === 'file_search') {
      return '文件检索';
    }
    return tool.capability;
  });
  const featureLabels = [
    capabilities.supportsToolUse ? '工具' : null,
    capabilities.supportsVision ? '图片' : null,
    capabilities.supportsAudio ? '音频' : null,
    capabilities.supportsPdf ? 'PDF' : null,
    ...serverToolLabels,
  ].filter(Boolean);
  const summary = featureLabels.length ? featureLabels.join(' · ') : '纯文本';
  return `${formatTokenMetric(capabilities.contextWindow)} context · ${formatTokenMetric(capabilities.maxOutputTokens)} output · ${summary}`;
}

function formatTokenMetric(value: number) {
  if (value >= 1_000_000) {
    return `${Math.round(value / 100_000) / 10}M`;
  }
  if (value >= 1_000) {
    return `${Math.round(value / 100) / 10}K`;
  }
  return String(value);
}
</script>
