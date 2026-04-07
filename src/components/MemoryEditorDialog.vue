<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogContent class="overflow-hidden bg-[linear-gradient(180deg,rgba(255,255,255,0.035),rgba(255,255,255,0.015)),rgba(10,10,10,0.94)]">
      <form class="contents" @submit.prevent="emit('submit')">
        <DialogHeader class="gap-0 px-5 pb-3 pt-5 text-left">
          <DialogTitle class="text-[18px] font-semibold tracking-[-0.01em] text-text">
            {{ mode === 'edit' ? `编辑 Memory · ${draftId}` : '新增 Memory' }}
          </DialogTitle>
          <DialogDescription class="mt-1 text-[12px] leading-5 text-text-muted">
            记忆会跨 task / session 保留，适合存项目事实、决策、模式和用户偏好。
          </DialogDescription>
        </DialogHeader>
        <div class="grid gap-4 px-5 pb-4">
          <div class="grid gap-4 sm:grid-cols-2">
            <div class="dialog-field">
              <label class="dialog-label" for="memory-id">Memory id</label>
              <Input
                id="memory-id"
                ref="idInputRef"
                :model-value="draftId"
                class="font-mono"
                :disabled="mode === 'edit'"
                @update:model-value="emit('update:draft-id', String($event))"
              />
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="memory-type">Type</label>
              <Input
                id="memory-type"
                :model-value="draftType"
                placeholder="fact"
                @update:model-value="emit('update:draft-type', String($event))"
              />
            </div>
          </div>
          <div class="grid gap-4 sm:grid-cols-2">
            <div class="dialog-field">
              <label class="dialog-label" for="memory-topic">Topic</label>
              <Input
                id="memory-topic"
                :model-value="draftTopic"
                placeholder="auth"
                @update:model-value="emit('update:draft-topic', String($event))"
              />
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="memory-level">Level</label>
              <Input
                id="memory-level"
                :model-value="draftLevel"
                placeholder="project"
                @update:model-value="emit('update:draft-level', String($event))"
              />
            </div>
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="memory-title">Title</label>
            <Input
              id="memory-title"
              :model-value="draftTitle"
              placeholder="JWT refresh token 有效期 7 天"
              @update:model-value="emit('update:draft-title', String($event))"
            />
          </div>
          <div class="grid gap-4 sm:grid-cols-2">
            <div class="dialog-field">
              <label class="dialog-label" for="memory-tags">Tags</label>
              <Input
                id="memory-tags"
                :model-value="draftTags"
                placeholder="auth jwt token src/auth"
                @update:model-value="emit('update:draft-tags', String($event))"
              />
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="memory-scope">Scope</label>
              <Input
                id="memory-scope"
                :model-value="draftScope"
                placeholder="shared"
                @update:model-value="emit('update:draft-scope', String($event))"
              />
            </div>
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="memory-content">Content</label>
            <Textarea
              id="memory-content"
              ref="contentInputRef"
              :model-value="draftContent"
              placeholder="写下完整记忆内容。"
              @update:model-value="emit('update:draft-content', String($event))"
            />
          </div>
        </div>
        <DialogFooter class="border-t border-white/8 px-5 py-4 sm:justify-end">
          <Button type="button" variant="outline" :disabled="busy" @click="emit('cancel')">取消</Button>
          <Button type="submit" :disabled="busy">{{ mode === 'edit' ? '保存修改' : '添加 Memory' }}</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';

type FocusableField = {
  focus: () => void;
  select: () => void;
};

defineProps<{
  open: boolean;
  mode: 'create' | 'edit';
  draftId: string;
  draftType: string;
  draftTopic: string;
  draftTitle: string;
  draftContent: string;
  draftTags: string;
  draftScope: string;
  draftLevel: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  'update:draft-id': [value: string];
  'update:draft-type': [value: string];
  'update:draft-topic': [value: string];
  'update:draft-title': [value: string];
  'update:draft-content': [value: string];
  'update:draft-tags': [value: string];
  'update:draft-scope': [value: string];
  'update:draft-level': [value: string];
  submit: [];
  cancel: [];
}>();

const idInputRef = ref<FocusableField | null>(null);
const contentInputRef = ref<FocusableField | null>(null);

defineExpose({
  focusIdField() {
    idInputRef.value?.focus();
    idInputRef.value?.select();
  },
  focusContentField() {
    contentInputRef.value?.focus();
    contentInputRef.value?.select();
  },
});
</script>
