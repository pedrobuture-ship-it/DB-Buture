import {
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Edge,
  type Node,
  type NodeProps,
  Handle,
  Position,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useMemo, useState } from "react";
import { autoLayout, type LayoutMode } from "@/lib/layout";
import type { DependencyKind, GraphEdge, GraphNode, StableId } from "@/types/schema";
import { cx } from "@/components/ui";

export const EDGE_STYLE: Record<DependencyKind, { color: string; dash?: string; label: string }> = {
  FOREIGN_KEY: { color: "#2dd4e6", label: "Foreign key" },
  DEPENDS_ON: { color: "#f2b13c", dash: "6 4", label: "Depends on" },
  DERIVES_FROM: { color: "#3ddc97", dash: "2 4", label: "Derives from" },
  TRIGGER_DEPENDENCY: { color: "#f0555a", dash: "10 3 2 3", label: "Trigger" },
};

const KIND_STYLE: Record<GraphNode["kind"], string> = {
  table: "border-cyan-dim text-fg",
  view: "border-amber/50 text-fg",
  materialized_view: "border-amber/50 text-fg",
  routine: "border-green/50 text-fg",
  trigger: "border-red/50 text-fg",
};

const KIND_TAG: Record<GraphNode["kind"], string> = {
  table: "TBL",
  view: "VIEW",
  materialized_view: "MAT",
  routine: "FN",
  trigger: "TRG",
};

interface PillData extends Record<string, unknown> {
  node: GraphNode;
  emphasis: "target" | "path" | "normal";
}

function Pill({ data, selected }: NodeProps<Node<PillData, "pill">>) {
  return (
    <div
      className={cx(
        "flex h-8 w-[200px] items-center gap-2 rounded-sm border bg-ink-1 px-2 font-mono text-[11px]",
        KIND_STYLE[data.node.kind],
        data.emphasis === "target" &&
          "border-cyan bg-cyan-wash shadow-[0_0_0_1px_var(--color-cyan)]",
        selected && "border-cyan",
      )}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-1.5 !w-1.5 !border-0 !bg-fg-dim"
      />
      <span className="shrink-0 text-[9px] text-fg-dim">{KIND_TAG[data.node.kind]}</span>
      <span className="truncate">{data.node.name}</span>
      <Handle
        type="source"
        position={Position.Right}
        className="!h-1.5 !w-1.5 !border-0 !bg-fg-dim"
      />
    </div>
  );
}

const nodeTypes = { pill: Pill };

function Inner({
  nodes,
  edges,
  target,
  mode,
  onSelect,
}: {
  nodes: GraphNode[];
  edges: GraphEdge[];
  target?: StableId;
  mode: LayoutMode;
  onSelect?: (id: StableId) => void;
}) {
  const [positions, setPositions] = useState<Record<string, { x: number; y: number }>>({});
  const flow = useReactFlow();

  const key = useMemo(
    () => nodes.map((n) => n.id).join("|") + "#" + edges.length + mode,
    [nodes, edges, mode],
  );
  useEffect(() => {
    let cancelled = false;
    void autoLayout(
      nodes.map((n) => ({ id: n.id, width: 200, height: 32 })),
      edges.map((e, i) => ({ id: `${i}`, source: e.from, target: e.to })),
      mode,
    ).then((p) => {
      if (cancelled) return;
      setPositions(p);
      requestAnimationFrame(() => void flow.fitView({ padding: 0.2, duration: 200 }));
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const flowNodes: Node<PillData, "pill">[] = useMemo(
    () =>
      nodes.map((n) => ({
        id: n.id,
        type: "pill",
        position: positions[n.id] ?? { x: 0, y: 0 },
        data: { node: n, emphasis: n.id === target ? "target" : "normal" },
      })),
    [nodes, positions, target],
  );
  const flowEdges: Edge[] = useMemo(
    () =>
      edges.map((e, i) => {
        const s = EDGE_STYLE[e.kind];
        return {
          id: `${i}:${e.from}>${e.to}:${e.kind}`,
          source: e.from,
          target: e.to,
          type: "smoothstep",
          markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12, color: s.color },
          style: { stroke: s.color, strokeDasharray: s.dash, strokeWidth: 1.2 },
        };
      }),
    [edges],
  );

  return (
    <ReactFlow
      nodes={flowNodes}
      edges={flowEdges}
      nodeTypes={nodeTypes}
      onNodeClick={(_, n) => onSelect?.(n.id)}
      minZoom={0.05}
      maxZoom={1.6}
      fitView
      colorMode="dark"
      nodesConnectable={false}
      onlyRenderVisibleElements
    >
      <Background variant={BackgroundVariant.Lines} gap={24} color="rgba(45,212,230,0.045)" />
      <Controls showInteractive={false} />
    </ReactFlow>
  );
}

export function MiniGraph(props: React.ComponentProps<typeof Inner>) {
  return (
    <ReactFlowProvider>
      <Inner {...props} />
    </ReactFlowProvider>
  );
}

export function EdgeLegend({ kinds }: { kinds?: DependencyKind[] }) {
  const list = (Object.keys(EDGE_STYLE) as DependencyKind[]).filter(
    (k) => !kinds || kinds.includes(k),
  );
  return (
    <div className="flex items-center gap-4 font-mono text-[10px] text-fg-muted">
      {list.map((k) => (
        <span key={k} className="flex items-center gap-1.5">
          <svg width="26" height="6" aria-hidden>
            <line
              x1="0"
              y1="3"
              x2="26"
              y2="3"
              stroke={EDGE_STYLE[k].color}
              strokeWidth="1.5"
              strokeDasharray={EDGE_STYLE[k].dash}
            />
          </svg>
          {EDGE_STYLE[k].label}
        </span>
      ))}
    </div>
  );
}
