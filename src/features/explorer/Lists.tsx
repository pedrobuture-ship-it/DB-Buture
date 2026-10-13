import { useMemo, useState } from "react";
import { EmptyState, ErrorNote, Label } from "@/components/ui";
import { useSchema } from "@/hooks/queries";
import { formatBytes, formatCount } from "@/lib/format";
import { fuzzyFilter } from "@/lib/fuzzy";
import { indexSignature } from "@/lib/schemaIndex";
import { useUi } from "@/stores/ui";
import type { AppError } from "@/types/schema";

type Kind = "tables" | "views" | "indexes";

function Frame({
  title,
  count,
  query,
  setQuery,
  children,
}: {
  title: string;
  count: number;
  query: string;
  setQuery: (q: string) => void;
  children: React.ReactNode;
}) {
  return (
    <div className="fade-in p-6">
      <div className="mb-3 flex items-center gap-3">
        <Label>{title}</Label>
        <span className="font-mono text-[11px] text-fg-dim">{count}</span>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter…"
          aria-label={`Filter ${title}`}
          className="ml-auto h-7 w-64 rounded-sm border border-line-strong bg-ink-2 px-2 font-mono text-[12px] placeholder:text-fg-dim focus:border-cyan focus:outline-none"
        />
      </div>
      <div className="overflow-hidden rounded-md border border-line bg-ink-1">{children}</div>
    </div>
  );
}

const th = "px-3 py-1.5 text-left font-normal";

export function ObjectList({ kind }: { kind: Kind }) {
  const { data, error, isLoading } = useSchema();
  const select = useUi((s) => s.select);
  const [query, setQuery] = useState("");

  const rows = useMemo(() => {
    if (!data) return [];
    const s = data.schema;
    if (kind === "tables")
      return fuzzyFilter(s.tables, query, (t) => t.name).map((t) => ({
        key: t.stableId,
        id: t.stableId,
        cells: [
          `${t.namespace}.${t.name}`,
          String(t.columns.length),
          formatCount(t.estimatedRows),
          formatBytes(t.sizeBytes),
          t.comment ?? "",
        ],
      }));
    if (kind === "views")
      return fuzzyFilter(s.views, query, (v) => v.name).map((v) => ({
        key: v.stableId,
        id: v.stableId,
        cells: [
          `${v.namespace}.${v.name}`,
          v.materialized ? "materialized" : "view",
          String(v.columns.length),
          v.comment ?? "",
        ],
      }));
    return s.tables
      .flatMap((t) => t.indexes.map((i) => ({ t, i })))
      .filter(({ i }) => query.trim() === "" || fuzzyFilter([i], query, (x) => x.name).length > 0)
      .map(({ t, i }) => ({
        key: i.stableId,
        id: t.stableId,
        cells: [i.name, `${t.namespace}.${t.name}`, indexSignature(i), i.predicate ?? ""],
      }));
  }, [data, kind, query]);

  if (isLoading) return <EmptyState title="Reading catalog…" />;
  if (error)
    return (
      <div className="p-6">
        <ErrorNote error={error as unknown as AppError} />
      </div>
    );

  const heads: Record<Kind, string[]> = {
    tables: ["table", "columns", "est. rows", "size", "comment"],
    views: ["view", "type", "columns", "comment"],
    indexes: ["index", "table", "definition", "predicate"],
  };

  return (
    <Frame title={kind} count={rows.length} query={query} setQuery={setQuery}>
      {rows.length === 0 ? (
        <EmptyState title={`No ${kind}`}>
          {query
            ? `Nothing matches “${query}”.`
            : `This database has no ${kind} in the visible schemas.`}
        </EmptyState>
      ) : (
        <table className="w-full text-[12px]">
          <thead>
            <tr className="border-b border-line">
              {heads[kind].map((h) => (
                <th key={h} className={th}>
                  <Label>{h}</Label>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr
                key={r.key}
                className="cursor-pointer border-b border-line/60 hover:bg-ink-2"
                onClick={() => select(r.id, "explorer")}
              >
                {r.cells.map((c, i) => (
                  <td
                    key={i}
                    className={
                      i === 0 ? "px-3 py-1.5 font-mono" : "px-3 py-1.5 font-mono text-fg-muted"
                    }
                  >
                    {c}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Frame>
  );
}
