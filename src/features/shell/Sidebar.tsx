import { Hexagon } from "lucide-react";
import { Label, cx } from "@/components/ui";
import { useUi } from "@/stores/ui";
import { NAV } from "./nav";

export function Sidebar() {
  const view = useUi((s) => s.view);
  const setView = useUi((s) => s.setView);
  return (
    <nav aria-label="Primary" className="flex w-52 shrink-0 flex-col border-r border-line bg-ink-1">
      <div className="flex h-12 items-center gap-2 border-b border-line px-4">
        <Hexagon size={18} className="text-cyan" strokeWidth={1.5} />
        <span className="font-mono text-[13px] font-semibold tracking-[0.14em]">
          DB<span className="text-cyan">-</span>BUTURE
        </span>
      </div>
      <div className="flex-1 overflow-y-auto py-2">
        {NAV.map((group, gi) => (
          <div
            key={gi}
            className={cx("px-2 py-1.5", group.title === "" && "mt-2 border-t border-line pt-3")}
          >
            {group.title && <Label className="mb-1 block px-2">{group.title}</Label>}
            {group.items.map((item) => {
              const active = view === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setView(item.id)}
                  aria-current={active ? "page" : undefined}
                  className={cx(
                    "flex h-7 w-full items-center gap-2.5 rounded-sm border-l-2 px-2 text-left text-[12px] transition-colors",
                    active
                      ? "border-cyan bg-cyan-wash text-fg"
                      : "border-transparent text-fg-muted hover:bg-ink-3 hover:text-fg",
                  )}
                >
                  <item.icon size={14} strokeWidth={1.6} className={active ? "text-cyan" : ""} />
                  {item.label}
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </nav>
  );
}
