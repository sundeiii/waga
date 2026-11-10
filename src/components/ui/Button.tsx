import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const button = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap font-medium transition-[background,color,border-color,box-shadow,transform] duration-200 ease-[var(--ease-out-soft)] disabled:pointer-events-none disabled:opacity-45 active:scale-[0.98] [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary:
          "bg-accent text-[#14121f] shadow-[0_0_0_1px_rgb(255_255_255/0.12)_inset,0_8px_24px_-8px_var(--accent)] hover:brightness-110",
        secondary: "bg-ink/[0.06] text-fg border border-line hover:bg-ink/[0.1] hover:border-line-strong",
        ghost: "text-muted hover:text-fg hover:bg-ink/[0.06]",
        danger: "bg-danger/15 text-danger border border-danger/30 hover:bg-danger/25",
        outline: "border border-line text-fg hover:bg-ink/[0.05] hover:border-line-strong",
      },
      size: {
        sm: "h-8 px-3 text-[13px] rounded-[10px]",
        md: "h-9 px-4 text-sm rounded-xl",
        lg: "h-11 px-5 text-sm rounded-xl",
        icon: "size-8 rounded-[10px]",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof button> {}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(({ className, variant, size, type = "button", ...props }, ref) => (
  <button ref={ref} type={type} className={cn(button({ variant, size }), className)} {...props} />
));
Button.displayName = "Button";
