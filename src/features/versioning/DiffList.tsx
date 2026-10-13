import { Badge, Label, Mono, Panel, cx } from "@/components/ui";
import type { ChangeKind, DiffReport, SchemaDiff } from "@/types/schema";

const tone: Record<ChangeKind, "green" | "red" | "amber"> = {
  added: "green",
  removed: "red",
  changed: "amber",
};
const sign: Record<ChangeKind, string> = { added: "+", removed: "−", changed: "~" };

function show(v: unknown): string {
  if (v === null || v === undefined) return "∅";
  if (typeof v === "string") return v;
  return JSON.stringify(v);
}

export function DiffSummary({ diff }: { diff: SchemaDiff }) {
  return (
    <div className="flex gap-4 font-mono text-[12px]" data-testid="diff-summary">
      <span className="text-green">+{diff.summary.added} added</span>
      <span className="text-red">−{diff.summary.removed} removed</span>
      <span className="text-amber">~{diff.summary.changed} changed</span>
    </div>
  );
}

export function DiffList({ diff }: { diff: SchemaDiff }) {
  if (diff.entries.length === 0) {
    return <p className="p-4 text-[12px] text-fg-dim">No structural differences.</p>;
  }
  return (
    <ul>
      {diff.entries.map((e) => (
        <li key={e.id} className="border-b border-line px-3 py-2 last:border-b-0">
          <div className="flex items-center gap-2">
            <span
              className={cx(
                "w-3 font-mono text-[13px]",
                e.change === "added" && "text-green",
                e.change === "removed" && "text-red",
                e.change === "changed" && "text-amber",
              )}
            >
              {sign[e.change]}
            </span>
            <Badge tone={tone[e.change]}>{e.change}</Badge>
            <Badge>{e.objectType.replace("_", " ")}</Badge>
            <Mono>{e.name}</Mono>
            {e.parent && (
              <Mono className="truncate text-fg-dim">{e.parent.replace(/^[a-z]+:/, "")}</Mono>
            )}
          </div>
          {e.fields.length > 0 && (
            <div className="mt-1.5 grid grid-cols-[110px_1fr_1fr] gap-x-3 pl-6 font-mono text-[11px]">
              <Label>field</Label>
              <Label>before</Label>
              <Label>after</Label>
              {e.fields.map((f) => (
                <div key={f.field} className="contents">
                  <span className="text-fg-muted">{f.field}</span>
                  <span className="truncate text-red/90" title={show(f.before)}>
                    {show(f.before)}
                  </span>
                  <span className="truncate text-green/90" title={show(f.after)}>
                    {show(f.after)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}

export function DiffReportView({ report }: { report: DiffReport }) {
  return (
    <div className="space-y-3">
      <Panel>
        <div className="grid grid-cols-2 gap-4 p-4">
          <div>
            <Label>before</Label>
            <div className="mt-0.5 text-[13px]">{report.beforeLabel}</div>
            <Mono className="text-fg-dim">sha256 {report.beforeSha.slice(0, 16)}…</Mono>
          </div>
          <div>
            <Label>after</Label>
            <div className="mt-0.5 text-[13px]">{report.afterLabel}</div>
            <Mono className="text-fg-dim">sha256 {report.afterSha.slice(0, 16)}…</Mono>
          </div>
        </div>
        <div className="border-t border-line px-4 py-2.5">
          {report.identical ? (
            <span className="font-mono text-[12px] text-green">IDENTICAL STRUCTURE</span>
          ) : (
            <DiffSummary diff={report.diff} />
          )}
        </div>
      </Panel>
      {!report.identical && (
        <Panel title="changes">
          <DiffList diff={report.diff} />
        </Panel>
      )}
    </div>
  );
}
