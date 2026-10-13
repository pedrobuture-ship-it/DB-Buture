import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import {
  Badge,
  EmptyState,
  ErrorNote,
  EvidenceBadge,
  Label,
  Mono,
  Panel,
  cx,
} from "@/components/ui";
import { MiniGraph, EdgeLegend } from "@/components/MiniGraph";
import { useSchema } from "@/hooks/queries";
import { fuzzyFilter } from "@/lib/fuzzy";
import { ipc } from "@/lib/ipc";
import { buildSchemaIndex } from "@/lib/schemaIndex";
import { useUi } from "@/stores/ui";
import type { AppError, GraphEdge, GraphNode, ImpactItem, ImpactReport } from "@/types/schema";

const LEVEL_TEXT = {
  none: "text-fg-dim",
  low: "text-cyan",
  medium: "text-amber",
  high: "text-red",
} as const;
const LEVEL_TONE = { none: "neutral", low: "cyan", medium: "amber", high: "red" } as const;

function ObjectPicker() {
  const { data } = useSchema();
  const select = useUi((s) => s.select);
  const [q, setQ] = useState("");
  const options = useMemo(() => {
    if (!data) return [];
    const s = data.schema;
    return [
      ...s.tables.flatMap((t) => [
        { id: t.stableId, label: `${t.namespace}.${t.name}` },
        ...t.columns.map((c) => ({ id: c.stableId, label: `${t.namespace}.${t.name}.${c.name}` })),
      ]),
      ...s.views.map((v) => ({ id: v.stableId, label: `${v.namespace}.${v.name}` })),
    ];
  }, [data]);
  const results = q.trim() ? fuzzyFilter(options, q, (o) => o.label).slice(0, 8) : [];
  return (
    <div className="relative w-96">
      <input
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Analyze a table, view or column…"
        aria-label="Object to analyze"
        className="h-8 w-full rounded-sm border border-line-strong bg-ink-2 px-2.5 font-mono text-[12px] placeholder:text-fg-dim focus:border-cyan focus:outline-none"
      />
      {results.length > 0 && (
        <ul className="absolute z-20 mt-1 w-full rounded-sm border border-line-strong bg-ink-2 py-1 shadow-lg">
          {results.map((r) => (
            <li key={r.id}>
              <button
                type="button"
                className="block w-full truncate px-3 py-1 text-left font-mono text-[12px] hover:bg-ink-4"
                onClick={() => {
                  select(r.id);
                  setQ("");
                }}
              >
                {r.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function ItemList({
  title,
  items,
  names,
}: {
  title: string;
  items: ImpactItem[];
  names: Map<string, string>;
}) {
  const select = useUi((s) => s.select);
  return (
    <Panel title={`${title} · ${items.length}`}>
      {items.length === 0 ? (
        <p className="p-3 text-[12px] text-fg-dim">None.</p>
      ) : (
        <ul>
          {items.map((i) => (
            <li
              key={`${i.id}:${i.relation}`}
              className="flex items-center gap-2 border-b border-line px-3 py-1.5 last:border-b-0"
            >
              <Badge>{i.kind.replace("_", " ")}</Badge>
              <button
                type="button"
                className="truncate font-mono text-[12px] text-cyan hover:underline"
                onClick={() => select(i.id, "explorer")}
              >
                {names.get(i.id) ?? i.name}
              </button>
              <span className="font-mono text-[10px] text-fg-dim">
                {i.relation.replace("_", " ").toLowerCase()}
              </span>
              {i.depth > 1 && (
                <span className="font-mono text-[10px] text-fg-dim">
                  via {names.get(i.via) ?? i.via}
                </span>
              )}
              <span className="ml-auto">
                <EvidenceBadge evidence={i.evidence} />
              </span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

function graphOf(
  report: ImpactReport,
  names: Map<string, string>,
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes = new Map<string, GraphNode>();
  const tableId = report.target;
  nodes.set(tableId, { id: tableId, name: report.targetName, namespace: "", kind: "table" });
  const edges: GraphEdge[] = [];
  for (const i of [...report.direct, ...report.transitive]) {
    nodes.set(i.id, {
      id: i.id,
      name: names.get(i.id)?.split(".").pop() ?? i.name,
      namespace: i.namespace,
      kind: i.kind,
    });
    if (!nodes.has(i.via))
      nodes.set(i.via, {
        id: i.via,
        name: names.get(i.via)?.split(".").pop() ?? i.via,
        namespace: "",
        kind: "table",
      });
    edges.push({ from: i.id, to: i.via, kind: i.relation });
  }
  // a column target maps onto its owning object in the graph
  return { nodes: [...nodes.values()], edges };
}

export function ImpactView() {
  const schema = useSchema();
  const selectedId = useUi((s) => s.selectedId);
  const names = useMemo(
    () => (schema.data ? buildSchemaIndex(schema.data.schema).names : new Map<string, string>()),
    [schema.data],
  );

  const report = useQuery<ImpactReport, AppError>({
    queryKey: ["analysis", "impact", selectedId, schema.data?.fetchedAt],
    queryFn: () => ipc.analyzeImpact(selectedId as string),
    enabled: !!selectedId && !!schema.data,
  });

  const g = useMemo(() => (report.data ? graphOf(report.data, names) : null), [report.data, names]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-line bg-ink-1/90 px-4 py-2">
        <Label>IMPACT ANALYSIS</Label>
        <ObjectPicker />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {!selectedId && (
          <EmptyState title="Choose an object">
            Search for a table, view or column above, or press Impact in the Explorer, to see which
            database objects depend on it.
          </EmptyState>
        )}
        {report.isLoading && <EmptyState title="Analyzing…" />}
        {report.error && (
          <div className="p-6">
            <ErrorNote error={report.error} />
          </div>
        )}
        {report.data && (
          <div className="fade-in grid grid-cols-[340px_1fr] gap-4 p-5">
            <div className="space-y-4">
              <Panel>
                <div className="p-4">
                  <Mono className="text-[15px]">{report.data.targetName}</Mono>
                  <div className="mt-4">
                    <Label>POTENTIAL IMPACT</Label>
                  </div>
                  <div className="mt-1 flex items-center gap-3">
                    <span
                      className={cx(
                        "font-mono text-[28px] font-semibold uppercase leading-none",
                        LEVEL_TEXT[report.data.level],
                      )}
                    >
                      {report.data.level}
                    </span>
                    <Badge tone={LEVEL_TONE[report.data.level]}>{report.data.level}</Badge>
                  </div>
                  <dl className="mt-4 grid grid-cols-[1fr_auto] gap-y-1.5 font-mono text-[12px]">
                    <dt className="text-fg-muted">Direct</dt>
                    <dd className="text-right">{report.data.counts.direct}</dd>
                    <dt className="text-fg-muted">Transitive</dt>
                    <dd className="text-right">{report.data.counts.transitive}</dd>
                    <dt className="text-fg-dim">Foreign keys</dt>
                    <dd className="text-right text-fg-muted">{report.data.counts.foreignKeys}</dd>
                    <dt className="text-fg-dim">Tables</dt>
                    <dd className="text-right text-fg-muted">{report.data.counts.tables}</dd>
                    <dt className="text-fg-dim">Views</dt>
                    <dd className="text-right text-fg-muted">{report.data.counts.views}</dd>
                    <dt className="text-fg-dim">Materialized views</dt>
                    <dd className="text-right text-fg-muted">
                      {report.data.counts.materializedViews}
                    </dd>
                    <dt className="text-fg-dim">Triggers</dt>
                    <dd className="text-right text-fg-muted">{report.data.counts.triggers}</dd>
                    <dt className="text-fg-dim">Routines</dt>
                    <dd className="text-right text-fg-muted">{report.data.counts.routines}</dd>
                  </dl>
                  <p className="mt-4 text-[11px] text-fg-dim">{report.data.levelBasis}</p>
                </div>
              </Panel>
              {report.data.affectedIndexes.length > 0 && (
                <Panel title="Indexes on this column">
                  <ul className="p-3 font-mono text-[12px]">
                    {report.data.affectedIndexes.map((i) => (
                      <li key={i}>{i}</li>
                    ))}
                  </ul>
                </Panel>
              )}
              <div className="space-y-1.5 rounded-sm border border-amber/30 bg-amber-wash p-3 text-[11px] text-amber">
                {report.data.notes.map((n) => (
                  <p key={n}>{n}</p>
                ))}
              </div>
            </div>
            <div className="space-y-4">
              {g && g.nodes.length > 1 && (
                <Panel title="dependency graph">
                  <div className="relative h-72">
                    <MiniGraph
                      nodes={g.nodes}
                      edges={g.edges}
                      target={report.data.target}
                      mode="horizontal"
                    />
                    <div className="absolute bottom-2 left-2 rounded-sm border border-line bg-ink-1/95 px-2 py-1">
                      <EdgeLegend />
                    </div>
                  </div>
                </Panel>
              )}
              <ItemList title="DIRECT" items={report.data.direct} names={names} />
              <ItemList title="TRANSITIVE" items={report.data.transitive} names={names} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
