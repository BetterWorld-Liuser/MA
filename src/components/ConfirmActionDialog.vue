<template>
  <AlertDialog :open="open" @update:open="emit('update:open', $event)">
    <AlertDialogContent class="overflow-hidden bg-[linear-gradient(180deg,rgba(255,255,255,0.035),rgba(255,255,255,0.015)),rgba(10,10,10,0.94)]">
      <AlertDialogHeader class="gap-0 px-5 pb-3 pt-5 text-left">
        <AlertDialogTitle class="text-[18px] font-semibold tracking-[-0.01em] text-text">
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
          class="!border-[color:var(--color-error)] !bg-[color:var(--color-error)] !text-[var(--destructive-foreground)] hover:!border-[#b93f3f] hover:!bg-[#b93f3f] focus-visible:!ring-[rgba(224,82,82,0.28)]"
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
