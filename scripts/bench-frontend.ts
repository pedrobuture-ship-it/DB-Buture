// Times the pure UI-side data structures on synthetic schemas.
//   npx vite-node scripts/bench-frontend.ts
import { buildSchemaIndex } from "../src/lib/schemaIndex";
import { buildSearchEntries, searchEntries } from "../src/lib/searchIndex";
import { buildTreeRows } from "../src/lib/tree";
import { buildErdEdges } from "../src/lib/erd";
import { computeHeat } from "../src/lib/heatmap";
import type { Column, DatabaseSchema, Table } from "../src/types/schema";

function synthetic(n: number): DatabaseSchema {
  let seed = 7;
  const rnd = (m: number) => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) % m;
  const tables: Table[] = Array.from({ length: n }, (_, i) => {
    const name = `table_${String(i).padStart(5, "0")}`;
    const columns: Column[] = Array.from({ length: 12 }, (_, c) => ({
      stableId: `postgres:column:public.${name}:c${c}`,
      name: `c${c}`,
      namespace: "public",
      ordinal: c + 1,
      dataType: "text",
      nativeType: "text",
      nullable: c > 0,
      default: null,
      generated: null,
      identity: null,
      comment: null,
      engineSpecificMetadata: {},
    }));
    return {
      stableId: `postgres:public.${name}`,
      name,
      namespace: "public",
      comment: null,
      estimatedRows: rnd(1_000_000),
      sizeBytes: rnd(1 << 30),
      columns,
      primaryKey: null,
      uniqueConstraints: [],
      checkConstraints: [],
      indexes: [],
      engineSpecificMetadata: {},
      foreignKeys:
        i === 0
          ? []
          : [
              {
                stableId: `postgres:constraint:public.${name}:fk0`,
                name: "fk0",
                namespace: "public",
                sourceTable: `postgres:public.${name}`,
                sourceColumns: ["c1"],
                targetTable: `postgres:public.table_${String(Math.min(rnd(i), rnd(i))).padStart(5, "0")}`,
                targetColumns: ["c0"],
                onDelete: "no_action" as const,
                onUpdate: "no_action" as const,
              },
            ],
    };
  });
  return {
    engine: "postgres",
    database: "synthetic",
    serverVersion: "17",
    tables,
    views: [],
    triggers: [],
    routines: [],
    sequences: [],
    dependencies: [],
    warnings: [],
    namespaces: [
      {
        stableId: "postgres:namespace:public",
        name: "public",
        isSystem: false,
        engineSpecificMetadata: {},
      },
    ],
  };
}

const ms = (f: () => unknown) => {
  const t = performance.now();
  f();
  return `${(performance.now() - t).toFixed(1)} ms`;
};

console.log(
  "| tables | columns | schema index | tree (collapsed) | tree (filtered) | search entries | search query | ER edges | heatmap |",
);
console.log("|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
for (const n of [100, 500, 1000, 5000]) {
  const s = synthetic(n);
  const open = new Set([s.namespaces[0]?.stableId ?? ""]);
  let entries: ReturnType<typeof buildSearchEntries> = [];
  const row = [
    n,
    n * 12,
    ms(() => buildSchemaIndex(s)),
    ms(() => buildTreeRows(s, open, "")),
    ms(() => buildTreeRows(s, open, "c11")),
    ms(() => (entries = buildSearchEntries(s))),
    ms(() => searchEntries(entries, "table_0042 c3")),
    ms(() => buildErdEdges(s, new Set(s.tables.map((t) => t.stableId)))),
    ms(() => computeHeat(s, "size")),
  ];
  console.log(`| ${row.join(" | ")} |`);
}
