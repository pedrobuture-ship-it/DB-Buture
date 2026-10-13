import { buildTreeRows } from "./tree";
import { sampleSchema } from "./testSchema";

describe("tree", () => {
  const schema = sampleSchema();

  it("shows only collapsed namespaces by default", () => {
    const rows = buildTreeRows(schema, new Set(), "");
    expect(rows.map((r) => r.label)).toEqual(["public"]);
    expect(rows[0]?.expandable).toBe(true);
  });

  it("expands to tables, views and nested groups", () => {
    const rows = buildTreeRows(
      schema,
      new Set(["postgres:namespace:public", "postgres:public.users"]),
      "",
    );
    const labels = rows.map((r) => r.label);
    expect(labels).toContain("users");
    expect(labels).toContain("Columns");
    expect(labels).toContain("Views");
    expect(rows.find((r) => r.label === "Columns")?.depth).toBe(2);
  });

  it("filters with fuzzy search and reveals ancestors", () => {
    const rows = buildTreeRows(schema, new Set(), "email");
    expect(rows.map((r) => r.label)).toEqual(
      expect.arrayContaining(["public", "users", "Columns", "email"]),
    );
    expect(rows.some((r) => r.label === "orders")).toBe(false);
  });

  it("returns nothing when no object matches", () => {
    expect(buildTreeRows(schema, new Set(), "zzzzzz")).toEqual([]);
  });
});
