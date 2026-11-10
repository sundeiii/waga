import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, CheckCircle2, Info, XCircle } from "lucide-react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { useUi } from "@/stores/ui";
import { errorMessage } from "@/services/api";
import { cn } from "@/lib/utils";

export function ConfirmDialog() {
  const confirm = useUi((s) => s.confirm);
  const closeConfirm = useUi((s) => s.closeConfirm);
  const toast = useUi((s) => s.toast);
  const [busy, setBusy] = useState(false);
  if (!confirm) return null;
  const cancel = () => {
    confirm.onCancel?.();
    closeConfirm();
  };
  const run = async () => {
    setBusy(true);
    try {
      await confirm.onConfirm();
      closeConfirm();
    } catch (e) {
      toast(errorMessage(e), "danger");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && cancel()}
      title={confirm.title}
      description={confirm.body}
      className={confirm.code ? "w-[min(640px,calc(100vw-48px))]" : undefined}
      footer={
        <>
          <Button variant="ghost" onClick={cancel}>
            Cancel
          </Button>
          <Button variant={confirm.danger ? "danger" : "primary"} onClick={run} disabled={busy}>
            {confirm.confirmLabel}
          </Button>
        </>
      }
    >
      {confirm.code ? (
        <pre className="selectable mb-3 max-h-64 overflow-auto whitespace-pre-wrap break-all rounded-xl border border-line bg-shade/35 p-3 font-mono text-[12.5px] leading-relaxed text-fg">{confirm.code}</pre>
      ) : null}
    </Dialog>
  );
}

const ICON = { info: Info, success: CheckCircle2, warning: AlertTriangle, danger: XCircle };
const COLOR = { info: "text-accent", success: "text-success", warning: "text-warning", danger: "text-danger" };

export function Toaster() {
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismissToast);
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-80 flex-col gap-2" role="status" aria-live="polite">
      <AnimatePresence initial={false}>
        {toasts.map((t) => {
          const Icon = ICON[t.tone];
          return (
            <motion.button
              key={t.id}
              layout
              initial={{ opacity: 0, y: 12, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, x: 24 }}
              transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
              onClick={() => dismiss(t.id)}
              className="glass-elevated pointer-events-auto flex items-start gap-2.5 rounded-xl px-3.5 py-3 text-left text-[13px]"
            >
              <Icon className={cn("mt-px size-4 shrink-0", COLOR[t.tone])} />
              <span className="leading-snug">{t.text}</span>
            </motion.button>
          );
        })}
      </AnimatePresence>
    </div>
  );
}
