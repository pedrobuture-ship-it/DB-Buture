import { Search, Unplug } from "lucide-react";
import { Badge, Button, Label, Mono } from "@/components/ui";
import { useDisconnect } from "@/hooks/queries";
import { engineLabel } from "@/lib/format";
import { useUi } from "@/stores/ui";
import type { ActiveInfo } from "@/types/schema";

export function Header({ info }: { info: ActiveInfo }) {
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const disconnect = useDisconnect();
  const meta: [string, string][] = [
    ["connection", info.connectionName],
    ["database", info.database],
    ["engine", engineLabel[info.engine] ?? info.engine],
    ["version", info.serverVersion.split(" ")[0] ?? info.serverVersion],
  ];
  return (
    <header className="flex h-12 shrink-0 items-center gap-5 border-b border-line bg-ink-1 px-4">
      {meta.map(([k, v]) => (
        <div key={k} className="flex min-w-0 flex-col">
          <Label>{k}</Label>
          <Mono className="truncate text-fg">{v}</Mono>
        </div>
      ))}
      <Badge tone="green">READ ONLY</Badge>
      <div className="ml-auto flex items-center gap-2">
        <button
          type="button"
          onClick={() => setPaletteOpen(true)}
          className="flex h-8 w-72 items-center gap-2 rounded-sm border border-line-strong bg-ink-2 px-2.5 text-left text-[12px] text-fg-dim hover:border-cyan-dim"
        >
          <Search size={13} />
          <span className="flex-1">Search objects…</span>
          <kbd className="rounded-sm border border-line-strong px-1 font-mono text-[10px]">
            Ctrl K
          </kbd>
        </button>
        <Button variant="ghost" onClick={() => disconnect.mutate()} aria-label="Disconnect">
          <Unplug size={13} /> Disconnect
        </Button>
      </div>
    </header>
  );
}
