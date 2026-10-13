import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { Camera, Download, GitCompare, Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { Badge, Button, EmptyState, ErrorNote, Label, Mono, Panel } from "@/components/ui";
import { formatAge } from "@/lib/format";
import { ipc, toAppError } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import type { AppError, DatabaseSchema, SnapshotRow } from "@/types/schema";
import { useDiffSelection } from "./diffSelection";

const KEY = ["snapshots"] as const;

export function Snapshots() {
  const qc = useQueryClient();
  const setView = useUi((s) => s.setView);
  const pick = useDiffSelection((s) => s.pick);
  const list = useQuery({ queryKey: KEY, queryFn: ipc.listSnapshots });
  const [name, setName] = useState("");
  const [opened, setOpened] = useState<{ row: SnapshotRow; schema: DatabaseSchema } | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameText, setRenameText] = useState("");

  const refresh = () => qc.invalidateQueries({ queryKey: KEY });
  const create = useMutation({
    mutationFn: () => ipc.createSnapshot(name.trim() || undefined),
    onSuccess: () => {
      setName("");
      return refresh();
    },
    onError: (e) => setError(toAppError(e)),
  });

  const run = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(toAppError(e));
    }
  };

  const exportSnap = (s: SnapshotRow) =>
    run(async () => {
      const path = await save({
        defaultPath: `${s.name.replace(/[^\w.-]+/g, "_")}.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (path) await ipc.saveExport(path, await ipc.exportSnapshot(s.id));
    });

  return (
    <div className="fade-in space-y-4 p-6">
      <div>
        <Label>SNAPSHOTS</Label>
        <p className="mt-1 text-[12px] text-fg-muted">
          A snapshot is a frozen copy of the schema structure with a SHA-256 of its canonical form.
          It is history, never the current state.
        </p>
      </div>

      <Panel title="new snapshot">
        <div className="flex items-center gap-2 p-3">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Name (optional), e.g. before-release-42"
            aria-label="Snapshot name"
            className="h-8 flex-1 rounded-sm border border-line-strong bg-ink-2 px-2.5 font-mono text-[12px] placeholder:text-fg-dim focus:border-cyan focus:outline-none"
          />
          <Button variant="primary" disabled={create.isPending} onClick={() => create.mutate()}>
            <Camera size={13} /> Take snapshot of loaded metadata
          </Button>
        </div>
      </Panel>

      {error && <ErrorNote error={error} />}

      <Panel title={`history · ${list.data?.length ?? 0}`}>
        {list.isLoading ? (
          <p className="p-4 text-[12px] text-fg-dim">Loading…</p>
        ) : !list.data?.length ? (
          <EmptyState title="No snapshots yet">
            Take a snapshot now, change your schema later, and compare the two.
          </EmptyState>
        ) : (
          <ul>
            {list.data.map((s) => (
              <li
                key={s.id}
                className="flex items-center gap-3 border-b border-line px-3 py-2.5 last:border-b-0"
              >
                <div className="min-w-0 flex-1">
                  {renaming === s.id ? (
                    <form
                      onSubmit={(e) => {
                        e.preventDefault();
                        void run(async () => {
                          await ipc.renameSnapshot(s.id, renameText);
                          setRenaming(null);
                          await refresh();
                        });
                      }}
                    >
                      <input
                        autoFocus
                        value={renameText}
                        onChange={(e) => setRenameText(e.target.value)}
                        onBlur={() => setRenaming(null)}
                        aria-label="New snapshot name"
                        className="h-7 w-full rounded-sm border border-cyan bg-ink-2 px-2 font-mono text-[12px] outline-none"
                      />
                    </form>
                  ) : (
                    <div className="truncate text-[13px]">{s.name}</div>
                  )}
                  <Mono className="text-fg-dim">
                    {s.database} · {s.tables} tables · {s.views} views · {formatAge(s.createdAt)} ·
                    sha {s.sha256.slice(0, 10)}…
                  </Mono>
                </div>
                <Badge>{s.engine}</Badge>
                <Button
                  onClick={() =>
                    void run(async () => {
                      const r = await ipc.getSnapshot(s.id);
                      setOpened({ row: r.snapshot, schema: r.schema });
                    })
                  }
                >
                  Open
                </Button>
                <Button
                  onClick={() => {
                    pick("before", s.id);
                    pick("after", "current");
                    setView("diff");
                  }}
                >
                  <GitCompare size={12} /> Compare
                </Button>
                <Button
                  variant="ghost"
                  aria-label={`Rename ${s.name}`}
                  onClick={() => {
                    setRenaming(s.id);
                    setRenameText(s.name);
                  }}
                >
                  <Pencil size={12} />
                </Button>
                <Button
                  variant="ghost"
                  aria-label={`Export ${s.name}`}
                  onClick={() => void exportSnap(s)}
                >
                  <Download size={12} />
                </Button>
                <Button
                  variant="ghost"
                  aria-label={`Delete ${s.name}`}
                  onClick={() =>
                    void run(async () => {
                      await ipc.deleteSnapshot(s.id);
                      if (opened?.row.id === s.id) setOpened(null);
                      await refresh();
                    })
                  }
                >
                  <Trash2 size={12} />
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      {opened && (
        <Panel
          title={`snapshot · ${opened.row.name}`}
          action={<Badge tone="amber">HISTORICAL · not the current state</Badge>}
        >
          <div className="p-4">
            <Mono className="text-fg-dim">
              taken {new Date(opened.row.createdAt).toLocaleString()} · sha256 {opened.row.sha256}
            </Mono>
            <div className="mt-3 grid grid-cols-2 gap-x-6 gap-y-0.5 font-mono text-[12px]">
              {opened.schema.tables.map((t) => (
                <div
                  key={t.stableId}
                  className="flex justify-between border-b border-line/60 py-0.5"
                >
                  <span>
                    {t.namespace}.{t.name}
                  </span>
                  <span className="text-fg-dim">{t.columns.length} cols</span>
                </div>
              ))}
            </div>
          </div>
        </Panel>
      )}
    </div>
  );
}
