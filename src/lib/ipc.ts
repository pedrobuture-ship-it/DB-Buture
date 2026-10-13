import { invoke } from "@tauri-apps/api/core";
import type {
  ActiveInfo,
  AppError,
  AuditReport,
  ConnectionConfig,
  ConnectionProbe,
  GraphView,
  ImpactReport,
  IndexReport,
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
  neighborhood: (id: StableId, hops?: number) => call<StableId[]>("neighborhood", { id, hops }),
  findPath: (from: StableId, to: StableId) =>
    call<{ shortest: StableId[] | null; alternatives: StableId[][] }>("find_path", { from, to }),
  analyzeImpact: (objectId: StableId) => call<ImpactReport>("analyze_impact", { objectId }),
  runAudit: () => call<AuditReport>("run_audit"),
  indexAnalysis: () => call<IndexReport>("index_analysis"),
};
