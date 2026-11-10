import * as RS from "@radix-ui/react-switch";
import { cn } from "@/lib/utils";

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  id,
  label,
}: {
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
  disabled?: boolean;
  id?: string;
  label?: string;
}) {
  return (
    <RS.Root
      id={id}
      aria-label={label}
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      className={cn(
        "relative inline-flex h-[22px] w-[38px] shrink-0 items-center rounded-full border border-line transition-colors duration-200",
        "data-[state=checked]:border-transparent data-[state=checked]:bg-accent data-[state=unchecked]:bg-white/10 disabled:opacity-40",
      )}
    >
      <RS.Thumb className="block size-[16px] translate-x-[3px] rounded-full bg-white shadow-md transition-transform duration-200 ease-[var(--ease-out-soft)] data-[state=checked]:translate-x-[18px]" />
    </RS.Root>
  );
}
