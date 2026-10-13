/** Mirrors the normalized Rust model (serde camelCase). Keep in sync with src-tauri/src/metadata. */

export type Engine = "postgres" | "mysql" | "sqlite" | "generic";
export type SslMode = "disable" | "prefer" | "require" | "verifyca" | "verifyfull";
export type StableId = string;
export type EngineMeta = Record<string, unknown>;

export interface Namespace {
  stableId: StableId;
  name: string;
  isSystem: boolean;
  engineSpecificMetadata: EngineMeta;
}

export interface Column {
  stableId: StableId;
  name: string;
  namespace: string;
  ordinal: number;
  dataType: string;
  nativeType: string;
  nullable: boolean;
  default: string | null;
  generated: string | null;
  identity: string | null;
  comment: string | null;
  engineSpecificMetadata: EngineMeta;
}

export interface PrimaryKey {
  stableId: StableId;
  name: string;
  namespace: string;
  columns: string[];
}

export type ReferentialAction = "no_action" | "restrict" | "cascade" | "set_null" | "set_default";

export interface ForeignKey {
  stableId: StableId;
  name: string;
  namespace: string;
  sourceTable: StableId;
  sourceColumns: string[];
  targetTable: StableId;
  targetColumns: string[];
  onDelete: ReferentialAction;
  onUpdate: ReferentialAction;
}

export interface UniqueConstraint {
  stableId: StableId;
  name: string;
  namespace: string;
  columns: string[];
}

export interface CheckConstraint {
  stableId: StableId;
  name: string;
  namespace: string;
  expression: string;
}

export interface DbIndex {
  stableId: StableId;
  name: string;
  namespace: string;
  table: StableId;
  columns: string[];
  unique: boolean;
  primary: boolean;
  method: string | null;
  predicate: string | null;
  expressions: string[];
  includedColumns: string[];
  definition: string | null;
  engineSpecificMetadata: EngineMeta;
}

export interface Table {
  stableId: StableId;
  name: string;
  namespace: string;
  comment: string | null;
  estimatedRows: number | null;
  sizeBytes: number | null;
  columns: Column[];
  primaryKey: PrimaryKey | null;
  foreignKeys: ForeignKey[];
  uniqueConstraints: UniqueConstraint[];
  checkConstraints: CheckConstraint[];
  indexes: DbIndex[];
  engineSpecificMetadata: EngineMeta;
}

export interface View {
  stableId: StableId;
  name: string;
  namespace: string;
  materialized: boolean;
  definition: string | null;
  columns: Column[];
  comment: string | null;
}

export interface Trigger {
  stableId: StableId;
  name: string;
  namespace: string;
  table: StableId;
  timing: string;
  events: string[];
  definition: string | null;
  function: StableId | null;
}

export interface Routine {
  stableId: StableId;
  name: string;
  namespace: string;
  kind: "function" | "procedure";
  language: string | null;
  arguments: string;
  returnType: string | null;
  definition: string | null;
}

export interface Sequence {
  stableId: StableId;
  name: string;
  namespace: string;
  dataType: string | null;
  ownedBy: StableId | null;
}

export type DependencyKind = "FOREIGN_KEY" | "DEPENDS_ON" | "DERIVES_FROM" | "TRIGGER_DEPENDENCY";

export interface Dependency {
  from: StableId;
  to: StableId;
  kind: DependencyKind;
}

export interface DatabaseSchema {
  engine: Engine;
  database: string;
  serverVersion: string | null;
  namespaces: Namespace[];
  tables: Table[];
  views: View[];
  triggers: Trigger[];
  routines: Routine[];
  sequences: Sequence[];
  dependencies: Dependency[];
  warnings: string[];
}

export interface SchemaPayload {
  schema: DatabaseSchema;
  fetchedAt: string;
  fingerprint: string;
}

export interface ConnectionConfig {
  engine: Engine;
  host: string | null;
  port: number | null;
  database: string | null;
  username: string | null;
  sslMode: SslMode;
  filePath: string | null;
}

export interface SavedConnection {
  id: string;
  name: string;
  config: ConnectionConfig;
  createdAt: string;
  updatedAt: string;
}

export interface ActiveInfo {
  connectionName: string;
  connectionId: string | null;
  engine: Engine;
  database: string;
  serverVersion: string;
  readOnly: boolean;
  /** Workspace opened from imported SQL: no live database behind it. */
  imported: boolean;
}

export interface ConnectionProbe {
  engine: Engine;
  serverVersion: string;
  latencyMs: number;
  readOnly: boolean;
}

export type Evidence = "observed_fact" | "heuristic" | "suggestion" | "unknown";
export type Severity = "info" | "low" | "medium" | "high";
export type ObjectKind = "table" | "view" | "materialized_view" | "routine" | "trigger";

export interface GraphNode {
  id: StableId;
  name: string;
  namespace: string;
  kind: ObjectKind;
}
export interface GraphEdge {
  from: StableId;
  to: StableId;
  kind: DependencyKind;
}
export interface GraphView {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface ImpactItem {
  id: StableId;
  name: string;
  namespace: string;
  kind: ObjectKind;
  reach: "DIRECT" | "TRANSITIVE";
  relation: DependencyKind;
  depth: number;
  via: StableId;
  evidence: Evidence;
}

export interface ImpactReport {
  target: StableId;
  targetName: string;
  targetKind: string;
  column: string | null;
  level: "none" | "low" | "medium" | "high";
  levelBasis: string;
  direct: ImpactItem[];
  transitive: ImpactItem[];
  affectedIndexes: string[];
  counts: {
    direct: number;
    transitive: number;
    foreignKeys: number;
    tables: number;
    views: number;
    materializedViews: number;
    triggers: number;
    routines: number;
  };
  notes: string[];
}

export type AuditCategory =
  "integrity" | "performance" | "maintainability" | "complexity" | "documentation";

export interface Finding {
  ruleId: string;
  severity: Severity;
  category: AuditCategory;
  evidence: Evidence;
  object: StableId;
  objectName: string;
  message: string;
}

export interface RuleInfo {
  id: string;
  title: string;
  description: string;
  severity: Severity;
  category: AuditCategory;
  evidence: Evidence;
}

export interface HealthReport {
  overall: number | null;
  categories: {
    category: AuditCategory;
    score: number;
    penaltyTotal: number;
    penalties: { ruleId: string; object: StableId; objectName: string; points: number }[];
  }[];
  budget: number;
  method: string;
}

export interface AuditReport {
  findings: Finding[];
  health: HealthReport;
  rules: RuleInfo[];
}

export interface IndexReport {
  missingFkIndexes: {
    table: StableId;
    tableName: string;
    foreignKey: StableId;
    foreignKeyName: string;
    columns: string[];
    evidence: Evidence;
  }[];
  redundant: {
    kind: "duplicate" | "overlapping";
    table: StableId;
    index: StableId;
    indexName: string;
    coveredBy: StableId;
    coveredByName: string;
    evidence: Evidence;
    explanation: string;
  }[];
  suggestions: {
    title: string;
    table: StableId;
    sql: string;
    rationale: string;
    evidence: Evidence;
  }[];
}

export interface AppError {
  kind: string;
  message: string;
}

// ---- SQL import, query execution, plans, versioning ----

export interface ImportWarning {
  statementNo: number;
  statement: string;
  reason: string;
}

export interface ImportResult {
  schema: DatabaseSchema;
  status: "parsed" | "parsed_with_warnings";
  statements: number;
  understood: number;
  warnings: ImportWarning[];
  warningsTruncated: boolean;
}

export interface ResultColumn {
  name: string;
  typeName: string;
}

export interface QueryResult {
  columns: ResultColumn[];
  rows: unknown[][];
  rowCount: number;
  truncated: boolean;
  limit: number;
  elapsedMs: number;
}

export interface PlanNode {
  id: number;
  nodeType: string;
  relation: string | null;
  index: string | null;
  filter: string | null;
  estimatedCost: number | null;
  startupCost: number | null;
  estimatedRows: number | null;
  details: [string, string][];
  children: PlanNode[];
}

export interface QueryPlan {
  engine: Engine;
  root: PlanNode;
  totalCost: number | null;
  observations: { nodeId: number; severity: Severity; evidence: Evidence; message: string }[];
  notes: string[];
}

export interface SnapshotRow {
  id: string;
  name: string;
  connectionId: string | null;
  sha256: string;
  createdAt: string;
  tables: number;
  views: number;
  engine: string;
  database: string;
}

export type ChangeKind = "added" | "removed" | "changed";

export interface DiffEntry {
  id: StableId;
  objectType: string;
  name: string;
  parent: StableId | null;
  change: ChangeKind;
  fields: { field: string; before: unknown; after: unknown }[];
}

export interface SchemaDiff {
  entries: DiffEntry[];
  summary: { added: number; removed: number; changed: number };
}

export interface DiffReport {
  beforeLabel: string;
  afterLabel: string;
  beforeSha: string;
  afterSha: string;
  identical: boolean;
  diff: SchemaDiff;
}

export interface MigrationReport {
  statements: {
    statementNo: number;
    sql: string;
    status: "applied" | "ignored" | "failed";
    message: string;
  }[];
  diff: SchemaDiff;
  risks: {
    kind: "structural" | "runtime" | "application";
    severity: Severity;
    evidence: Evidence;
    message: string;
    object: StableId | null;
  }[];
  simulated: DatabaseSchema;
  notes: string[];
}

export type ColumnClass = "DIRECT" | "DERIVED" | "AGGREGATED" | "UNKNOWN";

export interface LineageReport {
  tables: {
    view: StableId;
    viewName: string;
    sources: { id: StableId; name: string; origin: "catalog" | "parser" | "both" }[];
    confidence: "complete" | "partial";
    notes: string[];
  };
  columns: {
    view: StableId;
    viewName: string;
    confidence: "complete" | "partial";
    notes: string[];
    columns: {
      column: string;
      class: ColumnClass;
      sources: { table: StableId; tableName: string; column: string }[];
      expression: string | null;
      note: string | null;
    }[];
  };
}

export interface TableCentrality {
  table: StableId;
  name: string;
  referencedBy: number;
  references: number;
  pathShare: number;
  role: "hub" | "bridge" | "regular" | "isolated";
  explanation: string;
}
