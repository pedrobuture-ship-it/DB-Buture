import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Database, FileDigit, Trash2 } from "lucide-react";
import { Badge, Button, ErrorNote, Label, Mono, Panel, cx } from "@/components/ui";
import { useConnect, useSavedConnections } from "@/hooks/queries";
import { ipc, toAppError } from "@/lib/ipc";
import { engineLabel } from "@/lib/format";
import { useQueryClient } from "@tanstack/react-query";
import { keys } from "@/hooks/queries";
import type { AppError, ConnectionConfig, ConnectionProbe, Engine, SslMode } from "@/types/schema";
import { APP_TAGLINE } from "@/lib/version";

type Connectable = Exclude<Engine, "generic">;

const DEFAULT_PORT: Record<Connectable, number | null> = {
  postgres: 5432,
  mysql: 3306,
  sqlite: null,
};
const DEFAULT_USER: Record<Connectable, string> = {
  postgres: "postgres",
  mysql: "root",
  sqlite: "",
};

const ENGINES: { id: Connectable; name: string; note: string }[] = [
  { id: "postgres", name: "PostgreSQL", note: "deepest support" },
  { id: "mysql", name: "MySQL", note: "information_schema" },
  { id: "sqlite", name: "SQLite", note: "local file, read-only" },
];

interface FormState {
  name: string;
  host: string;
  port: string;
  database: string;
  username: string;
  password: string;
  sslMode: SslMode;
  filePath: string;
}

const emptyForm = (engine: Connectable): FormState => ({
  name: "",
  host: "localhost",
  port: DEFAULT_PORT[engine]?.toString() ?? "",
  database: "",
  username: DEFAULT_USER[engine],
  password: "",
  sslMode: "prefer",
  filePath: "",
});

function toConfig(engine: Connectable, f: FormState): ConnectionConfig {
  if (engine === "sqlite") {
    return {
      engine,
      host: null,
      port: null,
      database: null,
      username: null,
      sslMode: "disable",
      filePath: f.filePath.trim(),
    };
  }
  const port = Number.parseInt(f.port, 10);
  return {
    engine,
    host: f.host.trim(),
    port: Number.isNaN(port) ? null : port,
    database: f.database.trim(),
    username: f.username.trim(),
    sslMode: f.sslMode,
    filePath: null,
  };
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <Label>{label}</Label>
      {children}
    </label>
  );
}

const inputCls =
  "h-8 rounded-sm border border-line-strong bg-ink-2 px-2 font-mono text-[12px] text-fg placeholder:text-fg-dim focus:border-cyan focus:outline-none";

export function ConnectScreen() {
  const [engine, setEngine] = useState<Connectable>("postgres");
  const [form, setForm] = useState<FormState>(emptyForm("postgres"));
  const [probe, setProbe] = useState<ConnectionProbe | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState<"test" | "connect" | "save" | null>(null);
  const [saved, setSaved] = useState(false);
  const connect = useConnect();
  const qc = useQueryClient();
  const savedConnections = useSavedConnections();

  const set = <K extends keyof FormState>(k: K, v: FormState[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const pickEngine = (e: Connectable) => {
    setEngine(e);
    setForm(emptyForm(e));
    setProbe(null);
    setError(null);
  };

  // The password is cleared the moment it is sent; it never stays in component state.
  const takePassword = (): string | undefined => {
    const pw = form.password;
    setForm((f) => ({ ...f, password: "" }));
    return pw || undefined;
  };

  const run = async (kind: "test" | "connect" | "save") => {
    setBusy(kind);
    setError(null);
    setSaved(false);
    const config = toConfig(engine, form);
    const password = takePassword();
    try {
      if (kind === "test") {
        setProbe(await ipc.testConnection({ config, password }));
      } else if (kind === "save") {
        await ipc.saveConnection({
          name: form.name.trim() || config.database || "connection",
          config,
          password,
        });
        await qc.invalidateQueries({ queryKey: keys.saved });
        setSaved(true);
      } else {
        await connect.mutateAsync({ config, password, name: form.name.trim() || undefined });
      }
    } catch (e) {
      setError(toAppError(e));
      setProbe(null);
    } finally {
      setBusy(null);
    }
  };

  const browse = async () => {
    const picked = await open({
      multiple: false,
      filters: [
        { name: "SQLite database", extensions: ["db", "sqlite", "sqlite3", "db3"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof picked === "string") set("filePath", picked);
  };

  return (
    <div className="bp-grid flex h-full items-start justify-center overflow-auto p-8">
      <div className="grid w-full max-w-5xl grid-cols-[1fr_320px] gap-6">
        <div className="fade-in">
          <div className="mb-6">
            <div className="font-mono text-[11px] uppercase tracking-[0.2em] text-cyan">
              DB-BUTURE
            </div>
            <h1 className="mt-1 text-xl font-semibold tracking-tight">CONNECT DATABASE</h1>
            <p className="mt-1 text-[12px] text-fg-muted">
              {APP_TAGLINE} Connections are read-only by design.
            </p>
          </div>

          <div className="mb-4 grid grid-cols-3 gap-2">
            {ENGINES.map((e) => (
              <button
                key={e.id}
                type="button"
                onClick={() => pickEngine(e.id)}
                aria-pressed={engine === e.id}
                className={cx(
                  "flex items-center gap-3 rounded-md border px-3 py-3 text-left transition-colors",
                  engine === e.id
                    ? "border-cyan bg-cyan-wash"
                    : "border-line bg-ink-1 hover:border-line-strong",
                )}
              >
                {e.id === "sqlite" ? (
                  <FileDigit size={18} className="text-cyan" />
                ) : (
                  <Database size={18} className="text-cyan" />
                )}
                <span>
                  <span className="block text-[13px] font-medium">{e.name}</span>
                  <span className="block font-mono text-[10px] text-fg-dim">{e.note}</span>
                </span>
              </button>
            ))}
          </div>

          <Panel title={`${engineLabel[engine]} connection`}>
            <form
              className="grid grid-cols-2 gap-3 p-4"
              onSubmit={(ev) => {
                ev.preventDefault();
                void run("connect");
              }}
            >
              <Field label="Connection Name">
                <input
                  className={inputCls}
                  value={form.name}
                  onChange={(e) => set("name", e.target.value)}
                  placeholder="staging"
                />
              </Field>
              {engine === "sqlite" ? (
                <div className="col-span-2 flex items-end gap-2">
                  <div className="flex-1">
                    <Field label="File path">
                      <input
                        className={inputCls}
                        value={form.filePath}
                        onChange={(e) => set("filePath", e.target.value)}
                        placeholder="/path/to/app.db"
                      />
                    </Field>
                  </div>
                  <Button onClick={() => void browse()}>Browse…</Button>
                </div>
              ) : (
                <>
                  <Field label="Host">
                    <input
                      className={inputCls}
                      value={form.host}
                      onChange={(e) => set("host", e.target.value)}
                    />
                  </Field>
                  <Field label="Port">
                    <input
                      className={inputCls}
                      inputMode="numeric"
                      value={form.port}
                      onChange={(e) => set("port", e.target.value)}
                    />
                  </Field>
                  <Field label="Database">
                    <input
                      className={inputCls}
                      value={form.database}
                      onChange={(e) => set("database", e.target.value)}
                    />
                  </Field>
                  <Field label="Username">
                    <input
                      className={inputCls}
                      value={form.username}
                      onChange={(e) => set("username", e.target.value)}
                      autoComplete="off"
                    />
                  </Field>
                  <Field label="Password">
                    <input
                      className={inputCls}
                      type="password"
                      value={form.password}
                      onChange={(e) => set("password", e.target.value)}
                      autoComplete="new-password"
                      placeholder="stored in the OS keyring only if you save"
                    />
                  </Field>
                  <Field label="SSL Mode">
                    <select
                      className={inputCls}
                      value={form.sslMode}
                      onChange={(e) => set("sslMode", e.target.value as SslMode)}
                    >
                      <option value="disable">disable</option>
                      <option value="prefer">prefer</option>
                      <option value="require">require</option>
                      <option value="verifyca">verify-ca</option>
                      <option value="verifyfull">verify-full</option>
                    </select>
                  </Field>
                </>
              )}
              <div className="col-span-2 mt-1 flex items-center gap-2">
                <Button onClick={() => void run("test")} disabled={busy !== null}>
                  {busy === "test" ? "Testing…" : "Test Connection"}
                </Button>
                <Button type="submit" variant="primary" disabled={busy !== null}>
                  {busy === "connect" ? "Connecting…" : "Connect"}
                </Button>
                <Button onClick={() => void run("save")} disabled={busy !== null}>
                  Save Connection
                </Button>
                <span className="ml-auto flex items-center gap-1.5 text-[11px] text-fg-dim">
                  <Badge tone="green">READ ONLY</Badge> best protection: a read-only database
                  account
                </span>
              </div>
            </form>
          </Panel>

          {probe && (
            <div
              role="status"
              className="mt-3 rounded-md border border-green/40 bg-green-wash px-4 py-3"
            >
              <div className="font-mono text-[11px] uppercase tracking-[0.14em] text-green">
                CONNECTION SUCCESSFUL
              </div>
              <div className="mt-1 flex gap-6 font-mono text-[12px] text-fg">
                <span>
                  {engineLabel[probe.engine]} {probe.serverVersion.split(" ")[0]}
                </span>
                <span>Latency {probe.latencyMs}ms</span>
              </div>
            </div>
          )}
          {saved && (
            <div className="mt-3 text-[12px] text-green">
              Saved. The password (if any) went to the OS keyring.
            </div>
          )}
          {(error ?? connect.error) && (
            <div className="mt-3">
              <ErrorNote error={(error ?? connect.error) as AppError} />
            </div>
          )}
        </div>

        <aside className="fade-in">
          <Panel title="Saved connections">
            {savedConnections.data?.length ? (
              <ul>
                {savedConnections.data.map((c) => (
                  <li
                    key={c.id}
                    className="group flex items-center gap-2 border-b border-line px-3 py-2 last:border-b-0"
                  >
                    <button
                      type="button"
                      className="min-w-0 flex-1 text-left"
                      onClick={() => connect.mutate({ savedId: c.id })}
                      title="Connect"
                    >
                      <div className="truncate text-[13px]">{c.name}</div>
                      <Mono className="block truncate text-fg-dim">
                        {engineLabel[c.config.engine]} ·{" "}
                        {c.config.engine === "sqlite"
                          ? c.config.filePath
                          : `${c.config.host}:${c.config.port}/${c.config.database}`}
                      </Mono>
                    </button>
                    <button
                      type="button"
                      aria-label={`Delete ${c.name}`}
                      className="text-fg-dim opacity-0 hover:text-red group-hover:opacity-100 focus-visible:opacity-100"
                      onClick={() =>
                        void ipc
                          .deleteConnection(c.id)
                          .then(() => qc.invalidateQueries({ queryKey: keys.saved }))
                      }
                    >
                      <Trash2 size={14} />
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="p-4 text-[12px] text-fg-dim">
                No saved connections. Passwords are never written to disk; saving stores them in
                your OS keyring.
              </p>
            )}
          </Panel>
        </aside>
      </div>
    </div>
  );
}
