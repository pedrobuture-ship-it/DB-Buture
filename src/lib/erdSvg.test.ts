import { buildErdSvg } from "./erdSvg";
import { sampleSchema } from "./testSchema";

describe("buildErdSvg", () => {
  const schema = sampleSchema();
  const positions = {
    "postgres:public.users": { x: 0, y: 0 },
    "postgres:public.orders": { x: 400, y: 0 },
  };

  it("draws tables, columns and relationships", () => {
    const svg = buildErdSvg(schema, positions);
    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg).toContain(">users<");
    expect(svg).toContain(">user_id<");
    expect(svg).toContain("marker-end");
    expect(svg.match(/marker-end=/g)?.length).toBe(1);
  });

  it("is well formed XML", () => {
    const doc = new DOMParser().parseFromString(buildErdSvg(schema, positions), "image/svg+xml");
    expect(doc.querySelector("parsererror")).toBeNull();
  });

  it("escapes hostile names", () => {
    const s = structuredClone(schema);
    const t = s.tables[0];
    if (!t) throw new Error("fixture");
    t.name = '<script>alert("x")</script>';
    const svg = buildErdSvg(s, { [t.stableId]: { x: 0, y: 0 } });
    expect(svg).not.toContain("<script>");
    expect(svg).toContain("&lt;script&gt;");
  });

  it("skips tables without a position and relationships to hidden tables", () => {
    const svg = buildErdSvg(schema, { "postgres:public.orders": { x: 0, y: 0 } });
    expect(svg).not.toContain(">users<");
    expect(svg).not.toContain("marker-end=");
  });
});
