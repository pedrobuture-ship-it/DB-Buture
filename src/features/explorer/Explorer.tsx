import {
  ChevronDown,
  ChevronRight,
  Columns3,
  Eye,
  FolderClosed,
  Hash,
  KeyRound,
  Search,
  Table2,
  Zap,
} from "lucide-react";
import { useMemo, useState, type KeyboardEvent } from "react";
import { EmptyState, ErrorNote, Label, cx } from "@/components/ui";
import { VirtualList } from "@/components/VirtualList";
import { useSchema } from "@/hooks/queries";
import { buildSchemaIndex } from "@/lib/schemaIndex";
import { buildTreeRows, type TreeKind, type TreeRow } from "@/lib/tree";
import { useUi } from "@/stores/ui";
import type { AppError } from "@/types/schema";
import { Inspector } from "./Inspector";

const ROW_H = 24;

const icons: Record<TreeKind, typeof Table2> = {
  namespace: FolderClosed,
  table: Table2,
  view: Eye,
  group: FolderClosed,
  column: Columns3,
  index: Hash,
  constraint: KeyRound,
  routine: Zap,
};

export function Explorer() {
  const { data, error, isLoading } = useSchema();
  const selectedId = useUi((s) => s.selectedId);
  const select = useUi((s) => s.select);
  const showSystem = useUi((s) => s.showSystemSchemas);
  const setShowSystem = useUi((s) => s.setShowSystemSchemas);
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [cursor, setCursor] = useState(0);

  const schema = data?.schema;
  const index = useMemo(() => (schema ? buildSchemaIndex(schema) : null), [schema]);

  // Expand the first namespace by default so the tree is never an empty-looking list.
  const effectiveExpanded = useMemo(() => {
    if (expanded.size > 0 || !schema?.namespaces[0]) return expanded;
    return new Set([schema.namespaces[0].stableId]);
  }, [expanded, schema]);

  const rows = useMemo(
    () => (schema ? buildTreeRows(schema, effectiveExpanded, query) : []),
    [schema, effectiveExpanded, query],
  );

  if (isLoading) return <EmptyState title="Reading catalog…" />;
  if (error)
    return (
      <div className="p-6">
        <ErrorNote error={error as unknown as AppError} />
      </div>
    );
  if (!schema || !index) return null;

  const toggle = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(
        prev.size === 0 && schema.namespaces[0] ? [schema.namespaces[0].stableId] : prev,
      );
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const activate = (row: TreeRow) => {
    if (row.id) select(row.id);
    if (row.expandable && (!row.id || row.kind === "table" || row.kind === "view")) toggle(row.key);
  };

  const onKey = (e: KeyboardEvent) => {
    const row = rows[cursor];
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(rows.length - 1, c + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(0, c - 1));
    } else if (e.key === "ArrowRight" && row?.expandable && !row.expanded) {
      e.preventDefault();
      toggle(row.key);
    } else if (e.key === "ArrowLeft" && row?.expanded) {
      e.preventDefault();
      toggle(row.key);
    } else if (e.key === "Enter" && row) {
      e.preventDefault();
      activate(row);
    }
  };

  return (
    <div className="flex h-full">
      <aside className="flex w-80 shrink-0 flex-col border-r border-line bg-ink-1/80">
        <div className="space-y-2 border-b border-line p-2">
          <div className="flex h-7 items-center gap-2 rounded-sm border border-line-strong bg-ink-2 px-2">
            <Search size={12} className="text-fg-dim" />
            <input
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setCursor(0);
              }}
              placeholder="Filter objects…"
              aria-label="Filter objects"
              className="min-w-0 flex-1 bg-transparent font-mono text-[12px] outline-none placeholder:text-fg-dim"
            />
          </div>
          <label className="flex items-center gap-2 text-[11px] text-fg-muted">
            <input
              type="checkbox"
              checked={showSystem}
              onChange={(e) => setShowSystem(e.target.checked)}
            />
            Show system schemas
          </label>
        </div>
        {rows.length === 0 ? (
          <EmptyState title="No matching objects">
            {query
              ? `Nothing matches “${query}”.`
              : "This database has no tables or views in the visible schemas."}
          </EmptyState>
        ) : (
          <div
            tabIndex={0}
            onKeyDown={onKey}
            className="min-h-0 flex-1 outline-none"
            aria-label="Object tree"
          >
            <VirtualList
              className="h-full"
              items={rows}
              rowHeight={ROW_H}
              scrollToIndex={cursor}
              label="Objects"
              render={(row, i) => {
                const Icon = icons[row.kind];
                const selected = row.id != null && row.id === selectedId;
                return (
                  <div
                    role="treeitem"
                    aria-selected={selected}
                    aria-expanded={row.expandable ? row.expanded : undefined}
                    onClick={() => {
                      setCursor(i);
                      activate(row);
                    }}
                    className={cx(
                      "flex h-full cursor-pointer items-center gap-1.5 pr-2 text-[12px]",
                      selected ? "bg-cyan-wash text-fg" : "hover:bg-ink-3",
                      i === cursor && "outline outline-1 -outline-offset-1 outline-line-strong",
                    )}
                    style={{ paddingLeft: 8 + row.depth * 14 }}
                  >
                    <span className="flex w-3 shrink-0 justify-center text-fg-dim">
                      {row.expandable &&
                        (row.expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />)}
                    </span>
                    <Icon
                      size={12}
                      className={cx(
                        "shrink-0",
                        row.kind === "table" || row.kind === "view" ? "text-cyan" : "text-fg-dim",
                      )}
                    />
                    <span
                      className={cx(
                        "truncate",
                        row.kind === "group" && "text-fg-muted",
                        row.kind === "namespace" && "font-medium",
                      )}
                    >
                      {row.label}
                    </span>
                    {row.hint && (
                      <span className="ml-auto shrink-0 font-mono text-[10px] text-fg-dim">
                        {row.hint}
                      </span>
                    )}
                  </div>
                );
              }}
            />
          </div>
        )}
        <div className="border-t border-line px-3 py-1.5">
          <Label>{rows.length} rows</Label>
        </div>
      </aside>
      <section className="min-w-0 flex-1 overflow-auto">
        <Inspector schema={schema} index={index} selectedId={selectedId} />
      </section>
    </div>
  );
}
