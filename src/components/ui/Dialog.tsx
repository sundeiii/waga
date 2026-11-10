import * as RD from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  className,
  dismissable = true,
  icon,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  className?: string;
  /** When false, Esc / outside click do nothing (used for security prompts). */
  dismissable?: boolean;
  icon?: ReactNode;
}) {
  return (
    <RD.Root open={open} onOpenChange={onOpenChange}>
      <RD.Portal>
        <RD.Overlay className="fixed inset-0 z-40 bg-black/45 backdrop-blur-[6px] data-[state=open]:animate-[fade-in_180ms_ease-out]" />
        <RD.Content
          onEscapeKeyDown={(e) => !dismissable && e.preventDefault()}
          onPointerDownOutside={(e) => !dismissable && e.preventDefault()}
          onInteractOutside={(e) => !dismissable && e.preventDefault()}
          className={cn(
            "glass-elevated fixed left-1/2 top-1/2 z-50 flex max-h-[88vh] w-[min(560px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl",
            "data-[state=open]:animate-[dialog-in_220ms_var(--ease-out-soft)]",
            className,
          )}
        >
          <div className="flex items-start gap-3 px-6 pt-5 pb-3">
            {icon}
            <div className="min-w-0 flex-1">
              <RD.Title className="text-[15px] font-semibold tracking-tight">{title}</RD.Title>
              {description ? (
                <RD.Description className="mt-1 text-[13px] leading-relaxed text-muted">{description}</RD.Description>
              ) : (
                <RD.Description className="sr-only">{typeof title === "string" ? title : "Dialog"}</RD.Description>
              )}
            </div>
            {dismissable && (
              <RD.Close className="-mr-2 rounded-lg p-1.5 text-faint transition-colors hover:bg-ink/5 hover:text-fg" aria-label="Close">
                <X className="size-4" />
              </RD.Close>
            )}
          </div>
          {/* Padding on all sides so focus halos of fields aren't clipped by the scroll box. */}
          {children ? <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-2 pt-1">{children}</div> : null}
          {footer ? <div className="flex items-center justify-end gap-2 border-t border-line px-6 py-4">{footer}</div> : null}
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  );
}
