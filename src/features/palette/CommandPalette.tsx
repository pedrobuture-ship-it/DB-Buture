import { Columns3, Eye, Hash, KeyRound, Search, Table2, Zap, ArrowRight } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { cx } from "@/components/ui";
import { useSchema } from "@/hooks/queries";
import { fuzzyFilter } from "@/lib/fuzzy";
import { buildSearchEntries, searchEntries, type SearchKind } from "@/lib/searchIndex";
import { ALL_NAV_ITEMS } from "@/features/shell/nav";
import { useUi, type ViewId } from "@/stores/ui";

const KIND_ICON: Record<SearchKind, typeof Table2> = {
  table: Table2,
  view: Eye,
  column: Columns3,
  index: Hash,
  constraint: KeyRound,
  routine: Zap,
};

interface Item {
  key: string;
  label: string;
  detail: string;
  icon: typeof Table2;
  run: () => void;
}

export function CommandPalette() {
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen(!useUi.getState().paletteOpen);
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOpen]);

  return open ? <PaletteDialog /> : null;
}

function PaletteDialog() {
  const setOpen = useUi((s) => s.setPaletteOpen);
  const select = useUi((s) => s.select);
  const setView = useUi((s) => s.setView);
  const selectedId = useUi((s) => s.selectedId);
  const { data } = useSchema();
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const entries = useMemo(() => (data ? buildSearchEntries(data.schema) : []), [data]);

  const items: Item[] = useMemo(() => {
    const close = () => setOpen(false);
    const go = (view: ViewId) => () => {
      setView(view);
      close();
    };
    const actions: Item[] = [
      ...ALL_NAV_ITEMS.map((n) => ({
        key: `nav:${n.id}`,
        label: `Go to ${n.label}`,
        detail: "navigate",
        icon: n.icon as typeof Table2,
        run: go(n.id),
      })),
      ...(selectedId
        ? [
            {
              key: "act:impact",
              label: "Analyze impact of selection",
              detail: "analyze",
              icon: ArrowRight,
              run: () => {
                select(selectedId, "impact");
                close();
              },
            },
          ]
        : []),
    ];
    const objects: Item[] = searchEntries(entries, query).map((e) => ({
      key: e.id,
      label: e.label,
      detail: `${e.kind} · ${e.detail}`,
      icon: KIND_ICON[e.kind],
      run: () => {
        select(e.id, "explorer");
        close();
      },
    }));
    const matchingActions =
      query.trim() === ""
        ? actions.slice(0, 8)
        : fuzzyFilter(actions, query, (a) => a.label).slice(0, 6);
    return [...objects, ...matchingActions];
  }, [entries, query, selectedId, select, setOpen, setView]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-[14vh]"
      onMouseDown={() => setOpen(false)}
    >
      <div
        role="dialog"
        aria-label="Command palette"
        className="fade-in w-[560px] overflow-hidden rounded-md border border-line-strong bg-ink-1 shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex h-10 items-center gap-2 border-b border-line px-3">
          <Search size={14} className="text-fg-dim" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setCursor(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setCursor((c) => Math.min(items.length - 1, c + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setCursor((c) => Math.max(0, c - 1));
              } else if (e.key === "Enter") {
                e.preventDefault();
                items[cursor]?.run();
              }
            }}
            placeholder="Search tables, columns, views, indexes, constraints… or type an action"
            aria-label="Search"
            className="flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-dim"
          />
          <kbd className="rounded-sm border border-line-strong px-1 font-mono text-[10px] text-fg-dim">
            Esc
          </kbd>
        </div>
        <ul role="listbox" className="max-h-[360px] overflow-auto py-1">
          {items.length === 0 && (
            <li className="px-4 py-6 text-center text-[12px] text-fg-dim">
              No matches for “{query}”.
            </li>
          )}
          {items.map((it, i) => (
            <li key={it.key} role="option" aria-selected={i === cursor}>
              <button
                type="button"
                onMouseMove={() => setCursor(i)}
                onClick={it.run}
                className={cx(
                  "flex h-8 w-full items-center gap-2.5 px-3 text-left text-[12px]",
                  i === cursor ? "bg-cyan-wash" : "",
                )}
              >
                <it.icon size={13} className={i === cursor ? "text-cyan" : "text-fg-dim"} />
                <span className="truncate">{it.label}</span>
                <span className="ml-auto shrink-0 font-mono text-[10px] text-fg-dim">
                  {it.detail}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
