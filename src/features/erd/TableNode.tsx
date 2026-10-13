import { Handle, Position, useStore, type Node, type NodeProps } from "@xyflow/react";
import { memo } from "react";
import { cx } from "@/components/ui";
import { HEADER_H, ROW_H, visibleColumns, type ErdNodeData } from "@/lib/erd";
import { columnMarkers } from "@/lib/schemaIndex";

export type TableFlowNode = Node<ErdNodeData, "table">;

const markerColor = { PK: "text-amber", FK: "text-cyan", UQ: "text-fg-muted" } as const;

// Below this zoom the node collapses to its title (level of detail).
const zoomSelector = (s: { transform: [number, number, number] }) => s.transform[2] < 0.45;

function TableNodeImpl({ data, selected }: NodeProps<TableFlowNode>) {
  const lowZoom = useStore(zoomSelector);
  const { table } = data;
  const compact = data.compact || lowZoom;
  const { shown, hidden } = visibleColumns(table);

  return (
    <div
      onContextMenu={(e) => {
        e.preventDefault();
        data.onContext?.(table.stableId, e.clientX, e.clientY);
      }}
      className={cx(
        "w-[248px] rounded-sm border bg-ink-1 font-mono text-[11px] transition-opacity",
        selected
          ? "border-cyan shadow-[0_0_0_1px_var(--color-cyan)]"
          : data.highlighted
            ? "border-cyan-dim"
            : "border-line-strong",
        data.dimmed && "opacity-30",
      )}
    >
      <div
        className={cx(
          "flex items-center justify-between border-b px-2",
          selected ? "border-cyan bg-cyan-wash" : "border-line bg-ink-3",
        )}
        style={{ height: HEADER_H }}
      >
        <span className="truncate text-[12px] font-medium text-fg">{table.name}</span>
        <span className="ml-2 shrink-0 text-[9px] uppercase tracking-wider text-fg-dim">
          {table.namespace}
        </span>
      </div>
      {compact ? (
        <>
          <Handle
            type="target"
            position={Position.Left}
            id="t:*"
            className="!h-1.5 !w-1.5 !border-0 !bg-cyan-dim"
          />
          <Handle
            type="source"
            position={Position.Right}
            id="s:*"
            className="!h-1.5 !w-1.5 !border-0 !bg-cyan-dim"
          />
        </>
      ) : (
        <div className="py-[3px]">
          {shown.map((c) => {
            const markers = columnMarkers(table, c.name);
            return (
              <div
                key={c.stableId}
                className="relative flex items-center gap-1.5 px-2"
                style={{ height: ROW_H }}
              >
                <span
                  className={cx(
                    "w-8 shrink-0 whitespace-nowrap text-[9px] font-semibold",
                    markers[0] ? markerColor[markers[0]] : "",
                  )}
                >
                  {markers.join(" ")}
                </span>
                <span
                  className={cx(
                    "min-w-0 flex-1 truncate",
                    c.nullable ? "text-fg-muted" : "text-fg",
                  )}
                >
                  {c.name}
                </span>
                <span className="shrink-0 truncate text-[10px] text-fg-dim">{c.dataType}</span>
                <Handle
                  type="target"
                  position={Position.Left}
                  id={`t:${c.name}`}
                  className="!h-1.5 !w-1.5 !border-0 !bg-cyan-dim"
                />
                <Handle
                  type="source"
                  position={Position.Right}
                  id={`s:${c.name}`}
                  className="!h-1.5 !w-1.5 !border-0 !bg-cyan-dim"
                />
              </div>
            );
          })}
          {hidden > 0 && (
            <div
              className="px-2 text-[10px] text-fg-dim"
              style={{ height: ROW_H, lineHeight: `${ROW_H}px` }}
            >
              + {hidden} more columns
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export const TableNode = memo(TableNodeImpl);
