<script setup lang="ts">
import type { HTMLAttributes } from "vue"
import { ref } from "vue"
import { useVModel } from "@vueuse/core"
import { cn } from "@/lib/utils"

const props = defineProps<{
  defaultValue?: string | number
  modelValue?: string | number
  class?: HTMLAttributes["class"]
}>()

const emits = defineEmits<{
  (e: "update:modelValue", payload: string | number): void
}>()

const modelValue = useVModel(props, "modelValue", emits, {
  passive: true,
  defaultValue: props.defaultValue,
})

const inputRef = ref<HTMLInputElement | null>(null)

defineExpose({
  focus: () => inputRef.value?.focus(),
  select: () => inputRef.value?.select(),
})
</script>

<template>
  <input
    ref="inputRef"
    v-model="modelValue"
    data-slot="input"
    :class="cn(
      'file:text-foreground placeholder:text-text-dim selection:bg-primary selection:text-primary-foreground h-10 w-full min-w-0 rounded-xl border border-input bg-bg-secondary px-3 py-2 text-[13px] text-foreground shadow-none transition-[color,box-shadow,border-color] outline-hidden file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50',
      'focus-visible:border-ring focus-visible:ring-ring/40 focus-visible:ring-[3px]',
      'aria-invalid:ring-destructive/20 aria-invalid:border-destructive',
      props.class,
    )"
  >
</template>
