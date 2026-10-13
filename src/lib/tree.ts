import { fuzzyScore } from "@/lib/fuzzy";
import type { DatabaseSchema, StableId } from "@/types/schema";

export type TreeKind =
  "namespace" | "table" | "view" | "group" | "column" | "index" | "constraint" | "routine";

export interface TreeRow {
  key: string;
  depth: number;
  label: string;
  kind: TreeKind;
  /** Object id to select when the row is activated. */
  id?: StableId;
  hint?: string;
  expandable: boolean;
  expanded: boolean;
}

interface Node {
  key: string;
  label: string;
  kind: TreeKind;
  id?: StableId;
  hint?: string;
  children: Node[];
}

function buildNodes(s: DatabaseSchema): Node[] {
  const roots: Node[] = [];
  for (const ns of s.namespaces) {
    const children: Node[] = [];
    for (const t of s.tables.filter((t) => t.namespace === ns.name)) {
      const kids: Node[] = [];
      if (t.columns.length)
        kids.push({
          key: `${t.stableId}#cols`,
          label: "Columns",
          kind: "group",
          hint: String(t.columns.length),
          children: t.columns.map((c) => ({
            key: c.stableId,
            label: c.name,
            kind: "column",
            id: c.stableId,
            hint: c.dataType,
            children: [],
          })),
        });
      if (t.indexes.length)
        kids.push({
          key: `${t.stableId}#idx`,
          label: "Indexes",
          kind: "group",
          hint: String(t.indexes.length),
          children: t.indexes.map((i) => ({
            key: i.stableId,
            label: i.name,
            kind: "index",
            id: i.stableId,
            hint: i.method ?? undefined,
            children: [],
          })),
        });
      const constraints = [
        ...(t.primaryKey
          ? [{ id: t.primaryKey.stableId, name: t.primaryKey.name, hint: "PK" }]
          : []),
        ...t.foreignKeys.map((f) => ({ id: f.stableId, name: f.name, hint: "FK" })),
        ...t.uniqueConstraints.map((u) => ({ id: u.stableId, name: u.name, hint: "UQ" })),
        ...t.checkConstraints.map((c) => ({ id: c.stableId, name: c.name, hint: "CK" })),
      ];
      if (constraints.length)
        kids.push({
          key: `${t.stableId}#con`,
          label: "Constraints",
          kind: "group",
          hint: String(constraints.length),
          children: constraints.map((c) => ({
            key: c.id,
            label: c.name,
            kind: "constraint",
            id: c.id,
            hint: c.hint,
            children: [],
          })),
        });
      children.push({
        key: t.stableId,
        label: t.name,
        kind: "table",
        id: t.stableId,
        children: kids,
      });
    }
    const views = s.views.filter((v) => v.namespace === ns.name);
    if (views.length)
      children.push({
        key: `${ns.stableId}#views`,
        label: "Views",
        kind: "group",
        hint: String(views.length),
        children: views.map((v) => ({
          key: v.stableId,
          label: v.name,
          kind: "view",
          id: v.stableId,
          hint: v.materialized ? "matview" : undefined,
          children: v.columns.map((c) => ({
            key: c.stableId,
            label: c.name,
            kind: "column",
            id: c.stableId,
            hint: c.dataType,
            children: [],
          })),
        })),
      });
    const routines = s.routines.filter((r) => r.namespace === ns.name);
    if (routines.length)
      children.push({
        key: `${ns.stableId}#routines`,
        label: "Routines",
        kind: "group",
        hint: String(routines.length),
        children: routines.map((r) => ({
          key: r.stableId,
          label: r.name,
          kind: "routine",
          id: r.stableId,
          hint: r.kind,
          children: [],
        })),
      });
    roots.push({ key: ns.stableId, label: ns.name, kind: "namespace", children });
  }
  return roots;
}

/** Keeps nodes that match, plus their ancestors (so the path stays visible). */
function filterNodes(nodes: Node[], query: string, forceOpen: Set<string>): Node[] {
  const out: Node[] = [];
  for (const n of nodes) {
    const kids = filterNodes(n.children, query, forceOpen);
    const self = fuzzyScore(query, n.label) !== null;
    if (self || kids.length) {
      if (kids.length) forceOpen.add(n.key);
      out.push({ ...n, children: self && !kids.length ? n.children.slice(0, 0) : kids });
    }
  }
  return out;
}

export function buildTreeRows(
  schema: DatabaseSchema,
  expanded: ReadonlySet<string>,
  query: string,
): TreeRow[] {
  let nodes = buildNodes(schema);
  const forced = new Set<string>();
  const searching = query.trim() !== "";
  if (searching) nodes = filterNodes(nodes, query, forced);

  const rows: TreeRow[] = [];
  const walk = (list: Node[], depth: number) => {
    for (const n of list) {
      const isExpanded = expanded.has(n.key) || forced.has(n.key);
      rows.push({
        key: n.key,
        depth,
        label: n.label,
        kind: n.kind,
        id: n.id,
        hint: n.hint,
        expandable: n.children.length > 0,
        expanded: isExpanded && n.children.length > 0,
      });
      if (isExpanded) walk(n.children, depth + 1);
    }
  };
  walk(nodes, 0);
  return rows;
}
