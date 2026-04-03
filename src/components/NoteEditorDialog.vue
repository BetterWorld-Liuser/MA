<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogContent class="overflow-hidden bg-[linear-gradient(180deg,rgba(255,255,255,0.035),rgba(255,255,255,0.015)),rgba(10,10,10,0.94)]">
      <form class="contents" @submit.prevent="emit('submit')">
        <DialogHeader class="gap-0 px-5 pb-3 pt-5 text-left">
          <DialogTitle class="text-[18px] font-semibold tracking-[-0.01em] text-text">
            {{ mode === 'edit' ? `编辑 Note · ${draftId}` : '新增 Note' }}
          </DialogTitle>
          <DialogDescription class="mt-1 text-[12px] leading-5 text-text-muted">
            Notes 会直接进入 AI 下一轮上下文，适合保留目标、约束和临时决策。
          </DialogDescription>
        </DialogHeader>
        <div class="space-y-4 px-5 pb-4">
          <div class="dialog-field">
            <label class="dialog-label" for="note-id">Note id</label>
            <Input
              id="note-id"
              ref="noteIdInputRef"
              :model-value="draftId"
              class="font-mono"
              maxlength="40"
              placeholder="target"
              :disabled="mode === 'edit'"
              @update:model-value="emit('update:draft-id', String($event))"
            />
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="note-content">Content</label>
            <Textarea
              id="note-content"
              ref="noteContentInputRef"
              :model-value="draftContent"
              placeholder="写下这轮之后仍然重要的信息。"
              @update:model-value="emit('update:draft-content', String($event))"
            />
          </div>
        </div>
        <DialogFooter class="border-t border-white/8 px-5 py-4 sm:justify-end">
          <Button type="button" variant="outline" :disabled="busy" @click="emit('cancel')">取消</Button>
          <Button type="submit" :disabled="busy">{{ mode === 'edit' ? '保存修改' : '添加 Note' }}</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
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
  draftContent: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  'update:draft-id': [value: string];
  'update:draft-content': [value: string];
  submit: [];
  cancel: [];
}>();

const noteIdInputRef = ref<FocusableField | null>(null);
const noteContentInputRef = ref<FocusableField | null>(null);

defineExpose({
  focusIdField() {
    noteIdInputRef.value?.focus();
    noteIdInputRef.value?.select();
  },
  focusContentField() {
    noteContentInputRef.value?.focus();
    noteContentInputRef.value?.select();
  },
});
</script>
