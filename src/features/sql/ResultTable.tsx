import { VirtualList } from "@/components/VirtualList";
import { Badge } from "@/components/ui";
import { formatCount } from "@/lib/format";
import type { QueryResult } from "@/types/schema";

export function formatCell(v: unknown): string {
  if (v === null || v === undefined) return "NULL";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

const ROW_H = 24;
const COL_W = 180;

export function ResultTable({ result }: { result: QueryResult }) {
  const width = Math.max(result.columns.length * COL_W + 56, 400);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-3 border-b border-line bg-ink-1 px-3 py-1.5 font-mono text-[11px] text-fg-muted">
        <span>{formatCount(result.rowCount)} rows</span>
        <span>{result.elapsedMs} ms</span>
        {result.truncated && (
          <Badge tone="amber">limited to {formatCount(result.limit)} rows</Badge>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-x-auto">
        <div style={{ width }} className="flex h-full flex-col">
          <div
            className="flex border-b border-line bg-ink-2 font-mono text-[11px]"
            style={{ height: ROW_H }}
          >
            <div className="w-14 shrink-0 px-2 leading-6 text-fg-dim">#</div>
            {result.columns.map((c, i) => (
              <div
                key={i}
                className="shrink-0 truncate px-2 leading-6"
                style={{ width: COL_W }}
                title={`${c.name} · ${c.typeName}`}
              >
                {c.name} <span className="text-fg-dim">{c.typeName.toLowerCase()}</span>
              </div>
            ))}
          </div>
          <VirtualList
            className="min-h-0 flex-1"
            items={result.rows}
            rowHeight={ROW_H}
            render={(row, i) => (
              <div
                className="flex border-b border-line/50 font-mono text-[12px] hover:bg-ink-2"
                style={{ height: ROW_H }}
              >
                <div className="w-14 shrink-0 px-2 leading-6 text-fg-dim">{i + 1}</div>
                {row.map((cell, j) => (
                  <div
                    key={j}
                    className={
                      cell === null
                        ? "shrink-0 truncate px-2 leading-6 text-fg-dim italic"
                        : "shrink-0 truncate px-2 leading-6"
                    }
                    style={{ width: COL_W }}
                    title={formatCell(cell)}
                  >
                    {formatCell(cell)}
                  </div>
                ))}
              </div>
            )}
          />
        </div>
      </div>
    </div>
  );
}
