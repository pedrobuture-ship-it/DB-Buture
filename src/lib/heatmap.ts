import { relationshipDegree } from "@/lib/erd";
import { formatBytes, formatCount } from "@/lib/format";
import type { DatabaseSchema, Finding, StableId } from "@/types/schema";

export type HeatMode = "normal" | "size" | "rows" | "density" | "risk";

export const HEAT_LABELS: Record<HeatMode, string> = {
  normal: "Normal",
  size: "Table size",
  rows: "Estimated rows",
  density: "Relationship density",
  risk: "Risk",
};

export interface Heat {
  /** 0..1 position on the colour scale; null when the engine does not report the metric. */
  t: number | null;
  label: string;
}

export interface HeatLegend {
  title: string;
  min: string;
  max: string;
  note: string;
}

const RISK_POINTS: Record<Finding["severity"], number> = { high: 10, medium: 5, low: 2, info: 0 };

/** Maps an audit finding's object id onto the table it belongs to. */
export function tableOfObject(schema: DatabaseSchema, id: StableId): StableId | null {
  for (const t of schema.tables) {
    if (t.stableId === id) return t.stableId;
    if (t.indexes.some((i) => i.stableId === id)) return t.stableId;
    if (t.foreignKeys.some((f) => f.stableId === id)) return t.stableId;
    if (t.uniqueConstraints.some((u) => u.stableId === id)) return t.stableId;
    if (t.checkConstraints.some((c) => c.stableId === id)) return t.stableId;
    if (t.primaryKey?.stableId === id) return t.stableId;
  }
  return null;
}

function logScale(values: (number | null)[]): (v: number | null) => number | null {
  const nums = values.filter((v): v is number => v != null && v >= 0);
  if (nums.length === 0) return () => null;
  const max = Math.log1p(Math.max(...nums));
  return (v) => (v == null || max === 0 ? null : Math.log1p(v) / max);
}

export function computeHeat(
  schema: DatabaseSchema,
  mode: HeatMode,
  findings: Finding[] = [],
): { values: Map<StableId, Heat>; legend: HeatLegend | null } {
  const values = new Map<StableId, Heat>();
  if (mode === "normal") return { values, legend: null };

  if (mode === "size" || mode === "rows") {
    const pick = (t: DatabaseSchema["tables"][number]) =>
      mode === "size" ? t.sizeBytes : t.estimatedRows;
    const scale = logScale(schema.tables.map(pick));
    const nums = schema.tables.map(pick).filter((v): v is number => v != null);
    for (const t of schema.tables) {
      const v = pick(t);
      values.set(t.stableId, {
        t: scale(v),
        label: v == null ? "unknown" : mode === "size" ? formatBytes(v) : formatCount(v),
      });
    }
    const fmt = mode === "size" ? formatBytes : formatCount;
    return {
      values,
      legend: {
        title: mode === "size" ? "Total table size (log scale)" : "Estimated rows (log scale)",
        min: nums.length ? fmt(Math.min(...nums)) : "—",
        max: nums.length ? fmt(Math.max(...nums)) : "—",
        note: nums.length
          ? "Grey tables have no value: the engine did not report it."
          : "This engine did not report the metric for any table.",
      },
    };
  }

  if (mode === "density") {
    const deg = relationshipDegree(schema);
    const max = Math.max(0, ...deg.values());
    for (const t of schema.tables) {
      const d = deg.get(t.stableId) ?? 0;
      values.set(t.stableId, { t: max === 0 ? 0 : d / max, label: `${d} FK${d === 1 ? "" : "s"}` });
    }
    return {
      values,
      legend: {
        title: "Foreign keys touching the table",
        min: "0",
        max: String(max),
        note: "Counts foreign keys in either direction.",
      },
    };
  }

  // risk: audit penalty points per table, the same points that feed the health score
  const points = new Map<StableId, number>();
  for (const f of findings) {
    const table = tableOfObject(schema, f.object);
    if (table) points.set(table, (points.get(table) ?? 0) + RISK_POINTS[f.severity]);
  }
  const max = Math.max(0, ...points.values());
  for (const t of schema.tables) {
    const p = points.get(t.stableId) ?? 0;
    values.set(t.stableId, { t: max === 0 ? 0 : p / max, label: `${p} pts` });
  }
  return {
    values,
    legend: {
      title: "Audit penalty points",
      min: "0",
      max: `${max}`,
      note: "Points come from audit rules (facts and heuristics). See Health for the rules behind them.",
    },
  };
}

/** cyan → amber → red, readable on the near-black canvas. */
export function heatColor(t: number | null): string {
  if (t == null) return "#2a313b";
  const clamp = Math.min(1, Math.max(0, t));
  const stops: [number, number, number][] = [
    [15, 111, 124],
    [242, 177, 60],
    [240, 85, 90],
  ];
  const seg = clamp < 0.5 ? 0 : 1;
  const local = clamp < 0.5 ? clamp / 0.5 : (clamp - 0.5) / 0.5;
  const [a, b] = [
    stops[seg] as [number, number, number],
    stops[seg + 1] as [number, number, number],
  ];
  const mix = (i: 0 | 1 | 2) => Math.round(a[i] + (b[i] - a[i]) * local);
  return `rgb(${mix(0)}, ${mix(1)}, ${mix(2)})`;
}
