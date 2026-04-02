import type { VariantProps } from "class-variance-authority"
import { cva } from "class-variance-authority"

export { default as Button } from "./Button.vue"

export const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-xl border text-[12px] font-medium transition-all disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-hidden focus-visible:border-ring focus-visible:ring-ring/40 focus-visible:ring-[3px]",
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-primary text-primary-foreground hover:bg-[var(--ma-accent-hover)]",
        destructive:
          "border-[rgba(224,82,82,0.25)] bg-[rgba(224,82,82,0.16)] text-[#ffb2b2] hover:bg-[rgba(224,82,82,0.24)] focus-visible:ring-[rgba(224,82,82,0.2)]",
        outline:
          "border-border bg-[rgba(255,255,255,0.02)] text-foreground hover:bg-bg-hover",
        secondary:
          "border-transparent bg-bg-secondary text-foreground hover:bg-bg-hover",
        ghost:
          "border-transparent bg-transparent text-text-muted hover:bg-bg-hover hover:text-foreground",
        link: "border-transparent bg-transparent px-0 text-primary underline-offset-4 hover:underline",
      },
      size: {
        "default": "min-h-9 px-4 py-2 has-[>svg]:px-3",
        "sm": "min-h-8 gap-1.5 px-3 has-[>svg]:px-2.5",
        "lg": "min-h-10 px-6 has-[>svg]:px-4",
        "icon": "size-9",
        "icon-sm": "size-8",
        "icon-lg": "size-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
)
export type ButtonVariants = VariantProps<typeof buttonVariants>
