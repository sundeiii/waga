import { forwardRef, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type TextareaHTMLAttributes } from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

const base =
  // Focus = accent border + soft 3px halo (no browser outline, no double ring).
  "w-full rounded-xl bg-black/25 border border-line text-sm text-fg placeholder:text-faint transition-[border-color,box-shadow] duration-150 hover:border-line-strong outline-none focus-visible:outline-none focus:border-accent/60 focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--accent)_16%,transparent)] disabled:opacity-50 aria-[invalid=true]:border-danger/60 aria-[invalid=true]:focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-danger)_16%,transparent)]";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(({ className, ...p }, ref) => (
  <input ref={ref} className={cn(base, "h-9 px-3", className)} spellCheck={false} autoComplete="off" {...p} />
));
Input.displayName = "Input";

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(({ className, ...p }, ref) => (
  <textarea ref={ref} className={cn(base, "px-3 py-2 min-h-20 resize-y", className)} {...p} />
));
Textarea.displayName = "Textarea";

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(({ className, children, ...p }, ref) => (
  <div className="relative">
    <select ref={ref} className={cn(base, "h-9 appearance-none pl-3 pr-8 [&>option]:bg-elevated", className)} {...p}>
      {children}
    </select>
    <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 size-4 -translate-y-1/2 text-faint" />
  </div>
));
Select.displayName = "Select";

export function Field({
  label,
  hint,
  error,
  children,
  className,
  htmlFor,
}: {
  label: string;
  hint?: ReactNode;
  error?: string | null;
  children: ReactNode;
  className?: string;
  htmlFor?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={htmlFor} className="text-[12px] font-medium tracking-wide text-muted">
        {label}
      </label>
      {children}
      {error ? (
        <p role="alert" className="text-[12px] text-danger">
          {error}
        </p>
      ) : hint ? (
        <p className="text-[12px] text-faint">{hint}</p>
      ) : null}
    </div>
  );
}
