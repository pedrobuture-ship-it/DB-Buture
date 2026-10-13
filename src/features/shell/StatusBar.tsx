import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { Mono, cx } from "@/components/ui";
import { useRefreshSchema, useSchema } from "@/hooks/queries";
import { engineLabel, formatAge } from "@/lib/format";
import type { ActiveInfo } from "@/types/schema";

export function StatusBar({ info }: { info: ActiveInfo }) {
  const schema = useSchema();
  const refresh = useRefreshSchema();
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 15_000);
    return () => clearInterval(t);
  }, []);

  const s = schema.data?.schema;
  const objects = s
    ? s.tables.length + s.views.length + s.routines.length + s.sequences.length + s.triggers.length
    : null;

  return (
    <footer className="flex h-6 shrink-0 items-center gap-4 border-t border-line bg-ink-1 px-3 font-mono text-[10px] uppercase tracking-wider text-fg-muted">
      <span className="flex items-center gap-1.5 text-green">
        <span className="h-1.5 w-1.5 rounded-full bg-green" /> CONNECTED
      </span>
      <span>{engineLabel[info.engine]}</span>
      <span className="text-green">READ ONLY</span>
      {objects != null && <span>{objects} objects</span>}
      <span className="ml-auto flex items-center gap-2 normal-case tracking-normal">
        {schema.data && (
          <Mono className="text-fg-dim">metadata cached {formatAge(schema.data.fetchedAt)}</Mono>
        )}
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
          className="flex items-center gap-1 text-fg-muted hover:text-cyan disabled:opacity-50"
          aria-label="Refresh metadata"
        >
          <RefreshCw size={11} className={cx(refresh.isPending && "animate-spin")} /> refresh
        </button>
      </span>
    </footer>
  );
}
