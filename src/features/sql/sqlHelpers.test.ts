import { describe, expect, it } from "vitest";
import { completionSchema } from "@/components/SqlEditor";
import { sampleSchema } from "@/lib/testSchema";
import { formatCell } from "./ResultTable";
import { formatQuery } from "./SqlEditorView";

describe("sql editor helpers", () => {
  it("builds autocomplete from schemas, tables and columns", () => {
    const ns = completionSchema(sampleSchema()) as Record<string, Record<string, string[]>>;
    expect(Object.keys(ns)).toEqual(["public"]);
    expect(ns.public?.users).toEqual(["id", "email"]);
    expect(ns.public?.user_orders).toEqual(["user_id"]);
    expect(completionSchema(undefined)).toEqual({});
  });

  it("formats SQL per dialect and leaves invalid SQL untouched", () => {
    expect(formatQuery("select a,b from t where x=1", "postgres")).toContain(
      "SELECT\n  a,\n  b\nFROM\n  t",
    );
    const broken = "select ' unterminated";
    expect(formatQuery(broken, "postgres")).toBe(broken);
  });

  it("renders result cells honestly", () => {
    expect(formatCell(null)).toBe("NULL");
    expect(formatCell(undefined)).toBe("NULL");
    expect(formatCell({ a: 1 })).toBe('{"a":1}');
    expect(formatCell(0)).toBe("0");
    expect(formatCell(false)).toBe("false");
  });
});
