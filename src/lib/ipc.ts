import { invoke } from "@tauri-apps/api/core";
import type {
  ActiveInfo,
  AppError,
  AuditReport,
  ConnectionConfig,
  ConnectionProbe,
  DiffReport,
  GraphView,
  ImpactReport,
  ImportResult,
  IndexReport,
  LineageReport,
  MigrationReport,
  QueryPlan,
  QueryResult,
  SnapshotRow,
  TableCentrality,
  DatabaseSchema,
  SavedConnection,
  SchemaPayload,
  StableId,
} from "@/types/schema";

/** Normalizes anything thrown by the IPC layer into the typed AppError shape. */
export function toAppError(err: unknown): AppError {
  if (typeof err === "object" && err !== null && "kind" in err && "message" in err) {
    return err as AppError;
  }
  if (typeof err === "string") return { kind: "internal", message: err };
  if (err instanceof Error) return { kind: "internal", message: err.message };
  return { kind: "internal", message: "Unknown error" };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!("__TAURI_INTERNALS__" in window)) {
    throw {
      kind: "internal",
      message: "DB-BUTURE must run inside the desktop app.",
    } satisfies AppError;
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw toAppError(err);
  }
}

/** Node positions keyed by stable id. `manual` means the user moved something. */
export interface SavedLayout {
  positions: Record<string, { x: number; y: number }>;
  manual: boolean;
}

export type SqlDialect = "postgres" | "mysql" | "sqlite" | "generic";

export interface ConnectInput {
  savedId?: string;
  config?: ConnectionConfig;
  password?: string;
  name?: string;
}

export interface SaveInput {
  id?: string;
  name: string;
  config: ConnectionConfig;
  password?: string;
}

export const ipc = {
  testConnection: (a: { savedId?: string; config?: ConnectionConfig; password?: string }) =>
    call<ConnectionProbe>("test_connection", a),
  saveConnection: (input: SaveInput) => call<SavedConnection>("save_connection", { input }),
  listConnections: () => call<SavedConnection[]>("list_connections"),
  deleteConnection: (id: string) => call<null>("delete_connection", { id }),
  connect: (input: ConnectInput) => call<ActiveInfo>("connect", { input }),
  disconnect: () => call<null>("disconnect"),
  connectionStatus: () => call<ActiveInfo | null>("connection_status"),
  loadSchema: (refresh: boolean, includeSystem: boolean) =>
    call<SchemaPayload>("load_schema", { refresh, includeSystem }),
  dependencyGraph: () => call<GraphView>("dependency_graph"),
  neighborhood: (id: StableId, hops?: number, foreignKeysOnly = false) =>
    call<StableId[]>("neighborhood", { id, hops, foreignKeysOnly }),
  getLayout: (viewKind: string) => call<SavedLayout | null>("get_layout", { viewKind }),
  saveLayout: (viewKind: string, layout: SavedLayout) =>
    call<null>("save_layout", { viewKind, layout }),
  clearLayout: (viewKind: string) => call<null>("clear_layout", { viewKind }),
  findPath: (from: StableId, to: StableId) =>
    call<{ shortest: StableId[] | null; alternatives: StableId[][] }>("find_path", { from, to }),
  analyzeImpact: (objectId: StableId) => call<ImpactReport>("analyze_impact", { objectId }),
  runAudit: () => call<AuditReport>("run_audit"),
  indexAnalysis: () => call<IndexReport>("index_analysis"),
  importSql: (sql: string, dialect: SqlDialect) =>
    call<ImportResult>("import_sql", { sql, dialect }),
  openImport: (sql: string, dialect: SqlDialect, name?: string) =>
    call<ActiveInfo>("open_import", { sql, dialect, name }),
  runQuery: (id: string, sql: string, limit?: number, timeoutMs?: number) =>
    call<QueryResult>("run_query", { id, sql, limit, timeoutMs }),
  cancelQuery: (id: string) => call<boolean>("cancel_query", { id }),
  explainQuery: (sql: string) => call<QueryPlan>("explain_query", { sql }),
  createSnapshot: (name?: string) => call<SnapshotRow>("create_snapshot", { name }),
  listSnapshots: () => call<SnapshotRow[]>("list_snapshots"),
  renameSnapshot: (id: string, name: string) => call<null>("rename_snapshot", { id, name }),
  deleteSnapshot: (id: string) => call<null>("delete_snapshot", { id }),
  getSnapshot: (id: string) =>
    call<{ snapshot: SnapshotRow; schema: DatabaseSchema }>("get_snapshot", { id }),
  exportSnapshot: (id: string) => call<string>("export_snapshot", { id }),
  diffVersions: (before: string, after: string) =>
    call<DiffReport>("diff_versions", { before, after }),
  compareEnvironment: (savedId: string, password?: string) =>
    call<DiffReport>("compare_environment", { input: { savedId, password } }),
  simulateMigration: (ddl: string) => call<MigrationReport>("simulate_migration", { ddl }),
  tableCentrality: () => call<TableCentrality[]>("table_centrality"),
  viewLineage: (viewId: StableId) => call<LineageReport>("view_lineage", { viewId }),
  exportSchema: (format: "json" | "markdown") => call<string>("export_schema", { format }),
  /** Opens a native save dialog (in Rust) and writes the content. Resolves to the path, or null if cancelled. */
  saveExportAs: (suggestedName: string, content: string, base64 = false) =>
    call<string | null>("save_export_as", { suggestedName, content, base64 }),
};
