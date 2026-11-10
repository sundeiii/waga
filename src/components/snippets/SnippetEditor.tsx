import { useEffect, useState } from "react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { Field, Input, Select, Textarea } from "@/components/ui/Input";
import { Switch } from "@/components/ui/Switch";
import { errorMessage } from "@/services/api";
import { CATEGORY_LABEL, useSnippets } from "@/stores/snippets";
import { useUi } from "@/stores/ui";
import { parseTags } from "@/lib/validation";
import { SNIPPET_CATEGORIES, type SnippetCategory, type SnippetInput } from "@/types";

const blank = (): SnippetInput => ({ title: "", description: "", command: "", category: "custom", tags: [], favorite: false });

export function SnippetEditor() {
  const { editor, closeEditor, save } = useSnippets();
  const toast = useUi((s) => s.toast);
  const [form, setForm] = useState<SnippetInput>(blank);
  const [tags, setTags] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!editor.open) return;
    const base = editor.editing ? { ...editor.editing } : { ...blank(), ...editor.prefill };
    setForm(base);
    setTags(base.tags.join(", "));
    setError(null);
  }, [editor]);

  const set = <K extends keyof SnippetInput>(k: K, v: SnippetInput[K]) => setForm((f) => ({ ...f, [k]: v }));

  const submit = async () => {
    if (!form.title.trim() || !form.command.trim()) {
      setError("Title and command are required");
      return;
    }
    setSaving(true);
    try {
      await save({ ...form, tags: parseTags(tags) }, editor.editing?.id);
      toast(editor.editing ? "Snippet saved" : "Snippet added", "success");
      closeEditor();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={editor.open}
      onOpenChange={(o) => !o && closeEditor()}
      title={editor.editing ? "Edit snippet" : "New snippet"}
      description="Snippets are stored locally. Their contents are never written to the activity log."
      className="w-[min(640px,calc(100vw-48px))]"
      footer={
        <>
          {error && <p className="mr-auto text-[12.5px] text-danger">{error}</p>}
          <Button variant="ghost" onClick={closeEditor}>
            Cancel
          </Button>
          <Button variant="primary" disabled={saving} onClick={() => void submit()}>
            {saving ? "Saving…" : "Save snippet"}
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-6 gap-4 pb-4">
        <Field label="Title" className="col-span-4" htmlFor="sn-title">
          <Input id="sn-title" autoFocus value={form.title} onChange={(e) => set("title", e.target.value)} placeholder="Restart nginx safely" />
        </Field>
        <Field label="Category" className="col-span-2" htmlFor="sn-cat">
          <Select id="sn-cat" value={form.category} onChange={(e) => set("category", e.target.value as SnippetCategory)}>
            {SNIPPET_CATEGORIES.map((c) => (
              <option key={c} value={c}>
                {CATEGORY_LABEL[c]}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Command" className="col-span-6" htmlFor="sn-cmd" hint="Inserted exactly as written. Trailing newlines are removed so inserting never runs it.">
          <Textarea
            id="sn-cmd"
            value={form.command}
            onChange={(e) => set("command", e.target.value)}
            rows={5}
            spellCheck={false}
            placeholder="sudo nginx -t && sudo systemctl reload nginx"
            className="font-mono text-[12.5px] leading-relaxed"
          />
        </Field>
        <Field label="Description" className="col-span-6" htmlFor="sn-desc">
          <Input id="sn-desc" value={form.description} onChange={(e) => set("description", e.target.value)} placeholder="What it does, when to use it" />
        </Field>
        <Field label="Tags" hint="Comma separated" className="col-span-4" htmlFor="sn-tags">
          <Input id="sn-tags" value={tags} onChange={(e) => setTags(e.target.value)} placeholder="web, nginx" />
        </Field>
        <label className="col-span-2 flex items-center justify-between self-end rounded-xl border border-line bg-black/15 px-3 py-2 text-[13px]">
          Favorite
          <Switch checked={form.favorite} onCheckedChange={(v) => set("favorite", v)} label="Favorite" />
        </label>
      </div>
    </Dialog>
  );
}
