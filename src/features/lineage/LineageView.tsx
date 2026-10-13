import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { Badge, EmptyState, ErrorNote, Label, Mono, Panel, cx } from "@/components/ui";
import { useSchema } from "@/hooks/queries";
import { ipc } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import type { AppError, ColumnClass, LineageReport } from "@/types/schema";

const CLASS_TONE: Record<ColumnClass, "green" | "cyan" | "amber" | "neutral"> = {
  DIRECT: "green",
  DERIVED: "cyan",
  AGGREGATED: "amber",
  UNKNOWN: "neutral",
};

const CLASS_HELP: Record<ColumnClass, string> = {
  DIRECT: "copied from one source column",
  DERIVED: "computed from source columns",
  AGGREGATED: "aggregates source columns",
  UNKNOWN: "origin cannot be established from evidence",
};

export function ConfidenceBadge({ confidence }: { confidence: "complete" | "partial" }) {
  return (
    <Badge tone={confidence === "complete" ? "green" : "amber"}>confidence: {confidence}</Badge>
  );
}

export function LineageView() {
  const schema = useSchema();
  const selectedId = useUi((s) => s.selectedId);
  const select = useUi((s) => s.select);
  const views = useMemo(() => schema.data?.schema.views ?? [], [schema.data]);
  const [pickedId, setPickedId] = useState<string | null>(null);
  const viewId = useMemo(() => {
    const candidate = pickedId ?? selectedId;
    return views.find((v) => v.stableId === candidate)?.stableId ?? views[0]?.stableId ?? null;
  }, [pickedId, selectedId, views]);

  const report = useQuery<LineageReport, AppError>({
    queryKey: ["analysis", "lineage", viewId, schema.data?.fetchedAt],
    queryFn: () => ipc.viewLineage(viewId as string),
    enabled: !!viewId,
  });

  if (schema.isLoading) return <EmptyState title="Reading catalog…" />;
  if (views.length === 0) {
    return (
      <EmptyState title="No views to trace">
        Data lineage is derived from view definitions, and this database has no views in the visible
        schemas.
      </EmptyState>
    );
  }

  return (
    <div className="flex h-full">
      <aside className="w-72 shrink-0 overflow-auto border-r border-line bg-ink-1/80">
        <div className="border-b border-line px-3 py-2">
          <Label>views · {views.length}</Label>
        </div>
        <ul>
          {views.map((v) => (
            <li key={v.stableId}>
              <button
                type="button"
                onClick={() => setPickedId(v.stableId)}
                className={cx(
                  "flex w-full items-center gap-2 px-3 py-1.5 text-left font-mono text-[12px] hover:bg-ink-3",
                  v.stableId === viewId && "bg-cyan-wash",
                )}
              >
                <span className="truncate">
                  {v.namespace}.{v.name}
                </span>
                {v.materialized && <Badge>mat</Badge>}
              </button>
            </li>
          ))}
        </ul>
      </aside>
      <section className="min-w-0 flex-1 overflow-auto p-6">
        {report.isLoading && <EmptyState title="Tracing…" />}
        {report.error && <ErrorNote error={report.error} />}
        {report.data && (
          <div className="fade-in space-y-4">
            <div>
              <Label>DATA LINEAGE</Label>
              <h2 className="mt-1 font-mono text-[16px]">{report.data.tables.viewName}</h2>
            </div>

            <Panel
              title="table lineage"
              action={<ConfidenceBadge confidence={report.data.tables.confidence} />}
            >
              {report.data.tables.sources.length === 0 ? (
                <p className="p-3 text-[12px] text-fg-dim">No sources were found for this view.</p>
              ) : (
                <ul>
                  {report.data.tables.sources.map((s) => (
                    <li
                      key={s.id}
                      className="flex items-center gap-3 border-b border-line px-3 py-2 last:border-b-0"
                    >
                      <button
                        type="button"
                        className="font-mono text-[12px] text-cyan hover:underline"
                        onClick={() => select(s.id, "explorer")}
                      >
                        {s.name}
                      </button>
                      <Badge tone={s.origin === "both" ? "green" : "amber"}>
                        {s.origin === "both"
                          ? "catalog + parser"
                          : s.origin === "catalog"
                            ? "catalog only"
                            : "parser only"}
                      </Badge>
                    </li>
                  ))}
                </ul>
              )}
              {report.data.tables.notes.length > 0 && (
                <ul className="border-t border-line bg-amber-wash px-3 py-2 text-[11px] text-amber">
                  {report.data.tables.notes.map((n) => (
                    <li key={n}>{n}</li>
                  ))}
                </ul>
              )}
            </Panel>

            <Panel
              title="column lineage"
              action={<ConfidenceBadge confidence={report.data.columns.confidence} />}
            >
              <table className="w-full text-[12px]">
                <thead>
                  <tr className="border-b border-line text-left">
                    <th className="px-3 py-1.5 font-normal">
                      <Label>column</Label>
                    </th>
                    <th className="px-3 py-1.5 font-normal">
                      <Label>class</Label>
                    </th>
                    <th className="px-3 py-1.5 font-normal">
                      <Label>sources</Label>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {report.data.columns.columns.map((c, i) => (
                    <tr key={`${c.column}:${i}`} className="border-b border-line/60 align-top">
                      <td className="px-3 py-1.5 font-mono">{c.column}</td>
                      <td className="px-3 py-1.5">
                        <span title={CLASS_HELP[c.class]}>
                          <Badge tone={CLASS_TONE[c.class]}>{c.class}</Badge>
                        </span>
                      </td>
                      <td className="px-3 py-1.5 font-mono text-[11px]">
                        {c.sources.map((s) => (
                          <div key={`${s.table}.${s.column}`} className="text-fg-muted">
                            {s.tableName}.{s.column}
                          </div>
                        ))}
                        {c.expression && (
                          <Mono className="block truncate text-fg-dim">{c.expression}</Mono>
                        )}
                        {c.note && <div className="text-amber">{c.note}</div>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {report.data.columns.notes.length > 0 && (
                <ul className="border-t border-line bg-amber-wash px-3 py-2 text-[11px] text-amber">
                  {report.data.columns.notes.map((n) => (
                    <li key={n}>{n}</li>
                  ))}
                </ul>
              )}
            </Panel>
          </div>
        )}
      </section>
    </div>
  );
}
