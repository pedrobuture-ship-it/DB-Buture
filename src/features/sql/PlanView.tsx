import { useMemo } from "react";
import { Badge, EvidenceBadge, Label, Mono, Panel, SeverityBadge, cx } from "@/components/ui";
import { formatCount } from "@/lib/format";
import type { PlanNode, QueryPlan } from "@/types/schema";

function maxCost(n: PlanNode): number {
  return Math.max(n.estimatedCost ?? 0, ...n.children.map(maxCost));
}

function Node({
  node,
  max,
  depth,
  hot,
}: {
  node: PlanNode;
  max: number;
  depth: number;
  hot: Set<number>;
}) {
  const share =
    max > 0 && node.estimatedCost != null
      ? Math.max(2, Math.round((node.estimatedCost / max) * 100))
      : 0;
  return (
    <li>
      <div
        className={cx(
          "relative my-1 rounded-sm border bg-ink-1 px-2.5 py-1.5",
          hot.has(node.id) ? "border-amber/50" : "border-line",
        )}
        style={{ marginLeft: depth * 18 }}
        data-testid="plan-node"
      >
        <div className="flex items-center gap-2">
          <span className="font-mono text-[12px] font-medium text-fg">{node.nodeType}</span>
          {node.relation && <Mono className="text-cyan">{node.relation}</Mono>}
          {node.index && <Badge tone="green">index {node.index}</Badge>}
          <span className="ml-auto flex gap-3 font-mono text-[10px] text-fg-muted">
            {node.estimatedCost != null && <span>cost {node.estimatedCost.toFixed(2)}</span>}
            {node.estimatedRows != null && (
              <span>rows ≈ {formatCount(Math.round(node.estimatedRows))}</span>
            )}
          </span>
        </div>
        {node.filter && (
          <Mono className="mt-0.5 block truncate text-[11px] text-fg-dim">
            filter: {node.filter}
          </Mono>
        )}
        {share > 0 && (
          <div
            className="mt-1.5 h-0.5 overflow-hidden rounded-full bg-ink-4"
            title={`${share}% of the highest estimated cost`}
          >
            <div
              className={cx("h-full", hot.has(node.id) ? "bg-amber" : "bg-cyan-dim")}
              style={{ width: `${share}%` }}
            />
          </div>
        )}
      </div>
      {node.children.length > 0 && (
        <ul>
          {node.children.map((c) => (
            <Node key={c.id} node={c} max={max} depth={depth + 1} hot={hot} />
          ))}
        </ul>
      )}
    </li>
  );
}

export function PlanView({ plan }: { plan: QueryPlan }) {
  const max = useMemo(() => maxCost(plan.root), [plan]);
  const hot = useMemo(
    () => new Set(plan.observations.filter((o) => o.severity !== "info").map((o) => o.nodeId)),
    [plan],
  );
  return (
    <div className="grid grid-cols-[1fr_320px] gap-4 p-4">
      <div>
        <div className="mb-2 flex items-center gap-4">
          <Label>estimated plan</Label>
          <Mono className="text-fg-muted">
            total cost {plan.totalCost != null ? plan.totalCost.toFixed(2) : "—"}
          </Mono>
        </div>
        <ul data-testid="plan-tree">
          <Node node={plan.root} max={max} depth={0} hot={hot} />
        </ul>
      </div>
      <div className="space-y-3">
        <Panel title="observations">
          {plan.observations.length === 0 ? (
            <p className="p-3 text-[12px] text-fg-dim">
              Nothing noteworthy in this plan. That is not a guarantee: only estimates were
              inspected.
            </p>
          ) : (
            <ul>
              {plan.observations.map((o, i) => (
                <li key={i} className="border-b border-line p-3 last:border-b-0">
                  <div className="mb-1 flex gap-1.5">
                    <SeverityBadge severity={o.severity} />
                    <EvidenceBadge evidence={o.evidence} />
                  </div>
                  <p className="text-[12px]">{o.message}</p>
                </li>
              ))}
            </ul>
          )}
        </Panel>
        <div className="rounded-sm border border-line bg-ink-1 p-3 text-[11px] text-fg-dim">
          {plan.notes.map((n) => (
            <p key={n}>{n}</p>
          ))}
        </div>
      </div>
    </div>
  );
}
