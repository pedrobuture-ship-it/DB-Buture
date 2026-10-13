import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { EmptyState, ErrorNote, Label, cx } from "@/components/ui";
import { EdgeLegend, MiniGraph } from "@/components/MiniGraph";
import { useSchema } from "@/hooks/queries";
import { ipc } from "@/lib/ipc";
import type { LayoutMode } from "@/lib/layout";
import { useUi } from "@/stores/ui";
import type { AppError, GraphNode } from "@/types/schema";

type Filter = "all" | "tables" | "views" | "routines";

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "tables", label: "Tables" },
  { id: "views", label: "Views" },
  { id: "routines", label: "Routines" },
];

function keep(filter: Filter, n: GraphNode): boolean {
  switch (filter) {
    case "tables":
      return n.kind === "table";
    case "views":
      return n.kind === "view" || n.kind === "materialized_view" || n.kind === "table";
    case "routines":
      return n.kind === "routine" || n.kind === "trigger" || n.kind === "table";
    default:
      return true;
  }
}

export function DependencyGraph() {
  const schema = useSchema();
  const select = useUi((s) => s.select);
  const [filter, setFilter] = useState<Filter>("all");
  const [mode, setMode] = useState<LayoutMode>("horizontal");
  const [hideIsolated, setHideIsolated] = useState(true);

  const graph = useQuery({
    queryKey: ["analysis", "graph", schema.data?.fetchedAt],
    queryFn: ipc.dependencyGraph,
    enabled: !!schema.data,
  });

  const view = useMemo(() => {
    if (!graph.data) return null;
    const nodes0 = graph.data.nodes.filter((n) => keep(filter, n));
    const ids = new Set(nodes0.map((n) => n.id));
    const edges = graph.data.edges.filter((e) => ids.has(e.from) && ids.has(e.to));
    const linked = new Set(edges.flatMap((e) => [e.from, e.to]));
    const nodes = nodes0.filter((n) => !hideIsolated || linked.has(n.id));
    return { nodes, edges };
  }, [graph.data, filter, hideIsolated]);

  if (schema.isLoading || graph.isLoading) return <EmptyState title="Building graph…" />;
  const err = (schema.error ?? graph.error) as AppError | null;
  if (err)
    return (
      <div className="p-6">
        <ErrorNote error={err} />
      </div>
    );
  if (!view || view.nodes.length === 0) {
    return (
      <EmptyState title="No dependencies found">
        No foreign keys, view dependencies or trigger links were found for this filter.
      </EmptyState>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-line bg-ink-1/90 px-3 py-2">
        <Label>filter</Label>
        <div className="flex gap-1">
          {FILTERS.map((f) => (
            <button
              key={f.id}
              type="button"
              onClick={() => setFilter(f.id)}
              aria-pressed={filter === f.id}
              className={cx(
                "h-6 rounded-sm border px-2.5 font-mono text-[11px]",
                filter === f.id
                  ? "border-cyan bg-cyan-wash text-cyan"
                  : "border-line-strong text-fg-muted hover:text-fg",
              )}
            >
              {f.label}
            </button>
          ))}
        </div>
        <select
          aria-label="Direction"
          value={mode}
          onChange={(e) => setMode(e.target.value as LayoutMode)}
          className="h-6 rounded-sm border border-line-strong bg-ink-2 px-1.5 font-mono text-[11px]"
        >
          <option value="horizontal">Left → right</option>
          <option value="vertical">Top → bottom</option>
          <option value="compact">Compact</option>
        </select>
        <label className="flex items-center gap-1.5 text-[11px] text-fg-muted">
          <input
            type="checkbox"
            checked={hideIsolated}
            onChange={(e) => setHideIsolated(e.target.checked)}
          />
          hide unconnected
        </label>
        <span className="ml-auto font-mono text-[10px] text-fg-dim">
          {view.nodes.length} nodes · {view.edges.length} edges
        </span>
      </div>
      <div className="relative min-h-0 flex-1">
        <MiniGraph
          nodes={view.nodes}
          edges={view.edges}
          mode={mode}
          onSelect={(id) => select(id, "explorer")}
        />
        <div className="absolute bottom-3 left-3 rounded-sm border border-line bg-ink-1/95 px-3 py-1.5">
          <EdgeLegend />
        </div>
      </div>
    </div>
  );
}
