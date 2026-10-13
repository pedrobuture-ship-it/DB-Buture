import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  Badge,
  Button,
  EmptyState,
  ErrorNote,
  EvidenceBadge,
  Label,
  Mono,
  Panel,
  SeverityBadge,
  cx,
} from "@/components/ui";
import { ipc, toAppError } from "@/lib/ipc";
import type { AppError, DiffReport, MigrationReport } from "@/types/schema";
import { DiffList, DiffReportView, DiffSummary } from "./DiffList";
import { useDiffSelection } from "./diffSelection";

type Tab = "snapshots" | "migration";

function Select({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: { id: string; label: string }[];
}) {
  return (
    <label className="flex flex-col gap-1">
      <Label>{label}</Label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label={label}
        className="h-8 min-w-64 rounded-sm border border-line-strong bg-ink-2 px-2 font-mono text-[12px]"
      >
        <option value="">Choose…</option>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function Compare() {
  const { before, after, pick } = useDiffSelection();
  const snaps = useQuery({ queryKey: ["snapshots"], queryFn: ipc.listSnapshots });
  const [report, setReport] = useState<DiffReport | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const options = [
    { id: "current", label: "Current (loaded metadata)" },
    ...(snaps.data ?? []).map((s) => ({ id: s.id, label: s.name })),
  ];

  const compare = useMutation<DiffReport, AppError, undefined>({
    mutationFn: () => ipc.diffVersions(before, after),
    onSuccess: (r) => {
      setReport(r);
      setError(null);
    },
    onError: (e) => setError(toAppError(e)),
  });

  useEffect(() => {
    if (before && after) compare.mutate(undefined);
    // run automatically when arriving from "Compare" on a snapshot or when the selection changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [before, after]);

  return (
    <div className="space-y-4 p-6">
      <div className="flex items-end gap-3">
        <Select
          label="BEFORE"
          value={before}
          onChange={(v) => pick("before", v)}
          options={options}
        />
        <span className="pb-2 font-mono text-fg-dim">→</span>
        <Select label="AFTER" value={after} onChange={(v) => pick("after", v)} options={options} />
        <Button
          disabled={!before || !after || compare.isPending}
          onClick={() => compare.mutate(undefined)}
        >
          Compare
        </Button>
      </div>
      {error && <ErrorNote error={error} />}
      {!report && !error && (
        <EmptyState title="Pick two versions">
          Compare a snapshot with the current schema, or two snapshots with each other. Differences
          are matched by stable ids, so renamed objects show as removed + added.
        </EmptyState>
      )}
      {report && <DiffReportView report={report} />}
    </div>
  );
}

const STATUS_TONE = { applied: "green", ignored: "neutral", failed: "red" } as const;
const KIND_LABEL = {
  structural: "Structural risk",
  runtime: "Runtime risk",
  application: "Application risk",
} as const;
const KIND_HELP = {
  structural: "Visible in the catalog: dependent objects and keys.",
  runtime: "Heuristics about locking and rewrites; depends on data volume and engine version.",
  application: "Always unknown to DB-BUTURE: code, ORMs, reports and jobs are invisible.",
} as const;

function Migration() {
  const [ddl, setDdl] = useState("");
  const [report, setReport] = useState<MigrationReport | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const sim = useMutation<MigrationReport, AppError, undefined>({
    mutationFn: () => ipc.simulateMigration(ddl),
    onSuccess: (r) => {
      setReport(r);
      setError(null);
    },
    onError: (e) => setError(toAppError(e)),
  });
  return (
    <div className="space-y-4 p-6">
      <Panel
        title="migration DDL"
        action={<Badge tone="green">SIMULATION ONLY · never executed</Badge>}
      >
        <textarea
          value={ddl}
          onChange={(e) => setDdl(e.target.value)}
          spellCheck={false}
          aria-label="Migration DDL"
          placeholder={
            "ALTER TABLE users ADD COLUMN nickname text;\nALTER TABLE orders ALTER COLUMN total TYPE numeric(14,2);"
          }
          className="h-44 w-full resize-y bg-ink-0 p-3 font-mono text-[12px] outline-none placeholder:text-fg-dim"
        />
        <div className="border-t border-line p-2">
          <Button
            variant="primary"
            disabled={sim.isPending || !ddl.trim()}
            onClick={() => sim.mutate(undefined)}
          >
            Simulate on a copy of the model
          </Button>
        </div>
      </Panel>
      {error && <ErrorNote error={error} />}
      {report && (
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-4">
            <Panel title="statements">
              <ul>
                {report.statements.map((s, i) => (
                  <li key={i} className="border-b border-line px-3 py-2 last:border-b-0">
                    <div className="flex items-center gap-2">
                      <Badge tone={STATUS_TONE[s.status]}>{s.status}</Badge>
                      <span className="text-[12px]">{s.message}</span>
                    </div>
                    <Mono className="mt-0.5 block truncate text-fg-dim">{s.sql}</Mono>
                  </li>
                ))}
              </ul>
            </Panel>
            <Panel title="resulting changes" action={<DiffSummary diff={report.diff} />}>
              <DiffList diff={report.diff} />
            </Panel>
          </div>
          <div className="space-y-4">
            {(["structural", "runtime", "application"] as const).map((k) => {
              const items = report.risks.filter((r) => r.kind === k);
              return (
                <Panel key={k} title={`${KIND_LABEL[k]} · ${items.length}`}>
                  <p className="border-b border-line px-3 py-1.5 text-[11px] text-fg-dim">
                    {KIND_HELP[k]}
                  </p>
                  {items.length === 0 ? (
                    <p className="p-3 text-[12px] text-fg-dim">Nothing detected.</p>
                  ) : (
                    <ul>
                      {items.map((r, i) => (
                        <li
                          key={i}
                          className="flex items-start gap-2 border-b border-line px-3 py-2 last:border-b-0"
                        >
                          <SeverityBadge severity={r.severity} />
                          <EvidenceBadge evidence={r.evidence} />
                          <span className="text-[12px]">{r.message}</span>
                        </li>
                      ))}
                    </ul>
                  )}
                </Panel>
              );
            })}
            <p className="text-[11px] text-fg-dim">{report.notes.join(" ")}</p>
          </div>
        </div>
      )}
    </div>
  );
}

export function SchemaDiffView() {
  const [tab, setTab] = useState<Tab>("snapshots");
  return (
    <div className="fade-in">
      <div role="tablist" className="flex border-b border-line bg-ink-1/80 px-3">
        {(["snapshots", "migration"] as const).map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            type="button"
            onClick={() => setTab(t)}
            className={cx(
              "relative h-9 px-3 font-mono text-[11px] uppercase tracking-wider",
              tab === t ? "text-cyan" : "text-fg-dim hover:text-fg",
            )}
          >
            {t === "snapshots" ? "Compare versions" : "Migration simulator"}
            {tab === t && <span className="absolute inset-x-0 bottom-0 h-px bg-cyan" />}
          </button>
        ))}
      </div>
      {tab === "snapshots" ? <Compare /> : <Migration />}
    </div>
  );
}
