import { Route } from "lucide-react";
import { Button, EmptyState, ErrorNote } from "@/components/ui";
import { SqlEditor } from "@/components/SqlEditor";
import { useSchema, useStatus } from "@/hooks/queries";
import { useUi } from "@/stores/ui";
import { NeedsConnection, useQueryRunner } from "./SqlEditorView";
import { PlanView } from "./PlanView";

export function QueryPlans() {
  const status = useStatus();
  const schema = useSchema();
  const sqlText = useUi((s) => s.sqlText);
  const setSqlText = useUi((s) => s.setSqlText);
  const q = useQueryRunner();
  if (status.data?.imported) return <NeedsConnection what="Query plans" />;
  const doExplain = () => sqlText.trim() && q.explain.mutate(sqlText);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-line bg-ink-1/90 px-3 py-2">
        <Button
          variant="primary"
          disabled={q.explain.isPending || !sqlText.trim()}
          onClick={doExplain}
        >
          <Route size={12} /> Explain
        </Button>
        <span className="font-mono text-[11px] text-fg-dim">
          EXPLAIN only: the query is never executed and ANALYZE is never used.
        </span>
      </div>
      <div className="h-40 border-b border-line">
        <SqlEditor
          value={sqlText}
          onChange={setSqlText}
          onRun={doExplain}
          schema={schema.data?.schema}
          engine={status.data?.engine}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {q.explain.isPending && <EmptyState title="Planning…" />}
        {q.error && (
          <div className="p-4">
            <ErrorNote error={q.error} />
          </div>
        )}
        {q.plan && !q.error && <PlanView plan={q.plan} />}
        {!q.plan && !q.error && !q.explain.isPending && (
          <EmptyState title="No plan yet">
            Enter a SELECT statement and press Explain to see how the engine intends to run it.
          </EmptyState>
        )}
      </div>
    </div>
  );
}
