import { Activity, GitFork, Network } from "lucide-react";
import { Badge, Button, EmptyState, ErrorNote, Label, Mono, Panel, Stat } from "@/components/ui";
import { useSchema } from "@/hooks/queries";
import { engineLabel, formatBytes, formatCount } from "@/lib/format";
import { useUi } from "@/stores/ui";
import type { DatabaseSchema } from "@/types/schema";

export function summarize(s: DatabaseSchema) {
  const userNamespaces = s.namespaces.filter((n) => !n.isSystem).length;
  return {
    schemas: userNamespaces,
    tables: s.tables.length,
    views: s.views.length,
    columns: s.tables.reduce((n, t) => n + t.columns.length, 0),
    foreignKeys: s.tables.reduce((n, t) => n + t.foreignKeys.length, 0),
    indexes: s.tables.reduce((n, t) => n + t.indexes.length, 0),
    triggers: s.triggers.length,
    sizeBytes: s.tables.every((t) => t.sizeBytes == null)
      ? null
      : s.tables.reduce((n, t) => n + (t.sizeBytes ?? 0), 0),
    largest: [...s.tables]
      .filter((t) => t.sizeBytes != null)
      .sort((a, b) => (b.sizeBytes ?? 0) - (a.sizeBytes ?? 0))
      .slice(0, 6),
  };
}

export function Overview() {
  const { data, error, isLoading } = useSchema();
  const select = useUi((s) => s.select);
  const setView = useUi((s) => s.setView);

  if (isLoading) return <EmptyState title="Reading catalog…" />;
  if (error)
    return (
      <div className="p-6">
        <ErrorNote error={error as never} />
      </div>
    );
  if (!data) return null;

  const s = data.schema;
  const m = summarize(s);
  const stats: [string, string | number, string?][] = [
    ["Schemas", m.schemas],
    ["Tables", formatCount(m.tables)],
    ["Views", formatCount(m.views)],
    ["Columns", formatCount(m.columns)],
    ["Foreign Keys", formatCount(m.foreignKeys)],
    ["Indexes", formatCount(m.indexes)],
    ["Triggers", formatCount(m.triggers)],
  ];

  return (
    <div className="fade-in flex flex-col gap-4 p-6">
      <div>
        <Label>DATABASE OVERVIEW</Label>
        <h1 className="mt-1 text-xl font-semibold tracking-tight">{s.database}</h1>
        <div className="mt-1 flex items-center gap-3 text-[12px] text-fg-muted">
          <Mono>{engineLabel[s.engine]}</Mono>
          <Mono>{s.serverVersion?.split(" ")[0]}</Mono>
          <Badge tone="green">READ ONLY</Badge>
        </div>
      </div>

      <Panel>
        <div className="grid grid-cols-4 divide-y divide-line lg:grid-cols-7 lg:divide-y-0">
          {stats.map(([label, value]) => (
            <Stat key={label} label={label} value={value} />
          ))}
        </div>
      </Panel>

      {s.warnings.length > 0 && (
        <Panel title={`${s.warnings.length} introspection warnings`}>
          <ul className="space-y-1 p-3 font-mono text-[11px] text-amber">
            {s.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </Panel>
      )}

      <div className="grid grid-cols-3 gap-4">
        <Panel title="Largest tables" className="col-span-2">
          {m.largest.length === 0 ? (
            <p className="p-4 text-[12px] text-fg-dim">This engine does not report table sizes.</p>
          ) : (
            <table className="w-full text-[12px]">
              <thead>
                <tr className="text-left">
                  <th className="px-3 py-1.5">
                    <Label>table</Label>
                  </th>
                  <th className="px-3 py-1.5 text-right">
                    <Label>est. rows</Label>
                  </th>
                  <th className="px-3 py-1.5 text-right">
                    <Label>size</Label>
                  </th>
                </tr>
              </thead>
              <tbody>
                {m.largest.map((t) => (
                  <tr
                    key={t.stableId}
                    className="cursor-pointer border-t border-line hover:bg-ink-2"
                    onClick={() => select(t.stableId, "explorer")}
                  >
                    <td className="px-3 py-1.5 font-mono">
                      {t.namespace}.{t.name}
                    </td>
                    <td className="px-3 py-1.5 text-right font-mono text-fg-muted">
                      {formatCount(t.estimatedRows)}
                    </td>
                    <td className="px-3 py-1.5 text-right font-mono">{formatBytes(t.sizeBytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {m.sizeBytes != null && (
            <div className="border-t border-line px-3 py-2 text-[11px] text-fg-dim">
              Total table + index size: <Mono className="text-fg">{formatBytes(m.sizeBytes)}</Mono>.
              Row counts are catalog estimates.
            </div>
          )}
        </Panel>

        <Panel title="Architecture score">
          <div className="p-4">
            <div className="font-mono text-[22px] text-fg-dim">—</div>
            <p className="mt-2 text-[12px] text-fg-dim">
              Not scored here. The explainable health score lives in Health and every point traces
              back to a rule.
            </p>
            <Button className="mt-3" onClick={() => setView("health")}>
              <Activity size={13} /> Open Health
            </Button>
          </div>
        </Panel>
      </div>

      <div className="flex gap-2">
        <Button onClick={() => setView("erd")}>
          <Network size={13} /> ER Diagram
        </Button>
        <Button onClick={() => setView("dependencies")}>
          <GitFork size={13} /> Dependencies
        </Button>
      </div>
    </div>
  );
}
