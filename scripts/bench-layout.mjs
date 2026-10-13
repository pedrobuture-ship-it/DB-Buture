// Times ELK auto layout for synthetic graphs. Usage: node scripts/bench-layout.mjs
import ELK from "elkjs/lib/elk.bundled.js";

const elk = new ELK();
let seed = 12345;
const rnd = (m) => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) % m;

for (const n of (process.argv[2] ?? "100,500,1000,5000").split(",").map(Number)) {
  const nodes = Array.from({ length: n }, (_, i) => ({
    id: `t${i}`,
    width: 248,
    height: 30 + 12 * 20,
  }));
  const edges = [];
  for (let i = 1; i < n; i++)
    for (let k = 0; k < 1 + rnd(3); k++)
      edges.push({
        id: `e${i}-${k}`,
        sources: [`t${i}`],
        targets: [`t${Math.min(rnd(i), rnd(i))}`],
      });
  for (const [label, opts] of [
    [
      "layered",
      { "elk.algorithm": "layered", "elk.direction": "DOWN", "elk.edgeRouting": "ORTHOGONAL" },
    ],
    ["rectpacking", { "elk.algorithm": "rectpacking" }],
    [
      "layered-fast",
      {
        "elk.algorithm": "layered",
        "elk.direction": "DOWN",
        "elk.edgeRouting": "POLYLINE",
        "elk.layered.thoroughness": "1",
        "elk.layered.crossingMinimization.strategy": "INTERACTIVE",
        "elk.layered.nodePlacement.strategy": "SIMPLE",
        "elk.layered.cycleBreaking.strategy": "GREEDY",
      },
    ],
  ]) {
    const t = performance.now();
    await elk.layout({ id: "root", layoutOptions: opts, children: nodes, edges });
    console.log(`| ${n} | ${label} | ${(performance.now() - t).toFixed(0)} ms |`);
  }
}
