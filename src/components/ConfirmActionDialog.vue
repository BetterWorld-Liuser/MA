<template>
  <AlertDialog :open="open" @update:open="emit('update:open', $event)">
    <AlertDialogContent class="dialog-glass-surface overflow-hidden">
      <AlertDialogHeader class="gap-0 px-5 pb-3 pt-5 text-left">
        <AlertDialogTitle class="heading-serif-lg text-text">
          {{ title }}
        </AlertDialogTitle>
        <AlertDialogDescription class="mt-1 text-[12px] leading-5 text-text-muted">
          {{ description }}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <div class="px-5 pb-4">
        <p class="text-[13px] leading-6 text-text">
          {{ body }}
        </p>
      </div>
      <AlertDialogFooter class="border-t border-white/8 px-5 py-4 sm:justify-end">
        <AlertDialogCancel :disabled="busy">取消</AlertDialogCancel>
        <AlertDialogAction
          class="!border-[color:var(--color-error)] !bg-[color:var(--color-error)] !text-[var(--destructive-foreground)] hover:!border-[color:var(--destructive-hover-bg)] hover:!bg-[color:var(--destructive-hover-bg)] focus-visible:!ring-[color:var(--destructive-ring)]"
          :disabled="busy"
          @click="emit('confirm')"
        >
          {{ confirmLabel }}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>

<script setup lang="ts">
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';

defineProps<{
  open: boolean;
  title: string;
  description: string;
  body: string;
  confirmLabel: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  confirm: [];
}>();
</script>
