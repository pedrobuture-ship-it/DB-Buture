import type { DatabaseSchema, ForeignKey, StableId, Table } from "@/types/schema";

export const NODE_WIDTH = 248;
export const HEADER_H = 30;
export const ROW_H = 20;
export const MAX_ROWS = 14;

export interface ErdNodeData extends Record<string, unknown> {
  table: Table;
  /** Level of detail: compact nodes only show the table name. */
  compact: boolean;
  /** Huge schema: at very low zoom nodes render as bare blocks (no text, no handles). */
  huge: boolean;
  highlighted: boolean;
  dimmed: boolean;
  /** Heatmap tint; null when the heatmap is off. */
  heat: { color: string; label: string } | null;
  onContext?: (id: StableId, x: number, y: number) => void;
}

export interface ErdEdge {
  id: StableId;
  source: StableId;
  target: StableId;
  sourceColumn: string;
  targetColumn: string;
  fk: ForeignKey;
}

export function visibleColumns(t: Table): { shown: Table["columns"]; hidden: number } {
  if (t.columns.length <= MAX_ROWS) return { shown: t.columns, hidden: 0 };
  // keep keys first so relationships stay readable when the table is truncated
  const keyed = new Set([
    ...(t.primaryKey?.columns ?? []),
    ...t.foreignKeys.flatMap((f) => f.sourceColumns),
  ]);
  const rest = t.columns.filter((c) => !keyed.has(c.name));
  const keys = t.columns.filter((c) => keyed.has(c.name));
  const shown = [...keys, ...rest].slice(0, MAX_ROWS).sort((a, b) => a.ordinal - b.ordinal);
  return { shown, hidden: t.columns.length - shown.length };
}

export function nodeHeight(t: Table, compact: boolean): number {
  if (compact) return HEADER_H;
  const { shown, hidden } = visibleColumns(t);
  return HEADER_H + (shown.length + (hidden > 0 ? 1 : 0)) * ROW_H + 6;
}

/** Foreign keys between the given tables (self references included). */
export function buildErdEdges(schema: DatabaseSchema, visible: ReadonlySet<StableId>): ErdEdge[] {
  const edges: ErdEdge[] = [];
  for (const t of schema.tables) {
    if (!visible.has(t.stableId)) continue;
    for (const fk of t.foreignKeys) {
      if (!visible.has(fk.targetTable)) continue;
      edges.push({
        id: fk.stableId,
        source: t.stableId,
        target: fk.targetTable,
        sourceColumn: fk.sourceColumns[0] ?? "",
        targetColumn: fk.targetColumns[0] ?? "",
        fk,
      });
    }
  }
  return edges;
}

/** Tables with at least `min` incident FK edges, used to hide leaf noise in huge schemas. */
export function relationshipDegree(schema: DatabaseSchema): Map<StableId, number> {
  const deg = new Map<StableId, number>();
  for (const t of schema.tables) {
    for (const fk of t.foreignKeys) {
      deg.set(t.stableId, (deg.get(t.stableId) ?? 0) + 1);
      deg.set(fk.targetTable, (deg.get(fk.targetTable) ?? 0) + 1);
    }
  }
  return deg;
}
