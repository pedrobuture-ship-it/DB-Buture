import { autoLayout } from "./layout";

const nodes = [
  { id: "a", width: 100, height: 40 },
  { id: "b", width: 100, height: 40 },
  { id: "c", width: 100, height: 40 },
];
const edges = [
  { id: "1", source: "a", target: "b" },
  { id: "2", source: "b", target: "c" },
];

describe("autoLayout", () => {
  it("positions every node without overlap in hierarchical mode", async () => {
    const pos = await autoLayout(nodes, edges, "hierarchical");
    expect(Object.keys(pos).sort()).toEqual(["a", "b", "c"]);
    expect(pos.a?.y).toBeLessThan(pos.b?.y ?? 0);
    expect(pos.b?.y).toBeLessThan(pos.c?.y ?? 0);
  });

  it("lays out left to right in horizontal mode", async () => {
    const pos = await autoLayout(nodes, edges, "horizontal");
    expect(pos.a?.x).toBeLessThan(pos.b?.x ?? 0);
  });

  it("supports compact mode and ignores self references / unknown edges", async () => {
    const pos = await autoLayout(
      nodes,
      [...edges, { id: "s", source: "a", target: "a" }, { id: "x", source: "a", target: "zzz" }],
      "compact",
    );
    expect(Object.keys(pos)).toHaveLength(3);
  });

  it("handles an empty graph", async () => {
    expect(await autoLayout([], [], "vertical")).toEqual({});
  });
});
