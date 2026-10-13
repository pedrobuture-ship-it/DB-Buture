import { useState } from "react";
import { Badge, Button, ErrorNote, Label, Mono, Panel } from "@/components/ui";
import { useStatus } from "@/hooks/queries";
import { ipc, toAppError } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import type { AppError } from "@/types/schema";

export function Settings() {
  const status = useStatus();
  const showSystem = useUi((s) => s.showSystemSchemas);
  const setShowSystem = useUi((s) => s.setShowSystemSchemas);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<AppError | null>(null);

  const doExport = async (format: "json" | "markdown") => {
    setError(null);
    setMsg(null);
    try {
      const ext = format === "json" ? "json" : "md";
      const path = await ipc.saveExportAs(`schema.${ext}`, await ipc.exportSchema(format));
      if (path) setMsg(`Saved ${path}`);
    } catch (e) {
      setError(toAppError(e));
    }
  };

  const resetLayouts = async () => {
    setError(null);
    try {
      await ipc.clearLayout("erd");
      setMsg("ER diagram layout reset. It will be laid out again next time you open it.");
    } catch (e) {
      setError(toAppError(e));
    }
  };

  return (
    <div className="fade-in max-w-2xl space-y-4 p-6">
      <Label>SETTINGS</Label>

      <Panel title="safety">
        <div className="space-y-2 p-4 text-[12px]">
          <div className="flex items-center gap-2">
            <Badge tone="green">READ ONLY</Badge>
            <span>DB-BUTURE observes, explains and simulates. It does not secretly modify.</span>
          </div>
          <p className="text-fg-muted">
            Sessions are opened read-only (PostgreSQL <Mono>default_transaction_read_only</Mono>,
            MySQL <Mono>SET SESSION TRANSACTION READ ONLY</Mono>, SQLite read-only files) and
            mutating SQL is rejected before it is sent. The strongest protection is still a database
            account that only has read privileges.
          </p>
          <p className="text-fg-muted">
            Passwords live only in your operating system keyring. They are never written to the
            internal database, never sent back to the interface and never exported.
          </p>
        </div>
      </Panel>

      <Panel title="catalog">
        <label className="flex items-center gap-2 p-4 text-[12px]">
          <input
            type="checkbox"
            checked={showSystem}
            onChange={(e) => setShowSystem(e.target.checked)}
          />
          Show system schemas (pg_catalog, information_schema, …) in the explorer and analyses
        </label>
      </Panel>

      <Panel title="export">
        <div className="flex flex-wrap gap-2 p-4">
          <Button onClick={() => void doExport("json")}>Schema as JSON</Button>
          <Button onClick={() => void doExport("markdown")}>Schema as Markdown</Button>
          <Button variant="ghost" onClick={() => void resetLayouts()}>
            Reset ER layout
          </Button>
        </div>
        <p className="px-4 pb-4 text-[11px] text-fg-dim">
          Exports contain schema structure only: no credentials, hosts or data. The ER diagram can
          be exported as PNG or SVG from its toolbar.
        </p>
      </Panel>
      {msg && <p className="font-mono text-[12px] text-green">{msg}</p>}
      {error && <ErrorNote error={error} />}

      <Panel title="about">
        <div className="space-y-1 p-4 text-[12px]">
          <div className="font-mono text-[13px]">DB-BUTURE</div>
          <div className="text-fg-muted">Visual Database Intelligence for Developers.</div>
          {status.data && (
            <Mono className="block text-fg-dim">
              connected: {status.data.connectionName}
              {status.data.imported ? " (imported SQL, offline)" : ""}
            </Mono>
          )}
          <div className="pt-2 font-mono text-[11px] text-fg-dim">
            Copyright © Pedro Buture. All rights reserved.
          </div>
        </div>
      </Panel>
    </div>
  );
}
