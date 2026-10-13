import { buildErdEdges, nodeHeight, relationshipDegree, visibleColumns, MAX_ROWS } from "./erd";
import { col, sampleSchema, table } from "./testSchema";

describe("erd", () => {
  const schema = sampleSchema();

  it("builds one edge per foreign key between visible tables", () => {
    const all = new Set(schema.tables.map((t) => t.stableId));
    const edges = buildErdEdges(schema, all);
    expect(edges).toHaveLength(1);
    expect(edges[0]).toMatchObject({
      source: "postgres:public.orders",
      target: "postgres:public.users",
      sourceColumn: "user_id",
      targetColumn: "id",
    });
  });

  it("drops edges whose endpoints are hidden", () => {
    expect(buildErdEdges(schema, new Set(["postgres:public.orders"]))).toHaveLength(0);
  });

  it("truncates wide tables but keeps key columns", () => {
    const cols = Array.from({ length: 40 }, (_, i) => col("wide", `c${i}`, i + 1));
    const wide = table("wide", cols, {
      primaryKey: { stableId: "pk", name: "pk", namespace: "public", columns: ["c39"] },
    });
    const { shown, hidden } = visibleColumns(wide);
    expect(shown).toHaveLength(MAX_ROWS);
    expect(hidden).toBe(40 - MAX_ROWS);
    expect(shown.some((c) => c.name === "c39")).toBe(true);
    expect(nodeHeight(wide, true)).toBeLessThan(nodeHeight(wide, false));
  });

  it("counts relationship degree", () => {
    const d = relationshipDegree(schema);
    expect(d.get("postgres:public.orders")).toBe(1);
    expect(d.get("postgres:public.users")).toBe(1);
  });
});
