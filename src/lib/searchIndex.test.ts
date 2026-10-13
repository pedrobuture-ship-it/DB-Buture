import { buildSearchEntries, searchEntries } from "./searchIndex";
import { sampleSchema } from "./testSchema";

describe("searchIndex", () => {
  const entries = buildSearchEntries(sampleSchema());

  it("indexes tables, views, columns and constraints", () => {
    const kinds = new Set(entries.map((e) => e.kind));
    expect(kinds).toEqual(new Set(["table", "view", "column", "constraint"]));
  });

  it("finds a table by fuzzy name and ranks it above columns", () => {
    const r = searchEntries(entries, "usrs");
    expect(r[0]?.label).toBe("users");
    expect(r[0]?.kind).toBe("table");
  });

  it("finds columns with their owner as detail", () => {
    const r = searchEntries(entries, "email");
    expect(r[0]).toMatchObject({ kind: "column", detail: "public.users" });
  });

  it("returns nothing for an empty query and respects the limit", () => {
    expect(searchEntries(entries, "  ")).toEqual([]);
    expect(searchEntries(entries, "i", 2)).toHaveLength(2);
  });
});
