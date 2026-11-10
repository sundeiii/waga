import { useEffect, useState } from "react";
import { BadgeCheck, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Input, Textarea } from "@/components/ui/Input";
import { api, errorMessage } from "@/services/api";
import { useUi } from "@/stores/ui";
import { formatTimestamp } from "@/lib/utils";
import type { HostCa } from "@/types";

/** Trusted certificate authorities for SSH host certificates (@cert-authority). */
export function HostCaSection() {
  const [cas, setCas] = useState<HostCa[]>([]);
  const [adding, setAdding] = useState(false);
  const [pattern, setPattern] = useState("");
  const [key, setKey] = useState("");
  const [comment, setComment] = useState("");
  const [error, setError] = useState<string | null>(null);
  const askConfirm = useUi((s) => s.askConfirm);
  const toast = useUi((s) => s.toast);

  const load = () => api.listHostCas().then(setCas).catch(() => undefined);
  useEffect(() => {
    void load();
  }, []);

  const add = async () => {
    try {
      await api.addHostCa(pattern, key, comment);
      setPattern("");
      setKey("");
      setComment("");
      setAdding(false);
      setError(null);
      await load();
      toast("Certificate authority added", "success");
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="pt-3">
      <div className="mb-2 flex items-center justify-between">
        <div>
          <div className="flex items-center gap-1.5 text-[13px]">
            <BadgeCheck className="size-4 text-accent" /> Certificate authorities
          </div>
          <div className="text-[12px] text-faint">Servers with a valid host certificate from these CAs connect without a fingerprint prompt.</div>
        </div>
        {!adding && (
          <Button size="sm" variant="secondary" onClick={() => setAdding(true)}>
            Add CA
          </Button>
        )}
      </div>

      {cas.map((c) => (
        <div key={c.id} className="flex items-center gap-3 py-2">
          <div className="min-w-0 flex-1">
            <div className="font-mono text-[12.5px]">{c.pattern}</div>
            <div className="selectable truncate font-mono text-[11.5px] text-faint">
              {c.publicKey.split(" ")[0]} · {c.fingerprint}
            </div>
            <div className="text-[11px] text-faint">
              {c.comment ? `${c.comment} · ` : ""}added {formatTimestamp(c.addedAt)}
            </div>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="size-7 hover:text-danger"
            aria-label={`Remove CA for ${c.pattern}`}
            onClick={() =>
              askConfirm({
                title: `Stop trusting this CA for ${c.pattern}?`,
                body: "Hosts using its certificates will ask for fingerprint confirmation again.",
                confirmLabel: "Remove",
                danger: true,
                onConfirm: async () => {
                  await api.removeHostCa(c.id);
                  await load();
                },
              })
            }
          >
            <Trash2 />
          </Button>
        </div>
      ))}

      {adding && (
        <div className="mt-2 space-y-2 rounded-xl border border-line bg-shade/15 p-3">
          <Input value={pattern} onChange={(e) => setPattern(e.target.value)} placeholder="Host pattern, e.g. *.corp.example.com" className="font-mono" aria-label="Host pattern" />
          <Textarea
            value={key}
            onChange={(e) => setKey(e.target.value)}
            rows={3}
            spellCheck={false}
            placeholder="CA public key, e.g. ssh-ed25519 AAAAC3Nz… ca@corp"
            className="font-mono text-[12px]"
            aria-label="CA public key"
          />
          <Input value={comment} onChange={(e) => setComment(e.target.value)} placeholder="Note (optional)" aria-label="Note" />
          {error && <p className="text-[12px] text-danger">{error}</p>}
          <p className="text-[11.5px] text-faint">Only add a CA you got from your administrator through a trusted channel. A CA can vouch for every host matching its pattern.</p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setAdding(false)}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" disabled={!pattern.trim() || !key.trim()} onClick={() => void add()}>
              Trust CA
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
