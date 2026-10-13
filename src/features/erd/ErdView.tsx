import {
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  useStore,
  type Edge,
  type NodeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Crosshair, Download, LayoutGrid, ListTree, Radar, Route, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, EmptyState, ErrorNote, Label, cx } from "@/components/ui";
import { useAudit } from "@/features/health/HealthView";
import { useSchema } from "@/hooks/queries";
import { buildErdEdges, nodeHeight, NODE_WIDTH } from "@/lib/erd";
import { buildErdSvg, svgToPngBase64 } from "@/lib/erdSvg";
import { HEAT_LABELS, computeHeat, heatColor, type HeatMode } from "@/lib/heatmap";
import { ipc, toAppError, type SavedLayout } from "@/lib/ipc";
import { autoLayout, LAYOUT_LABELS, type LayoutMode } from "@/lib/layout";
import { useUi } from "@/stores/ui";
import type { AppError, StableId } from "@/types/schema";
import { TableNode, type TableFlowNode } from "./TableNode";

const nodeTypes = { table: TableNode };
const LAYOUT_KIND = "erd";
/** Above this many tables nodes start collapsed (level of detail). */
const COMPACT_ABOVE = 150;
/** Above this many tables the default layout is the compact packing. */
const HUGE_ABOVE = 1500;

type Depth = 1 | 2 | 3 | "all";

function ErdCanvas() {
  const { data, error, isLoading } = useSchema();
  const selectedId = useUi((s) => s.selectedId);
  const select = useUi((s) => s.select);
  const flow = useReactFlow();
  const [nodes, setNodes] = useState<TableFlowNode[]>([]);
  const [userMode, setUserMode] = useState<LayoutMode | null>(null);
  const [layingOut, setLayingOut] = useState(false);
  const [allEdges, setAllEdges] = useState(false);
  const [hoverId, setHoverId] = useState<StableId | null>(null);
  // hovering previews the same highlight a selection gives; a selection always wins
  const focusId = selectedId ?? hoverId;
  const [focus, setFocus] = useState<{ id: StableId; depth: Depth } | null>(null);
  const [focusResult, setFocusResult] = useState<{ key: string; ids: Set<StableId> } | null>(null);
  const [menu, setMenu] = useState<{ id: StableId; x: number; y: number } | null>(null);
  const [layoutError, setLayoutError] = useState<AppError | null>(null);
  const [heatMode, setHeatMode] = useState<HeatMode>("normal");
  const [pathOpen, setPathOpen] = useState(false);
  const [pathFrom, setPathFrom] = useState("");
  const [pathTo, setPathTo] = useState("");
  const [path, setPath] = useState<{
    shortest: StableId[] | null;
    alternatives: StableId[][];
  } | null>(null);
  const audit = useAudit(heatMode === "risk");
  const positions = useRef<SavedLayout>({ positions: {}, manual: false });
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const schema = data?.schema;
  const focusKey = focus ? `${focus.id}|${focus.depth}` : null;
  const focusIds = focusKey && focusResult?.key === focusKey ? focusResult.ids : null;
  const tables = useMemo(() => {
    if (!schema) return [];
    return focusIds ? schema.tables.filter((t) => focusIds.has(t.stableId)) : schema.tables;
  }, [schema, focusIds]);
  const compact = tables.length > COMPACT_ABOVE;
  // Very large schemas default to the cheap compact packing; the user can still pick any mode.
  const mode: LayoutMode = userMode ?? (tables.length > HUGE_ABOVE ? "compact" : "hierarchical");

  const visibleIds = useMemo(() => new Set(tables.map((t) => t.stableId)), [tables]);
  const erdEdges = useMemo(
    () => (schema ? buildErdEdges(schema, visibleIds) : []),
    [schema, visibleIds],
  );

  // Focus mode asks Rust (petgraph) for the neighbourhood: React only renders it.
  useEffect(() => {
    if (!focus || !focusKey) return;
    let cancelled = false;
    ipc
      .neighborhood(focus.id, focus.depth === "all" ? undefined : focus.depth, true)
      .then((ids) => !cancelled && setFocusResult({ key: focusKey, ids: new Set(ids) }))
      .catch((e) => !cancelled && setLayoutError(e as AppError));
    return () => {
      cancelled = true;
    };
  }, [focus, focusKey]);

  const persist = useCallback(() => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      void ipc.saveLayout(LAYOUT_KIND, positions.current).catch(() => undefined);
    }, 400);
  }, []);

  const runAutoLayout = useCallback(
    async (m: LayoutMode, list = tables, edgesList = erdEdges) => {
      setLayingOut(true);
      const pos = await autoLayout(
        list.map((t) => ({ id: t.stableId, width: NODE_WIDTH, height: nodeHeight(t, compact) })),
        edgesList.map((e) => ({ id: e.id, source: e.source, target: e.target })),
        m,
      ).finally(() => setLayingOut(false));
      positions.current = { positions: { ...positions.current.positions, ...pos }, manual: false };
      setNodes((prev) =>
        prev.map((n) => (pos[n.id] ? { ...n, position: pos[n.id] ?? n.position } : n)),
      );
      persist();
      requestAnimationFrame(() => void flow.fitView({ duration: 250, padding: 0.15 }));
    },
    [tables, erdEdges, compact, persist, flow],
  );

  // Build nodes whenever the visible tables change. Saved positions win; auto layout only
  // fills in tables that have no position yet, and never moves a manually arranged diagram.
  useEffect(() => {
    if (!schema) return;
    let cancelled = false;
    void (async () => {
      const saved = await ipc.getLayout(LAYOUT_KIND).catch(() => null);
      if (cancelled) return;
      if (saved) positions.current = saved;
      const missing = tables.filter((t) => !positions.current.positions[t.stableId]);
      if (missing.length > 0) {
        setLayingOut(true);
        const pos = await autoLayout(
          tables.map((t) => ({
            id: t.stableId,
            width: NODE_WIDTH,
            height: nodeHeight(t, compact),
          })),
          erdEdges.map((e) => ({ id: e.id, source: e.source, target: e.target })),
          mode,
        ).finally(() => setLayingOut(false));
        if (cancelled) return;
        const keepManual = positions.current.manual;
        for (const t of missing)
          positions.current.positions[t.stableId] = pos[t.stableId] ?? { x: 0, y: 0 };
        positions.current.manual = keepManual;
        persist();
      }
      setNodes(
        tables.map((t) => ({
          id: t.stableId,
          type: "table",
          position: positions.current.positions[t.stableId] ?? { x: 0, y: 0 },
          data: {
            table: t,
            compact,
            huge: tables.length > HUGE_ABOVE,
            highlighted: false,
            dimmed: false,
            heat: null,
            onContext: (id, x, y) => setMenu({ id, x, y }),
          },
        })),
      );
      requestAnimationFrame(() => void flow.fitView({ padding: 0.15 }));
    })();
    return () => {
      cancelled = true;
    };
    // mode intentionally excluded: switching mode is an explicit user action (AUTO LAYOUT)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tables, erdEdges, compact]);

  // Highlight the selected table, its FKs and neighbours; dim everything else.
  const related = useMemo(() => {
    const set = new Set<StableId>();
    if (!focusId) return set;
    set.add(focusId);
    for (const e of erdEdges) {
      if (e.source === focusId) set.add(e.target);
      if (e.target === focusId) set.add(e.source);
    }
    return set;
  }, [focusId, erdEdges]);

  const heat = useMemo(
    () =>
      schema
        ? computeHeat(schema, heatMode, audit.data?.findings ?? [])
        : { values: new Map(), legend: null },
    [schema, heatMode, audit.data],
  );
  const pathNodes = useMemo(() => new Set(path?.shortest ?? []), [path]);
  const pathEdgeKeys = useMemo(() => {
    const keys = new Set<string>();
    const ids = path?.shortest ?? [];
    for (let i = 0; i + 1 < ids.length; i++) {
      keys.add(`${ids[i]}|${ids[i + 1]}`);
      keys.add(`${ids[i + 1]}|${ids[i]}`);
    }
    return keys;
  }, [path]);

  const shownNodes = useMemo(
    () =>
      nodes.map((n) => ({
        ...n,
        selected: n.id === selectedId,
        data: {
          ...n.data,
          highlighted: pathNodes.size > 0 ? pathNodes.has(n.id) : related.has(n.id),
          dimmed:
            pathNodes.size > 0
              ? !pathNodes.has(n.id)
              : focusId != null && related.size > 0 && !related.has(n.id),
          heat: heat.legend
            ? (() => {
                const h = heat.values.get(n.id);
                return h ? { color: heatColor(h.t), label: h.label } : null;
              })()
            : null,
        },
      })),
    [nodes, related, selectedId, heat, pathNodes],
  );

  // Huge schemas: drawing thousands of edges is the main render cost, so by default only the
  // edges of the selected / path tables are drawn. The toggle brings them all back.
  const hugeMode = tables.length > HUGE_ABOVE;
  // at very low zoom huge diagrams draw no edges at all (nodes have no handles there)
  const ultraLow = useStore((st) => st.transform[2] < 0.2);
  const drawnEdges = useMemo(() => {
    if (hugeMode && ultraLow) return [];
    if (!hugeMode || allEdges) return erdEdges;
    return erdEdges.filter(
      (e) =>
        e.source === selectedId ||
        e.target === selectedId ||
        pathEdgeKeys.has(`${e.source}|${e.target}`),
    );
  }, [hugeMode, ultraLow, allEdges, erdEdges, selectedId, pathEdgeKeys]);

  const edges: Edge[] = useMemo(
    () =>
      drawnEdges.map((e) => {
        const onPath = pathEdgeKeys.has(`${e.source}|${e.target}`);
        const active =
          pathNodes.size > 0
            ? onPath
            : focusId != null && (e.source === focusId || e.target === focusId);
        const dim = pathNodes.size > 0 ? !onPath : focusId != null && related.size > 0 && !active;
        return {
          id: e.id,
          source: e.source,
          target: e.target,
          sourceHandle: compact ? "s:*" : `s:${e.sourceColumn}`,
          targetHandle: compact ? "t:*" : `t:${e.targetColumn}`,
          type: "smoothstep",
          markerEnd: {
            type: MarkerType.ArrowClosed,
            width: 14,
            height: 14,
            color: active ? "#2dd4e6" : "#3a4655",
          },
          label: active
            ? `${e.fk.sourceColumns.join(", ")} → ${e.fk.targetColumns.join(", ")}`
            : undefined,
          labelStyle: { fill: "#2dd4e6", fontFamily: "var(--font-mono)", fontSize: 10 },
          labelBgStyle: { fill: "#0b0e12" },
          style: {
            stroke: active ? "#2dd4e6" : "#3a4655",
            strokeWidth: active ? 1.6 : 1,
            opacity: dim ? 0.15 : 1,
          },
          zIndex: active ? 10 : 0,
        } satisfies Edge;
      }),
    [drawnEdges, focusId, related, compact, pathNodes, pathEdgeKeys],
  );

  const onNodesChange = useCallback(
    (changes: NodeChange<TableFlowNode>[]) => {
      setNodes((prev) => {
        let next = prev;
        for (const c of changes) {
          if (c.type === "position" && c.position) {
            const { id, position } = c;
            next = next.map((n) => (n.id === id ? { ...n, position } : n));
            if (c.dragging === false) {
              positions.current.positions[id] = position;
              positions.current.manual = true; // never overwritten automatically from now on
              persist();
            }
          }
        }
        return next;
      });
    },
    [persist],
  );

  const names = useMemo(
    () => new Map((schema?.tables ?? []).map((t) => [t.stableId, t.name])),
    [schema],
  );

  const findPath = async () => {
    setLayoutError(null);
    try {
      setPath(await ipc.findPath(pathFrom, pathTo));
    } catch (e) {
      setLayoutError(toAppError(e));
    }
  };

  const exportDiagram = async (kind: "svg" | "png") => {
    if (!schema) return;
    setLayoutError(null);
    try {
      const svg = buildErdSvg(schema, positions.current.positions);
      if (kind === "svg") await ipc.saveExportAs("er-diagram.svg", svg);
      else await ipc.saveExportAs("er-diagram.png", await svgToPngBase64(svg), true);
    } catch (e) {
      setLayoutError(toAppError(e));
    }
  };

  if (isLoading) return <EmptyState title="Reading catalog…" />;
  if (error)
    return (
      <div className="p-6">
        <ErrorNote error={error as unknown as AppError} />
      </div>
    );
  if (!schema) return null;
  if (schema.tables.length === 0) {
    return (
      <EmptyState title="No tables to draw">
        This database has no tables in the visible schemas, so there is no ER diagram yet.
      </EmptyState>
    );
  }

  const focusName = focus ? schema.tables.find((t) => t.stableId === focus.id)?.name : null;
  const selectedTable = selectedId ? schema.tables.find((t) => t.stableId === selectedId) : null;

  return (
    <div className="relative h-full" onClick={() => setMenu(null)}>
      <div className="absolute left-3 top-3 z-10 flex flex-wrap items-center gap-2 rounded-md border border-line bg-ink-1/95 p-1.5">
        <Label className="px-1">AUTO LAYOUT</Label>
        <select
          aria-label="Layout mode"
          value={mode}
          onChange={(e) => setUserMode(e.target.value as LayoutMode)}
          className="h-7 rounded-sm border border-line-strong bg-ink-2 px-1.5 font-mono text-[11px]"
        >
          {(Object.keys(LAYOUT_LABELS) as LayoutMode[]).map((m) => (
            <option key={m} value={m}>
              {LAYOUT_LABELS[m]}
            </option>
          ))}
        </select>
        <Button variant="primary" onClick={() => void runAutoLayout(mode)}>
          <LayoutGrid size={13} /> Run
        </Button>
        <span className="mx-1 h-5 w-px bg-line" />
        {focus ? (
          <>
            <Label className="px-1">focus: {focusName}</Label>
            <select
              aria-label="Focus depth"
              value={String(focus.depth)}
              onChange={(e) =>
                setFocus({
                  id: focus.id,
                  depth: e.target.value === "all" ? "all" : (Number(e.target.value) as Depth),
                })
              }
              className="h-7 rounded-sm border border-line-strong bg-ink-2 px-1.5 font-mono text-[11px]"
            >
              <option value="1">1 hop</option>
              <option value="2">2 hops</option>
              <option value="3">3 hops</option>
              <option value="all">All related</option>
            </select>
            <Button variant="ghost" onClick={() => setFocus(null)} aria-label="Exit focus mode">
              <X size={13} />
            </Button>
          </>
        ) : (
          <Button
            disabled={!selectedTable}
            onClick={() => selectedTable && setFocus({ id: selectedTable.stableId, depth: 2 })}
          >
            <Crosshair size={13} />{" "}
            {selectedTable ? `Focus on ${selectedTable.name}` : "Select a table to focus"}
          </Button>
        )}
      </div>
      <div className="absolute left-3 top-[58px] z-10 flex flex-wrap items-center gap-2 rounded-md border border-line bg-ink-1/95 p-1.5">
        <Label className="px-1">HEATMAP</Label>
        <select
          aria-label="Heatmap mode"
          value={heatMode}
          onChange={(e) => setHeatMode(e.target.value as HeatMode)}
          className="h-7 rounded-sm border border-line-strong bg-ink-2 px-1.5 font-mono text-[11px]"
        >
          {(Object.keys(HEAT_LABELS) as HeatMode[]).map((m) => (
            <option key={m} value={m}>
              {HEAT_LABELS[m]}
            </option>
          ))}
        </select>
        <span className="mx-1 h-5 w-px bg-line" />
        <Button onClick={() => setPathOpen((v) => !v)} aria-pressed={pathOpen}>
          <Route size={13} /> Path
        </Button>
        <Button onClick={() => void exportDiagram("svg")}>
          <Download size={13} /> SVG
        </Button>
        <Button onClick={() => void exportDiagram("png")}>
          <Download size={13} /> PNG
        </Button>
      </div>
      {pathOpen && (
        <div className="absolute left-3 top-[106px] z-10 w-[420px] space-y-2 rounded-md border border-line bg-ink-1/95 p-2.5">
          <div className="flex items-center gap-2">
            {[
              { v: pathFrom, set: setPathFrom, label: "From table" },
              { v: pathTo, set: setPathTo, label: "To table" },
            ].map((f) => (
              <select
                key={f.label}
                aria-label={f.label}
                value={f.v}
                onChange={(e) => f.set(e.target.value)}
                className="h-7 min-w-0 flex-1 rounded-sm border border-line-strong bg-ink-2 px-1.5 font-mono text-[11px]"
              >
                <option value="">{f.label}…</option>
                {schema.tables.map((t) => (
                  <option key={t.stableId} value={t.stableId}>
                    {t.namespace}.{t.name}
                  </option>
                ))}
              </select>
            ))}
            <Button
              variant="primary"
              disabled={!pathFrom || !pathTo}
              onClick={() => void findPath()}
            >
              Find
            </Button>
            <Button variant="ghost" aria-label="Clear path" onClick={() => setPath(null)}>
              <X size={13} />
            </Button>
          </div>
          {path && (
            <div className="font-mono text-[11px]">
              {path.shortest ? (
                <>
                  <div className="text-cyan">shortest · {path.shortest.length - 1} hop(s)</div>
                  <div className="text-fg-muted">
                    {path.shortest.map((id) => names.get(id) ?? id).join(" → ")}
                  </div>
                  {path.alternatives.length > 1 && (
                    <div className="mt-1 text-fg-dim">
                      {path.alternatives.length - 1} alternative route(s) of up to 8 hops
                    </div>
                  )}
                </>
              ) : (
                <div className="text-amber">No relationship path connects these tables.</div>
              )}
            </div>
          )}
        </div>
      )}
      {heat.legend && (
        <div className="absolute bottom-3 left-14 z-10 w-72 rounded-md border border-line bg-ink-1/95 p-2.5">
          <Label>{heat.legend.title}</Label>
          <div
            className="mt-1.5 h-2 rounded-sm"
            style={{
              background: `linear-gradient(to right, ${heatColor(0)}, ${heatColor(0.5)}, ${heatColor(1)})`,
            }}
            aria-hidden
          />
          <div className="mt-1 flex justify-between font-mono text-[10px] text-fg-muted">
            <span>{heat.legend.min}</span>
            <span>{heat.legend.max}</span>
          </div>
          <p className="mt-1 text-[10px] text-fg-dim">{heat.legend.note}</p>
        </div>
      )}
      {hugeMode && (
        <div className="absolute right-3 top-3 z-10 flex items-center gap-2 rounded-md border border-line bg-ink-1/95 px-2.5 py-1.5 text-[11px] text-fg-muted">
          <span>{tables.length} tables · edges drawn for the selection only</span>
          <label className="flex items-center gap-1 text-fg">
            <input
              type="checkbox"
              checked={allEdges}
              onChange={(e) => setAllEdges(e.target.checked)}
            />
            all edges
          </label>
        </div>
      )}
      {layingOut && (
        <div
          role="status"
          className="absolute left-1/2 top-3 z-10 -translate-x-1/2 rounded-md border border-cyan-dim bg-ink-1/95 px-3 py-1.5 font-mono text-[11px] text-cyan"
        >
          Laying out {tables.length} tables…
        </div>
      )}
      {layoutError && (
        <div className="absolute right-3 top-3 z-10 w-80">
          <ErrorNote error={layoutError} />
        </div>
      )}
      <ReactFlow
        nodes={shownNodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onNodeClick={(_, n) => select(n.id)}
        onNodeMouseEnter={(_, n) => setHoverId(n.id)}
        onNodeMouseLeave={() => setHoverId(null)}
        onPaneClick={() => select(null)}
        minZoom={0.05}
        maxZoom={1.8}
        fitView
        colorMode="dark"
        onlyRenderVisibleElements
      >
        <Background variant={BackgroundVariant.Lines} gap={24} color="rgba(45,212,230,0.045)" />
        <MiniMap
          pannable
          zoomable
          nodeColor="#2d3846"
          maskColor="rgba(7,9,12,0.7)"
          className="!border !border-line"
        />
        <Controls showInteractive={false} />
      </ReactFlow>
      {menu && (
        <ul
          role="menu"
          className="fixed z-50 w-48 rounded-sm border border-line-strong bg-ink-2 py-1 text-[12px] shadow-lg"
          style={{ left: menu.x, top: menu.y }}
        >
          {[
            {
              label: "Focus on this table",
              icon: Crosshair,
              run: () => setFocus({ id: menu.id, depth: 2 }),
            },
            { label: "Impact analysis", icon: Radar, run: () => select(menu.id, "impact") },
            { label: "Open in Explorer", icon: ListTree, run: () => select(menu.id, "explorer") },
          ].map((item) => (
            <li key={item.label} role="none">
              <button
                role="menuitem"
                type="button"
                className={cx(
                  "flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-ink-4",
                )}
                onClick={() => {
                  item.run();
                  setMenu(null);
                }}
              >
                <item.icon size={12} className="text-cyan" /> {item.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function ErdView() {
  return (
    <ReactFlowProvider>
      <ErdCanvas />
    </ReactFlowProvider>
  );
}
