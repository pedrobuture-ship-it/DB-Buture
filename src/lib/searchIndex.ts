import { fuzzyScore } from "@/lib/fuzzy";
import type { DatabaseSchema, StableId } from "@/types/schema";

export type SearchKind = "table" | "view" | "column" | "index" | "constraint" | "routine";

export interface SearchEntry {
  id: StableId;
  label: string;
  detail: string;
  kind: SearchKind;
}

export function buildSearchEntries(s: DatabaseSchema): SearchEntry[] {
  const out: SearchEntry[] = [];
  for (const t of s.tables) {
    out.push({ id: t.stableId, label: t.name, detail: t.namespace, kind: "table" });
    for (const c of t.columns)
      out.push({
        id: c.stableId,
        label: c.name,
        detail: `${t.namespace}.${t.name}`,
        kind: "column",
      });
    for (const i of t.indexes)
      out.push({
        id: i.stableId,
        label: i.name,
        detail: `${t.namespace}.${t.name}`,
        kind: "index",
      });
    for (const f of t.foreignKeys)
      out.push({
        id: f.stableId,
        label: f.name,
        detail: `${t.namespace}.${t.name}`,
        kind: "constraint",
      });
    for (const u of t.uniqueConstraints)
      out.push({
        id: u.stableId,
        label: u.name,
        detail: `${t.namespace}.${t.name}`,
        kind: "constraint",
      });
    for (const c of t.checkConstraints)
      out.push({
        id: c.stableId,
        label: c.name,
        detail: `${t.namespace}.${t.name}`,
        kind: "constraint",
      });
    if (t.primaryKey)
      out.push({
        id: t.primaryKey.stableId,
        label: t.primaryKey.name,
        detail: `${t.namespace}.${t.name}`,
        kind: "constraint",
      });
  }
  for (const v of s.views) {
    out.push({ id: v.stableId, label: v.name, detail: v.namespace, kind: "view" });
    for (const c of v.columns)
      out.push({
        id: c.stableId,
        label: c.name,
        detail: `${v.namespace}.${v.name}`,
        kind: "column",
      });
  }
  for (const r of s.routines)
    out.push({ id: r.stableId, label: r.name, detail: r.namespace, kind: "routine" });
  return out;
}

const KIND_BOOST: Record<SearchKind, number> = {
  table: 3,
  view: 3,
  routine: 1,
  index: 0,
  constraint: 0,
  column: -1,
};

export function searchEntries(entries: SearchEntry[], query: string, limit = 50): SearchEntry[] {
  const q = query.trim();
  if (q === "") return [];
  const scored: { e: SearchEntry; score: number }[] = [];
  for (const e of entries) {
    const score = fuzzyScore(q, e.label);
    if (score !== null) scored.push({ e, score: score + KIND_BOOST[e.kind] });
  }
  scored.sort((a, b) => b.score - a.score || a.e.label.localeCompare(b.e.label));
  return scored.slice(0, limit).map((x) => x.e);
}
