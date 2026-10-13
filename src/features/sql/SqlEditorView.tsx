import { useMutation } from "@tanstack/react-query";
import { Ban, Play, Route, WandSparkles } from "lucide-react";
import { useRef, useState } from "react";
import { format as formatSql } from "sql-formatter";
import { Button, EmptyState, ErrorNote, Label, cx } from "@/components/ui";
import { SqlEditor } from "@/components/SqlEditor";
import { useSchema, useStatus } from "@/hooks/queries";
import { ipc, toAppError } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import type { AppError, Engine, QueryPlan, QueryResult } from "@/types/schema";
import { PlanView } from "./PlanView";
import { ResultTable } from "./ResultTable";

const FORMAT_LANG: Record<Engine, "postgresql" | "mysql" | "sqlite" | "sql"> = {
  postgres: "postgresql",
  mysql: "mysql",
  sqlite: "sqlite",
  generic: "sql",
};

export function formatQuery(sql: string, engine: Engine): string {
  try {
    return formatSql(sql, { language: FORMAT_LANG[engine], keywordCase: "upper", tabWidth: 2 });
  } catch {
    return sql; // unparsable SQL is left as typed
  }
}

export function NeedsConnection({ what }: { what: string }) {
  return (
    <EmptyState title="Live connection required">
      {what} runs against a database. This workspace was imported from SQL, so there is nothing to
      connect to.
    </EmptyState>
  );
}

/** Shared by the SQL Editor and Query Plans views. */
export function useQueryRunner() {
  const [result, setResult] = useState<QueryResult | null>(null);
  const [plan, setPlan] = useState<QueryPlan | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const runId = useRef<string | null>(null);

  const run = useMutation<QueryResult, AppError, { sql: string; limit: number; timeoutMs: number }>(
    {
      mutationFn: async ({ sql, limit, timeoutMs }) => {
        const id = crypto.randomUUID();
        runId.current = id;
        try {
          return await ipc.runQuery(id, sql, limit, timeoutMs);
        } finally {
          runId.current = null;
        }
      },
      onMutate: () => setError(null),
      onSuccess: (r) => {
        setResult(r);
        setPlan(null);
      },
      onError: (e) => setError(toAppError(e)),
    },
  );

  const explain = useMutation<QueryPlan, AppError, string>({
    mutationFn: (sql) => ipc.explainQuery(sql),
    onMutate: () => setError(null),
    onSuccess: (p) => {
      setPlan(p);
      setResult(null);
    },
    onError: (e) => setError(toAppError(e)),
  });

  const cancel = () => {
    if (runId.current) void ipc.cancelQuery(runId.current);
  };

  return { result, plan, error, run, explain, cancel };
}

export function SqlEditorView() {
  const status = useStatus();
  const schema = useSchema();
  const sqlText = useUi((s) => s.sqlText);
  const setSqlText = useUi((s) => s.setSqlText);
  const [limit, setLimit] = useState(500);
  const [timeoutS, setTimeoutS] = useState(30);
  const q = useQueryRunner();

  if (status.data?.imported) return <NeedsConnection what="The SQL editor" />;
  const engine = status.data?.engine ?? "generic";
  const busy = q.run.isPending || q.explain.isPending;
  const doRun = () =>
    !busy && sqlText.trim() && q.run.mutate({ sql: sqlText, limit, timeoutMs: timeoutS * 1000 });

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-line bg-ink-1/90 px-3 py-2">
        <Button variant="primary" disabled={busy || !sqlText.trim()} onClick={doRun}>
          <Play size={12} /> Run
        </Button>
        <Button disabled={busy || !sqlText.trim()} onClick={() => q.explain.mutate(sqlText)}>
          <Route size={12} /> Explain
        </Button>
        <Button disabled={!sqlText.trim()} onClick={() => setSqlText(formatQuery(sqlText, engine))}>
          <WandSparkles size={12} /> Format
        </Button>
        <Button variant="danger" disabled={!q.run.isPending} onClick={q.cancel}>
          <Ban size={12} /> Cancel
        </Button>
        <span className="ml-auto flex items-center gap-3 font-mono text-[11px] text-fg-muted">
          <label className="flex items-center gap-1.5">
            limit
            <select
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
              className="h-6 rounded-sm border border-line-strong bg-ink-2 px-1"
              aria-label="Row limit"
            >
              {[100, 500, 1000, 5000].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-1.5">
            timeout
            <select
              value={timeoutS}
              onChange={(e) => setTimeoutS(Number(e.target.value))}
              className="h-6 rounded-sm border border-line-strong bg-ink-2 px-1"
              aria-label="Timeout"
            >
              {[5, 15, 30, 120].map((n) => (
                <option key={n} value={n}>
                  {n}s
                </option>
              ))}
            </select>
          </label>
          <span className="rounded-sm border border-green/40 bg-green-wash px-1.5 py-px text-green">
            READ ONLY
          </span>
        </span>
      </div>
      <div className="h-[38%] min-h-32 border-b border-line">
        <SqlEditor
          value={sqlText}
          onChange={setSqlText}
          onRun={doRun}
          schema={schema.data?.schema}
          engine={engine}
        />
      </div>
      <div className={cx("min-h-0 flex-1 overflow-auto")}>
        {busy && <EmptyState title={q.explain.isPending ? "Planning…" : "Running…"} />}
        {q.error && !busy && (
          <div className="p-4">
            <ErrorNote error={q.error} />
          </div>
        )}
        {!busy && !q.error && q.result && <ResultTable result={q.result} />}
        {!busy && !q.error && q.plan && <PlanView plan={q.plan} />}
        {!busy && !q.error && !q.result && !q.plan && (
          <EmptyState title="No results yet">
            Write a SELECT and press <Label className="mx-1">Ctrl+Enter</Label>. Anything that would
            modify data is blocked, and the connection itself is read-only.
          </EmptyState>
        )}
      </div>
    </div>
  );
}
