import { computeHeat, heatColor, tableOfObject } from "./heatmap";
import { sampleSchema } from "./testSchema";
import type { Finding } from "@/types/schema";

describe("heatmap", () => {
  const schema = sampleSchema();
  const orders = schema.tables.find((t) => t.name === "orders");
  const users = schema.tables.find((t) => t.name === "users");
  if (!orders || !users) throw new Error("fixture");

  it("normal mode has no legend", () => {
    expect(computeHeat(schema, "normal").legend).toBeNull();
  });

  it("size mode uses a log scale and reports unknown values honestly", () => {
    const s = structuredClone(schema);
    const [t0, t1] = s.tables;
    if (!t0 || !t1) throw new Error("fixture");
    t0.sizeBytes = 1024 * 1024;
    t1.sizeBytes = null;
    const { values, legend } = computeHeat(s, "size");
    expect(values.get(t0.stableId)?.t).toBe(1);
    expect(values.get(t1.stableId)).toEqual({ t: null, label: "unknown" });
    expect(legend?.max).toBe("1.0 MB");
    expect(heatColor(null)).toBe("#2a313b");
  });

  it("density counts foreign keys in both directions", () => {
    const { values, legend } = computeHeat(schema, "density");
    expect(values.get(orders.stableId)?.label).toBe("1 FK");
    expect(values.get(users.stableId)?.t).toBe(1);
    expect(legend?.max).toBe("1");
  });

  it("risk mode sums audit points per owning table", () => {
    const fk = orders.foreignKeys[0];
    if (!fk) throw new Error("fixture");
    const findings: Finding[] = [
      {
        ruleId: "fk-without-index",
        severity: "medium",
        category: "performance",
        evidence: "observed_fact",
        object: fk.stableId,
        objectName: "x",
        message: "m",
      },
      {
        ruleId: "table-without-pk",
        severity: "high",
        category: "integrity",
        evidence: "observed_fact",
        object: users.stableId,
        objectName: "y",
        message: "m",
      },
    ];
    const { values } = computeHeat(schema, "risk", findings);
    expect(values.get(orders.stableId)?.label).toBe("5 pts");
    expect(values.get(users.stableId)?.label).toBe("10 pts");
    expect(values.get(users.stableId)?.t).toBe(1);
    expect(tableOfObject(schema, fk.stableId)).toBe(orders.stableId);
    expect(tableOfObject(schema, "nope")).toBeNull();
  });

  it("heat colours span cyan to red", () => {
    expect(heatColor(0)).toBe("rgb(15, 111, 124)");
    expect(heatColor(1)).toBe("rgb(240, 85, 90)");
    expect(heatColor(0.5)).toBe("rgb(242, 177, 60)");
  });
});
