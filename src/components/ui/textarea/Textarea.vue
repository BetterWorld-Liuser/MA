<script setup lang="ts">
import type { HTMLAttributes } from "vue"
import { ref } from "vue"
import { useVModel } from "@vueuse/core"
import { cn } from "@/lib/utils"

const props = defineProps<{
  class?: HTMLAttributes["class"]
  defaultValue?: string | number
  modelValue?: string | number
}>()

const emits = defineEmits<{
  (e: "update:modelValue", payload: string | number): void
}>()

const modelValue = useVModel(props, "modelValue", emits, {
  passive: true,
  defaultValue: props.defaultValue,
})

const textareaRef = ref<HTMLTextAreaElement | null>(null)

defineExpose({
  focus: () => textareaRef.value?.focus(),
  select: () => textareaRef.value?.select(),
})
</script>

<template>
  <textarea
    ref="textareaRef"
    v-model="modelValue"
    data-slot="textarea"
    :class="cn('border-input placeholder:text-text-dim focus-visible:border-ring focus-visible:ring-ring/40 aria-invalid:ring-destructive/20 aria-invalid:border-destructive flex field-sizing-content min-h-28 w-full rounded-xl border bg-bg-secondary px-3 py-2 text-[13px] leading-6 text-foreground shadow-none transition-[color,box-shadow,border-color] outline-hidden focus-visible:ring-[3px] disabled:cursor-not-allowed disabled:opacity-50', props.class)"
  />
</template>
