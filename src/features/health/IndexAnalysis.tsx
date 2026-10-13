import { useQuery } from "@tanstack/react-query";
import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { Button, EmptyState, ErrorNote, EvidenceBadge, Label, Mono, Panel } from "@/components/ui";
import { useSchema } from "@/hooks/queries";
import { ipc } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import type { AppError, IndexReport } from "@/types/schema";

function CopySql({ sql }: { sql: string }) {
  const [done, setDone] = useState(false);
  return (
    <Button
      onClick={() => {
        void navigator.clipboard.writeText(sql).then(() => {
          setDone(true);
          setTimeout(() => setDone(false), 1500);
        });
      }}
    >
      {done ? <Check size={12} /> : <Copy size={12} />} COPY SQL
    </Button>
  );
}

export function IndexAnalysis() {
  const schema = useSchema();
  const select = useUi((s) => s.select);
  const report = useQuery<IndexReport, AppError>({
    queryKey: ["analysis", "indexes", schema.data?.fetchedAt],
    queryFn: ipc.indexAnalysis,
    enabled: !!schema.data,
  });

  if (report.isLoading) return <EmptyState title="Analyzing indexes…" />;
  if (report.error)
    return (
      <div className="p-6">
        <ErrorNote error={report.error} />
      </div>
    );
  if (!report.data) return null;
  const r = report.data;

  return (
    <div className="fade-in space-y-4 p-6">
      <div>
        <Label>INDEX ANALYSIS</Label>
        <p className="mt-1 text-[12px] text-fg-muted">
          DB-BUTURE only reads the catalog. The SQL below is a suggestion to review and run
          yourself; nothing is executed.
        </p>
      </div>

      <Panel title={`foreign keys without a supporting index · ${r.missingFkIndexes.length}`}>
        {r.missingFkIndexes.length === 0 ? (
          <p className="p-3 text-[12px] text-fg-dim">
            Every foreign key has an index that starts with its columns.
          </p>
        ) : (
          <ul>
            {r.missingFkIndexes.map((g) => (
              <li
                key={g.foreignKey}
                className="flex items-center gap-3 border-b border-line px-3 py-2 last:border-b-0"
              >
                <EvidenceBadge evidence={g.evidence} />
                <button
                  type="button"
                  className="font-mono text-[12px] text-cyan hover:underline"
                  onClick={() => select(g.table, "explorer")}
                >
                  {g.tableName}
                </button>
                <Mono className="text-fg-muted">
                  {g.foreignKeyName} ({g.columns.join(", ")})
                </Mono>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title={`duplicate and overlapping indexes · ${r.redundant.length}`}>
        {r.redundant.length === 0 ? (
          <p className="p-3 text-[12px] text-fg-dim">
            No duplicate or overlapping indexes detected.
          </p>
        ) : (
          <ul>
            {r.redundant.map((x) => (
              <li
                key={`${x.index}:${x.coveredBy}`}
                className="flex items-center gap-3 border-b border-line px-3 py-2 last:border-b-0"
              >
                <EvidenceBadge evidence={x.evidence} />
                <span className="font-mono text-[10px] uppercase text-fg-dim">{x.kind}</span>
                <Mono>{x.explanation}</Mono>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title={`suggested SQL · ${r.suggestions.length}`}>
        {r.suggestions.length === 0 ? (
          <p className="p-3 text-[12px] text-fg-dim">No suggestions.</p>
        ) : (
          <ul>
            {r.suggestions.map((s, i) => (
              <li key={`${s.table}:${i}`} className="border-b border-line p-3 last:border-b-0">
                <div className="flex items-center gap-2">
                  <EvidenceBadge evidence={s.evidence} />
                  <span className="text-[12px]">{s.title}</span>
                  <span className="ml-auto">
                    <CopySql sql={s.sql} />
                  </span>
                </div>
                <pre className="mt-2 overflow-auto rounded-sm border border-line bg-ink-0 p-2.5 font-mono text-[12px] text-cyan">
                  {s.sql}
                </pre>
                <p className="mt-1.5 text-[11px] text-fg-dim">{s.rationale}</p>
              </li>
            ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}
