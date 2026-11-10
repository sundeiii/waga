import { SquareCode } from "lucide-react";
import { EmptyState, Panel } from "@/components/ui/misc";

const INFO = {
  snippets: { icon: <SquareCode />, title: "Snippets are not implemented yet", body: "Planned for Phase 6: a searchable command library with insert-without-running as the default and explicit confirmation to execute." },
} as const;

export function ComingSoon({ which }: { which: keyof typeof INFO }) {
  const i = INFO[which];
  return (
    <Panel className="mx-auto mt-6 max-w-2xl">
      <EmptyState icon={i.icon} title={i.title} body={i.body} />
    </Panel>
  );
}
