import type {
  Column,
  DatabaseSchema,
  DbIndex,
  ForeignKey,
  StableId,
  Table,
  View,
} from "@/types/schema";

export interface ColumnRef {
  parent: Table | View;
  column: Column;
}

export interface SchemaIndex {
  tables: Map<StableId, Table>;
  views: Map<StableId, View>;
  columns: Map<StableId, ColumnRef>;
  /** Foreign keys that point at a table. */
  incomingFks: Map<StableId, ForeignKey[]>;
  /** Display names for any known object id. */
  names: Map<StableId, string>;
  /** Index and constraint ids mapped to the table that owns them. */
  owners: Map<StableId, Table>;
}

export function buildSchemaIndex(schema: DatabaseSchema): SchemaIndex {
  const tables = new Map<StableId, Table>();
  const views = new Map<StableId, View>();
  const columns = new Map<StableId, ColumnRef>();
  const incomingFks = new Map<StableId, ForeignKey[]>();
  const names = new Map<StableId, string>();
  const owners = new Map<StableId, Table>();

  for (const t of schema.tables) {
    for (const i of t.indexes) owners.set(i.stableId, t);
    for (const f of t.foreignKeys) owners.set(f.stableId, t);
    for (const u of t.uniqueConstraints) owners.set(u.stableId, t);
    for (const c of t.checkConstraints) owners.set(c.stableId, t);
    if (t.primaryKey) owners.set(t.primaryKey.stableId, t);
    tables.set(t.stableId, t);
    names.set(t.stableId, `${t.namespace}.${t.name}`);
    for (const c of t.columns) columns.set(c.stableId, { parent: t, column: c });
    for (const fk of t.foreignKeys) {
      const list = incomingFks.get(fk.targetTable) ?? [];
      list.push(fk);
      incomingFks.set(fk.targetTable, list);
    }
  }
  for (const v of schema.views) {
    views.set(v.stableId, v);
    names.set(v.stableId, `${v.namespace}.${v.name}`);
    for (const c of v.columns) columns.set(c.stableId, { parent: v, column: c });
  }
  for (const r of schema.routines)
    names.set(r.stableId, `${r.namespace}.${r.name}(${r.arguments})`);
  for (const t of schema.triggers) names.set(t.stableId, `${t.namespace}.${t.name}`);
  return { tables, views, columns, incomingFks, names, owners };
}

export function tableName(t: { namespace: string; name: string }): string {
  return `${t.namespace}.${t.name}`;
}

/** Column role markers shown in the PK / UQ / FK gutter. */
export function columnMarkers(table: Table, column: string): ("PK" | "UQ" | "FK")[] {
  const out: ("PK" | "UQ" | "FK")[] = [];
  if (table.primaryKey?.columns.includes(column)) out.push("PK");
  if (table.uniqueConstraints.some((u) => u.columns.length === 1 && u.columns[0] === column))
    out.push("UQ");
  if (table.foreignKeys.some((f) => f.sourceColumns.includes(column))) out.push("FK");
  return out;
}

export function indexSignature(i: DbIndex): string {
  const cols = [...i.columns, ...i.expressions.map((e) => `(${e})`)].join(", ");
  return `${i.method ?? "btree"}${i.unique ? " unique" : ""} (${cols})`;
}
