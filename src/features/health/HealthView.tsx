import { useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import {
  EmptyState,
  ErrorNote,
  EvidenceBadge,
  Label,
  Mono,
  Panel,
  SeverityBadge,
  cx,
} from "@/components/ui";
import { useSchema } from "@/hooks/queries";
import { ipc } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import type { AppError, AuditCategory, AuditReport, Finding } from "@/types/schema";

const CATEGORY_LABEL: Record<AuditCategory, string> = {
  integrity: "Integrity",
  performance: "Performance",
  maintainability: "Maintainability",
  complexity: "Complexity",
  documentation: "Documentation",
};

function scoreColor(score: number): string {
  if (score >= 85) return "text-green";
  if (score >= 60) return "text-amber";
  return "text-red";
}

function barColor(score: number): string {
  if (score >= 85) return "bg-green";
  if (score >= 60) return "bg-amber";
  return "bg-red";
}

export function useAudit(enabled = true) {
  const schema = useSchema();
  return useQuery<AuditReport, AppError>({
    queryKey: ["analysis", "audit", schema.data?.fetchedAt],
    queryFn: ipc.runAudit,
    enabled: enabled && !!schema.data,
  });
}

function FindingRow({ f }: { f: Finding }) {
  const select = useUi((s) => s.select);
  return (
    <li className="flex items-center gap-2.5 border-b border-line px-3 py-1.5 last:border-b-0">
      <SeverityBadge severity={f.severity} />
      <EvidenceBadge evidence={f.evidence} />
      <span className="whitespace-nowrap font-mono text-[10px] text-fg-dim">{f.ruleId}</span>
      <button
        type="button"
        onClick={() => select(f.object, "explorer")}
        className="truncate text-left text-[12px] hover:text-cyan"
      >
        {f.message}
      </button>
    </li>
  );
}

export function HealthView() {
  const audit = useAudit();
  const [open, setOpen] = useState<string | null>(null);

  if (audit.isLoading) return <EmptyState title="Running audit rules…" />;
  if (audit.error)
    return (
      <div className="p-6">
        <ErrorNote error={audit.error} />
      </div>
    );
  if (!audit.data) return null;
  const { health, findings, rules } = audit.data;

  if (health.overall == null) {
    return (
      <EmptyState title="Nothing to score">
        There are no tables in the visible schemas, so no health score is computed.
      </EmptyState>
    );
  }

  const facts = findings.filter((f) => f.evidence === "observed_fact");
  const heuristics = findings.filter((f) => f.evidence === "heuristic");

  return (
    <div className="fade-in space-y-4 p-6">
      <div>
        <Label>DATABASE HEALTH</Label>
        <p className="mt-1 text-[12px] text-fg-muted">
          An explainable score: every deducted point links to a rule and an object.
        </p>
      </div>

      <div className="grid grid-cols-[240px_1fr] gap-4">
        <Panel>
          <div className="p-4">
            <Label>overall</Label>
            <div
              className={cx(
                "mt-1 font-mono text-[44px] font-semibold leading-none",
                scoreColor(health.overall),
              )}
            >
              {health.overall.toFixed(0)}
            </div>
            <div className="mt-1 font-mono text-[10px] text-fg-dim">out of 100</div>
            <p className="mt-4 text-[11px] leading-relaxed text-fg-dim">{health.method}</p>
          </div>
        </Panel>
        <Panel title="categories">
          <ul>
            {health.categories.map((c) => (
              <li key={c.category} className="border-b border-line last:border-b-0">
                <button
                  type="button"
                  onClick={() => setOpen(open === c.category ? null : c.category)}
                  className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-ink-2"
                  aria-expanded={open === c.category}
                >
                  {open === c.category ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                  <span className="w-36 text-[12px]">{CATEGORY_LABEL[c.category]}</span>
                  <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-ink-4">
                    <span
                      className={cx("block h-full", barColor(c.score))}
                      style={{ width: `${c.score}%` }}
                    />
                  </span>
                  <span
                    className={cx("w-12 text-right font-mono text-[13px]", scoreColor(c.score))}
                  >
                    {c.score.toFixed(0)}
                  </span>
                  <span className="w-20 text-right font-mono text-[10px] text-fg-dim">
                    −{c.penaltyTotal} pts
                  </span>
                </button>
                {open === c.category && (
                  <ul className="border-t border-line bg-ink-0/60 px-9 py-2 font-mono text-[11px] text-fg-muted">
                    {c.penalties.length === 0 && <li>No penalties.</li>}
                    {c.penalties.map((p, i) => (
                      <li key={`${p.ruleId}:${p.object}:${i}`} className="flex gap-3 py-0.5">
                        <span className="w-10 text-red">−{p.points}</span>
                        <span className="text-fg-dim">{p.ruleId}</span>
                        <span>{p.objectName}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ul>
        </Panel>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <Panel title={`observed facts · ${facts.length}`}>
          {facts.length === 0 ? (
            <p className="p-3 text-[12px] text-fg-dim">No observed issues.</p>
          ) : (
            <ul>
              {facts.map((f, i) => (
                <FindingRow key={`${f.ruleId}:${f.object}:${i}`} f={f} />
              ))}
            </ul>
          )}
        </Panel>
        <Panel title={`heuristics · ${heuristics.length}`}>
          {heuristics.length === 0 ? (
            <p className="p-3 text-[12px] text-fg-dim">No heuristic findings.</p>
          ) : (
            <ul>
              {heuristics.map((f, i) => (
                <FindingRow key={`${f.ruleId}:${f.object}:${i}`} f={f} />
              ))}
            </ul>
          )}
        </Panel>
      </div>

      <Panel title={`rules · ${rules.length}`}>
        <ul>
          {rules.map((r) => (
            <li
              key={r.id}
              className="flex items-start gap-3 border-b border-line px-3 py-2 last:border-b-0"
            >
              <SeverityBadge severity={r.severity} />
              <EvidenceBadge evidence={r.evidence} />
              <div>
                <Mono className="text-fg">{r.title}</Mono>{" "}
                <span className="font-mono text-[10px] text-fg-dim">{r.id}</span>
                <p className="text-[11px] text-fg-muted">{r.description}</p>
              </div>
            </li>
          ))}
        </ul>
      </Panel>
    </div>
  );
}
