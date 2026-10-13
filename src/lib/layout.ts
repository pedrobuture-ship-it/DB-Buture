import ELK, { type ElkNode } from "elkjs/lib/elk.bundled.js";
import ElkWorker from "elkjs/lib/elk-worker.min.js?worker";

export type LayoutMode = "hierarchical" | "compact" | "horizontal" | "vertical";

export const LAYOUT_LABELS: Record<LayoutMode, string> = {
  hierarchical: "Hierarchical",
  compact: "Compact",
  horizontal: "Horizontal",
  vertical: "Vertical",
};

export interface LayoutNode {
  id: string;
  width: number;
  height: number;
}

export interface LayoutEdge {
  id: string;
  source: string;
  target: string;
}

/**
 * Layout runs in a Web Worker in the app so a large schema never freezes the interface.
 * Tests (no Worker) fall back to the bundled, in-thread engine.
 */
let elkInstance: InstanceType<typeof ELK> | null = null;
function getElk(): InstanceType<typeof ELK> {
  if (!elkInstance) {
    elkInstance =
      typeof Worker === "undefined"
        ? new ELK()
        : new ELK({ workerFactory: () => new ElkWorker() as unknown as Worker });
  }
  return elkInstance;
}

/** Above this many nodes the layered algorithm switches to cheaper settings (about 10x faster). */
export const FAST_LAYOUT_ABOVE = 400;

function options(mode: LayoutMode, nodeCount: number): Record<string, string> {
  const fast = nodeCount > FAST_LAYOUT_ABOVE;
  const base: Record<string, string> = {
    "elk.spacing.nodeNode": "40",
    "elk.layered.spacing.nodeNodeBetweenLayers": "80",
    "elk.edgeRouting": fast ? "POLYLINE" : "ORTHOGONAL",
    ...(fast
      ? {
          "elk.layered.thoroughness": "1",
          "elk.layered.crossingMinimization.strategy": "INTERACTIVE",
          "elk.layered.nodePlacement.strategy": "SIMPLE",
          "elk.layered.cycleBreaking.strategy": "GREEDY",
        }
      : {}),
  };
  switch (mode) {
    case "horizontal":
      return { ...base, "elk.algorithm": "layered", "elk.direction": "RIGHT" };
    case "vertical":
      return {
        ...base,
        "elk.algorithm": "layered",
        "elk.direction": "DOWN",
        "elk.layered.spacing.nodeNodeBetweenLayers": "100",
      };
    case "compact":
      return {
        "elk.algorithm": "rectpacking",
        "elk.spacing.nodeNode": "28",
        "elk.aspectRatio": "1.8",
      };
    default:
      return {
        ...base,
        "elk.algorithm": "layered",
        "elk.direction": "DOWN",
        ...(fast ? {} : { "elk.layered.crossingMinimization.strategy": "LAYER_SWEEP" }),
      };
  }
}

export async function autoLayout(
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  mode: LayoutMode,
): Promise<Record<string, { x: number; y: number }>> {
  if (nodes.length === 0) return {};
  const ids = new Set(nodes.map((n) => n.id));
  const graph: ElkNode = {
    id: "root",
    layoutOptions: options(mode, nodes.length),
    children: nodes.map((n) => ({ id: n.id, width: n.width, height: n.height })),
    edges: edges
      .filter((e) => ids.has(e.source) && ids.has(e.target) && e.source !== e.target)
      .map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] })),
  };
  const out = await getElk().layout(graph);
  const positions: Record<string, { x: number; y: number }> = {};
  for (const c of out.children ?? []) positions[c.id] = { x: c.x ?? 0, y: c.y ?? 0 };
  return positions;
}
