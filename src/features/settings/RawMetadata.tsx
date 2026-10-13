import { Copy } from "lucide-react";
import { useMemo, useState } from "react";
import { Button, EmptyState, Label } from "@/components/ui";
import { useSchema } from "@/hooks/queries";
import { buildSchemaIndex } from "@/lib/schemaIndex";
import { useUi } from "@/stores/ui";

export function RawMetadata() {
  const { data, isLoading } = useSchema();
  const selectedId = useUi((s) => s.selectedId);
  const [scope, setScope] = useState<"selection" | "schema">("selection");
  const index = useMemo(() => (data ? buildSchemaIndex(data.schema) : null), [data]);

  const content = useMemo(() => {
    if (!data || !index) return null;
    if (scope === "selection" && selectedId) {
      const hit =
        index.tables.get(selectedId) ??
        index.views.get(selectedId) ??
        index.columns.get(selectedId)?.parent ??
        index.owners.get(selectedId);
      if (hit) return hit;
    }
    return data.schema;
  }, [data, index, scope, selectedId]);

  if (isLoading) return <EmptyState title="Reading catalog…" />;
  if (!content) return null;
  const text = JSON.stringify(content, null, 2);
  const isSelection = content !== data?.schema;

  return (
    <div className="flex h-full flex-col p-6">
      <div className="mb-3 flex items-center gap-3">
        <Label>RAW METADATA</Label>
        <span className="font-mono text-[11px] text-fg-dim">
          normalized model (what the UI receives), not raw catalog rows
        </span>
        <div className="ml-auto flex items-center gap-2">
          <select
            value={scope}
            onChange={(e) => setScope(e.target.value as "selection" | "schema")}
            aria-label="Scope"
            className="h-7 rounded-sm border border-line-strong bg-ink-2 px-1.5 font-mono text-[11px]"
          >
            <option value="selection">Selected object</option>
            <option value="schema">Whole schema</option>
          </select>
          <Button onClick={() => void navigator.clipboard.writeText(text)}>
            <Copy size={12} /> Copy
          </Button>
        </div>
      </div>
      {!isSelection && scope === "selection" && (
        <p className="mb-2 text-[11px] text-fg-dim">Nothing selected: showing the whole schema.</p>
      )}
      <pre className="min-h-0 flex-1 overflow-auto rounded-sm border border-line bg-ink-0 p-3 font-mono text-[11px] leading-relaxed text-fg-muted">
        {text}
      </pre>
    </div>
  );
}
