import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { Button, EmptyState, ErrorNote, Label, Mono } from "@/components/ui";
import { useSavedConnections, useStatus } from "@/hooks/queries";
import { engineLabel } from "@/lib/format";
import { ipc, toAppError } from "@/lib/ipc";
import type { AppError, DiffReport } from "@/types/schema";
import { DiffReportView } from "./DiffList";
import { NeedsConnection } from "@/features/sql/SqlEditorView";

export function Environments() {
  const status = useStatus();
  const saved = useSavedConnections();
  const [target, setTarget] = useState("");
  const [report, setReport] = useState<DiffReport | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const compare = useMutation<DiffReport, AppError, undefined>({
    mutationFn: () => ipc.compareEnvironment(target),
    onSuccess: (r) => {
      setReport(r);
      setError(null);
    },
    onError: (e) => {
      setError(toAppError(e));
      setReport(null);
    },
  });

  if (status.data?.imported) return <NeedsConnection what="Environment comparison" />;
  const candidates = (saved.data ?? []).filter((c) => c.id !== status.data?.connectionId);

  return (
    <div className="fade-in space-y-4 p-6">
      <div>
        <Label>ENVIRONMENTS</Label>
        <p className="mt-1 max-w-2xl text-[12px] text-fg-muted">
          Compare the structure of the connected database ({status.data?.connectionName}) with
          another saved connection, for example staging against production. Only schema metadata is
          read; no table data is ever compared or transferred.
        </p>
      </div>
      {candidates.length === 0 ? (
        <EmptyState title="No other saved connections">
          Save a second connection from the connect screen (use Disconnect, fill the form and Save
          Connection) to compare environments.
        </EmptyState>
      ) : (
        <div className="flex items-end gap-3">
          <label className="flex flex-col gap-1">
            <Label>compare against</Label>
            <select
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              aria-label="Environment to compare against"
              className="h-8 min-w-72 rounded-sm border border-line-strong bg-ink-2 px-2 font-mono text-[12px]"
            >
              <option value="">Choose a saved connection…</option>
              {candidates.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name} ({engineLabel[c.config.engine]})
                </option>
              ))}
            </select>
          </label>
          <Button
            variant="primary"
            disabled={!target || compare.isPending}
            onClick={() => compare.mutate(undefined)}
          >
            {compare.isPending ? "Comparing…" : "Compare structure"}
          </Button>
        </div>
      )}
      {error && <ErrorNote error={error} />}
      {report && (
        <>
          <Mono className="block text-fg-dim">
            Drift = differences between the two environments. “before” is the connected database,
            “after” is the other one.
          </Mono>
          <DiffReportView report={report} />
        </>
      )}
    </div>
  );
}
