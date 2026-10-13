import { FileUp, ArrowLeft } from "lucide-react";
import { useRef, useState } from "react";
import { Badge, Button, ErrorNote, Label, Mono, Panel } from "@/components/ui";
import { keys } from "@/hooks/queries";
import { ipc, toAppError, type SqlDialect } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import { useQueryClient } from "@tanstack/react-query";
import type { AppError, ImportResult } from "@/types/schema";

const DIALECTS: { id: SqlDialect; label: string }[] = [
  { id: "postgres", label: "PostgreSQL" },
  { id: "mysql", label: "MySQL" },
  { id: "sqlite", label: "SQLite" },
  { id: "generic", label: "Generic" },
];

export function ImportScreen() {
  const setImportOpen = useUi((s) => s.setImportOpen);
  const qc = useQueryClient();
  const [sql, setSql] = useState("");
  const [dialect, setDialect] = useState<SqlDialect>("postgres");
  const [result, setResult] = useState<ImportResult | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  const parse = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(await ipc.importSql(sql, dialect));
    } catch (e) {
      setError(toAppError(e));
      setResult(null);
    } finally {
      setBusy(false);
    }
  };

  const open = async () => {
    setBusy(true);
    setError(null);
    try {
      const info = await ipc.openImport(sql, dialect);
      useUi.getState().reset();
      qc.removeQueries({ queryKey: ["schema"] });
      qc.removeQueries({ queryKey: ["analysis"] });
      qc.setQueryData(keys.status, info);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  const onFile = async (file: File | undefined) => {
    if (!file) return;
    setSql(await file.text());
    setResult(null);
    if (/\.sqlite|\.db/i.test(file.name))
      setError({
        kind: "validation",
        message: "That looks like a database file. Use Connect → SQLite for those.",
      });
  };

  const tables = result?.schema.tables.length ?? 0;
  const views = result?.schema.views.length ?? 0;

  return (
    <div className="bp-grid h-full overflow-auto p-8">
      <div className="fade-in mx-auto max-w-5xl">
        <button
          type="button"
          onClick={() => setImportOpen(false)}
          className="mb-4 flex items-center gap-1.5 text-[12px] text-fg-muted hover:text-cyan"
        >
          <ArrowLeft size={13} /> Back to connections
        </button>
        <div className="mb-5">
          <div className="font-mono text-[11px] uppercase tracking-[0.2em] text-cyan">
            DB-BUTURE
          </div>
          <h1 className="mt-1 text-xl font-semibold tracking-tight">IMPORT SQL</h1>
          <p className="mt-1 text-[12px] text-fg-muted">
            Understand a schema from DDL without connecting to any database. Nothing leaves this
            machine.
          </p>
        </div>

        <Panel
          title="schema script"
          action={
            <div className="flex items-center gap-2">
              <select
                aria-label="SQL dialect"
                value={dialect}
                onChange={(e) => setDialect(e.target.value as SqlDialect)}
                className="h-6 rounded-sm border border-line-strong bg-ink-2 px-1.5 font-mono text-[11px]"
              >
                {DIALECTS.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.label}
                  </option>
                ))}
              </select>
              <input
                ref={fileRef}
                type="file"
                accept=".sql,text/plain"
                hidden
                onChange={(e) => void onFile(e.target.files?.[0])}
              />
              <Button onClick={() => fileRef.current?.click()}>
                <FileUp size={12} /> Open .sql
              </Button>
            </div>
          }
        >
          <textarea
            value={sql}
            onChange={(e) => {
              setSql(e.target.value);
              setResult(null);
            }}
            spellCheck={false}
            aria-label="SQL script"
            placeholder={
              "CREATE TABLE users (id bigint PRIMARY KEY, email text NOT NULL UNIQUE);\nCREATE TABLE orders (id bigint PRIMARY KEY, user_id bigint REFERENCES users(id));"
            }
            className="h-72 w-full resize-y bg-ink-0 p-3 font-mono text-[12px] leading-relaxed outline-none placeholder:text-fg-dim"
          />
          <div className="flex items-center gap-2 border-t border-line p-2">
            <Button
              variant="primary"
              disabled={busy || sql.trim() === ""}
              onClick={() => void parse()}
            >
              {busy ? "Parsing…" : "Parse"}
            </Button>
            <Button disabled={busy || !result || tables + views === 0} onClick={() => void open()}>
              Open schema
            </Button>
          </div>
        </Panel>

        {error && (
          <div className="mt-3">
            <ErrorNote error={error} />
          </div>
        )}

        {result && (
          <div className="mt-4 space-y-3" role="status">
            <div
              className={
                result.status === "parsed"
                  ? "rounded-md border border-green/40 bg-green-wash px-4 py-3"
                  : "rounded-md border border-amber/40 bg-amber-wash px-4 py-3"
              }
            >
              <div
                className={
                  result.status === "parsed"
                    ? "font-mono text-[11px] uppercase tracking-[0.14em] text-green"
                    : "font-mono text-[11px] uppercase tracking-[0.14em] text-amber"
                }
              >
                {result.status === "parsed" ? "PARSED" : "PARSED WITH WARNINGS"}
              </div>
              <div className="mt-1 flex gap-6 font-mono text-[12px]">
                <span>{result.statements} statements</span>
                <span>{result.understood} understood</span>
                <span>{tables} tables</span>
                <span>{views} views</span>
                <span>
                  {result.warnings.length}
                  {result.warningsTruncated ? "+" : ""} warnings
                </span>
              </div>
            </div>
            {result.warnings.length > 0 && (
              <Panel title="not understood">
                <ul>
                  {result.warnings.map((w, i) => (
                    <li key={i} className="border-b border-line px-3 py-2 last:border-b-0">
                      <div className="flex items-center gap-2">
                        <Badge tone="amber">
                          {w.statementNo > 0 ? `#${w.statementNo}` : "schema"}
                        </Badge>
                        <span className="text-[12px] text-amber">{w.reason}</span>
                      </div>
                      <Mono className="mt-1 block truncate text-fg-dim">{w.statement}</Mono>
                    </li>
                  ))}
                </ul>
                {result.warningsTruncated && (
                  <p className="px-3 py-2 text-[11px] text-fg-dim">
                    More warnings exist than are listed here.
                  </p>
                )}
              </Panel>
            )}
          </div>
        )}
        <Label className="mt-6 block">
          Imported workspaces are offline: SQL editor, query plans and refresh need a live
          connection.
        </Label>
      </div>
    </div>
  );
}
