import { ArrowRight, Radar } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import { Badge, Button, EmptyState, Label, Mono, cx } from "@/components/ui";
import { formatBytes, formatCount } from "@/lib/format";
import { columnMarkers, indexSignature, tableName, type SchemaIndex } from "@/lib/schemaIndex";
import { useUi } from "@/stores/ui";
import type { DatabaseSchema, StableId, Table, View } from "@/types/schema";

type Tab =
  | "columns"
  | "constraints"
  | "indexes"
  | "relationships"
  | "dependencies"
  | "stats"
  | "raw"
  | "definition";

export function Inspector({
  schema,
  index,
  selectedId,
}: {
  schema: DatabaseSchema;
  index: SchemaIndex;
  selectedId: StableId | null;
}) {
  const resolved = useMemo(() => {
    if (!selectedId) return null;
    const t = index.tables.get(selectedId);
    if (t) return { object: t, column: null as string | null };
    const v = index.views.get(selectedId);
    if (v) return { object: v, column: null };
    const c = index.columns.get(selectedId);
    if (c) return { object: c.parent, column: c.column.name };
    const owner = index.owners.get(selectedId);
    if (owner) return { object: owner, column: null };
    return null;
  }, [selectedId, index]);

  if (!resolved) {
    return (
      <EmptyState title="Select an object">
        Pick a table or view in the tree to inspect its columns, constraints, indexes and
        relationships.
      </EmptyState>
    );
  }
  const isTable = "foreignKeys" in resolved.object;
  return isTable ? (
    <TableView
      key={resolved.object.stableId}
      table={resolved.object as Table}
      schema={schema}
      index={index}
      highlight={resolved.column}
    />
  ) : (
    <ViewView
      key={resolved.object.stableId}
      view={resolved.object as View}
      schema={schema}
      index={index}
      highlight={resolved.column}
    />
  );
}

function Tabs<T extends string>({
  tabs,
  value,
  onChange,
}: {
  tabs: { id: T; label: string; count?: number }[];
  value: T;
  onChange: (t: T) => void;
}) {
  return (
    <div role="tablist" className="flex gap-0 border-b border-line">
      {tabs.map((t) => (
        <button
          key={t.id}
          role="tab"
          aria-selected={value === t.id}
          type="button"
          onClick={() => onChange(t.id)}
          className={cx(
            "relative h-8 px-3 font-mono text-[11px] uppercase tracking-wider transition-colors",
            value === t.id ? "text-cyan" : "text-fg-dim hover:text-fg",
          )}
        >
          {t.label}
          {t.count != null && <span className="ml-1.5 text-fg-dim">{t.count}</span>}
          {value === t.id && <span className="absolute inset-x-0 bottom-0 h-px bg-cyan" />}
        </button>
      ))}
    </div>
  );
}

function Header({
  title,
  subtitle,
  kind,
  extra,
}: {
  title: string;
  subtitle?: string | null;
  kind: string;
  extra?: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-line bg-ink-1/70 px-5 py-3">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <Badge tone="cyan">{kind}</Badge>
          <h2 className="truncate font-mono text-[15px] font-medium">{title}</h2>
        </div>
        {subtitle && <p className="mt-1 text-[12px] text-fg-muted">{subtitle}</p>}
      </div>
      {extra}
    </div>
  );
}

function ImpactButton({ id }: { id: StableId }) {
  const select = useUi((s) => s.select);
  return (
    <Button onClick={() => select(id, "impact")}>
      <Radar size={13} /> Impact
    </Button>
  );
}

function Th({ children, right }: { children: ReactNode; right?: boolean }) {
  return (
    <th className={cx("px-3 py-1.5 text-left font-normal", right && "text-right")}>
      <Label>{children}</Label>
    </th>
  );
}

function TableView({
  table,
  schema,
  index,
  highlight,
}: {
  table: Table;
  schema: DatabaseSchema;
  index: SchemaIndex;
  highlight: string | null;
}) {
  const [tab, setTab] = useState<Tab>("columns");
  const select = useUi((s) => s.select);
  const incoming = index.incomingFks.get(table.stableId) ?? [];
  const deps = schema.dependencies.filter((d) => d.from === table.stableId);
  const dependents = schema.dependencies.filter((d) => d.to === table.stableId);
  const constraintCount =
    (table.primaryKey ? 1 : 0) +
    table.foreignKeys.length +
    table.uniqueConstraints.length +
    table.checkConstraints.length;

  return (
    <div className="fade-in">
      <Header
        title={tableName(table)}
        kind="table"
        subtitle={table.comment}
        extra={
          <ImpactButton
            id={
              highlight
                ? (table.columns.find((c) => c.name === highlight)?.stableId ?? table.stableId)
                : table.stableId
            }
          />
        }
      />
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "columns", label: "Columns", count: table.columns.length },
          { id: "constraints", label: "Constraints", count: constraintCount },
          { id: "indexes", label: "Indexes", count: table.indexes.length },
          {
            id: "relationships",
            label: "Relationships",
            count: table.foreignKeys.length + incoming.length,
          },
          { id: "dependencies", label: "Dependencies", count: deps.length + dependents.length },
          { id: "stats", label: "Stats" },
          { id: "raw", label: "Raw" },
        ]}
      />
      <div className="p-5">
        {tab === "columns" && (
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-line">
                <Th>key</Th>
                <Th>name</Th>
                <Th>type</Th>
                <Th>null</Th>
                <Th>default</Th>
                <Th>comment</Th>
              </tr>
            </thead>
            <tbody>
              {table.columns.map((c) => {
                const fk = table.foreignKeys.find((f) => f.sourceColumns.includes(c.name));
                return (
                  <tr
                    key={c.stableId}
                    className={cx(
                      "border-b border-line/60",
                      highlight === c.name && "bg-cyan-wash",
                    )}
                  >
                    <td className="w-16 px-3 py-1.5">
                      <span className="flex gap-1">
                        {columnMarkers(table, c.name).map((m) => (
                          <Badge
                            key={m}
                            tone={m === "PK" ? "amber" : m === "FK" ? "cyan" : "neutral"}
                          >
                            {m}
                          </Badge>
                        ))}
                      </span>
                    </td>
                    <td className="px-3 py-1.5 font-mono">{c.name}</td>
                    <td className="px-3 py-1.5 font-mono text-fg-muted" title={c.nativeType}>
                      {c.nativeType}
                      {fk && (
                        <button
                          type="button"
                          onClick={() => select(fk.targetTable, "explorer")}
                          className="ml-2 inline-flex items-center gap-1 text-cyan hover:underline"
                        >
                          <ArrowRight size={11} />
                          {index.names.get(fk.targetTable)}.
                          {fk.targetColumns[fk.sourceColumns.indexOf(c.name)]}
                        </button>
                      )}
                    </td>
                    <td className="px-3 py-1.5 font-mono text-[11px]">
                      {c.nullable ? <span className="text-fg-dim">NULL</span> : "NOT NULL"}
                    </td>
                    <td className="px-3 py-1.5 font-mono text-[11px] text-fg-muted">
                      {c.identity
                        ? `identity (${c.identity})`
                        : c.generated
                          ? `generated (${c.generated})`
                          : (c.default ?? "")}
                    </td>
                    <td className="px-3 py-1.5 text-fg-muted">{c.comment}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}

        {tab === "constraints" && (
          <div className="space-y-2">
            {constraintCount === 0 && <p className="text-fg-dim">No constraints.</p>}
            {table.primaryKey && (
              <ConstraintRow
                tone="amber"
                kind="PRIMARY KEY"
                name={table.primaryKey.name}
                detail={`(${table.primaryKey.columns.join(", ")})`}
              />
            )}
            {table.foreignKeys.map((f) => (
              <ConstraintRow
                key={f.stableId}
                tone="cyan"
                kind="FOREIGN KEY"
                name={f.name}
                detail={`(${f.sourceColumns.join(", ")}) → ${index.names.get(f.targetTable)} (${f.targetColumns.join(", ")})  ON DELETE ${f.onDelete.replace("_", " ")} · ON UPDATE ${f.onUpdate.replace("_", " ")}`}
              />
            ))}
            {table.uniqueConstraints.map((u) => (
              <ConstraintRow
                key={u.stableId}
                tone="neutral"
                kind="UNIQUE"
                name={u.name}
                detail={`(${u.columns.join(", ")})`}
              />
            ))}
            {table.checkConstraints.map((c) => (
              <ConstraintRow
                key={c.stableId}
                tone="neutral"
                kind="CHECK"
                name={c.name}
                detail={c.expression}
              />
            ))}
          </div>
        )}

        {tab === "indexes" &&
          (table.indexes.length === 0 ? (
            <p className="text-fg-dim">No indexes.</p>
          ) : (
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-line">
                  <Th>name</Th>
                  <Th>definition</Th>
                  <Th>predicate</Th>
                </tr>
              </thead>
              <tbody>
                {table.indexes.map((i) => (
                  <tr key={i.stableId} className="border-b border-line/60">
                    <td className="px-3 py-1.5 font-mono">
                      {i.name} {i.primary && <Badge tone="amber">PK</Badge>}{" "}
                      {i.unique && !i.primary && <Badge>UNIQUE</Badge>}
                      {i.engineSpecificMetadata.valid === false && (
                        <Badge tone="red">INVALID</Badge>
                      )}
                    </td>
                    <td className="px-3 py-1.5 font-mono text-fg-muted">
                      {indexSignature(i)}
                      {i.includedColumns.length > 0 && (
                        <span className="text-fg-dim">
                          {" "}
                          include ({i.includedColumns.join(", ")})
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-1.5 font-mono text-[11px] text-fg-muted">
                      {i.predicate ?? ""}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ))}

        {tab === "relationships" && (
          <div className="grid grid-cols-2 gap-6">
            <div>
              <Label>references (outgoing)</Label>
              <ul className="mt-2 space-y-1">
                {table.foreignKeys.length === 0 && <li className="text-fg-dim">None</li>}
                {table.foreignKeys.map((f) => (
                  <li key={f.stableId} className="font-mono text-[12px]">
                    {f.sourceColumns.join(", ")}{" "}
                    <ArrowRight size={11} className="mx-1 inline text-fg-dim" />
                    <button
                      type="button"
                      className="text-cyan hover:underline"
                      onClick={() => select(f.targetTable, "explorer")}
                    >
                      {index.names.get(f.targetTable)}
                    </button>
                    <span className="text-fg-dim"> ({f.targetColumns.join(", ")})</span>
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <Label>referenced by (incoming)</Label>
              <ul className="mt-2 space-y-1">
                {incoming.length === 0 && <li className="text-fg-dim">None</li>}
                {incoming.map((f) => (
                  <li key={f.stableId} className="font-mono text-[12px]">
                    <button
                      type="button"
                      className="text-cyan hover:underline"
                      onClick={() => select(f.sourceTable, "explorer")}
                    >
                      {index.names.get(f.sourceTable)}
                    </button>
                    <span className="text-fg-dim"> ({f.sourceColumns.join(", ")})</span>
                    <ArrowRight size={11} className="mx-1 inline text-fg-dim" />{" "}
                    {f.targetColumns.join(", ")}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}

        {tab === "dependencies" && (
          <DependencyLists
            deps={deps.map((d) => d.to)}
            dependents={dependents.map((d) => d.from)}
            index={index}
          />
        )}

        {tab === "stats" && (
          <dl className="grid max-w-md grid-cols-[160px_1fr] gap-y-2 text-[12px]">
            <dt className="text-fg-dim">Estimated rows</dt>
            <dd className="font-mono">{formatCount(table.estimatedRows)}</dd>
            <dt className="text-fg-dim">Total size</dt>
            <dd className="font-mono">{formatBytes(table.sizeBytes)}</dd>
            <dt className="text-fg-dim">Columns</dt>
            <dd className="font-mono">{table.columns.length}</dd>
            <dt className="text-fg-dim">Indexes</dt>
            <dd className="font-mono">{table.indexes.length}</dd>
            <dt className="text-fg-dim">Foreign keys</dt>
            <dd className="font-mono">{table.foreignKeys.length}</dd>
            <dd className="col-span-2 text-[11px] text-fg-dim">
              Row counts are catalog estimates, not exact counts. “—” means the engine does not
              report it.
            </dd>
          </dl>
        )}

        {tab === "raw" && (
          <pre className="overflow-auto rounded-sm border border-line bg-ink-0 p-3 font-mono text-[11px] text-fg-muted">
            {JSON.stringify(table, null, 2)}
          </pre>
        )}
      </div>
    </div>
  );
}

function ConstraintRow({
  kind,
  name,
  detail,
  tone,
}: {
  kind: string;
  name: string;
  detail: string;
  tone: "amber" | "cyan" | "neutral";
}) {
  return (
    <div className="flex items-baseline gap-3 rounded-sm border border-line bg-ink-1 px-3 py-2">
      <Badge tone={tone}>{kind}</Badge>
      <Mono className="text-fg">{name}</Mono>
      <Mono className="text-fg-muted">{detail}</Mono>
    </div>
  );
}

function DependencyLists({
  deps,
  dependents,
  index,
}: {
  deps: StableId[];
  dependents: StableId[];
  index: SchemaIndex;
}) {
  const select = useUi((s) => s.select);
  const list = (ids: StableId[]) =>
    ids.length === 0 ? (
      <li className="text-fg-dim">None</li>
    ) : (
      ids.map((id) => (
        <li key={id}>
          <button
            type="button"
            className="font-mono text-[12px] text-cyan hover:underline"
            onClick={() => select(id, "explorer")}
          >
            {index.names.get(id) ?? id}
          </button>
        </li>
      ))
    );
  return (
    <div className="grid grid-cols-2 gap-6">
      <div>
        <Label>depends on</Label>
        <ul className="mt-2 space-y-1">{list(deps)}</ul>
      </div>
      <div>
        <Label>depended on by</Label>
        <ul className="mt-2 space-y-1">{list(dependents)}</ul>
      </div>
    </div>
  );
}

function ViewView({
  view,
  schema,
  index,
  highlight,
}: {
  view: View;
  schema: DatabaseSchema;
  index: SchemaIndex;
  highlight: string | null;
}) {
  const [tab, setTab] = useState<Tab>("columns");
  const deps = schema.dependencies.filter((d) => d.from === view.stableId);
  const dependents = schema.dependencies.filter((d) => d.to === view.stableId);
  return (
    <div className="fade-in">
      <Header
        title={tableName(view)}
        kind={view.materialized ? "materialized view" : "view"}
        subtitle={view.comment}
        extra={<ImpactButton id={view.stableId} />}
      />
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "columns", label: "Columns", count: view.columns.length },
          { id: "definition", label: "Definition" },
          { id: "dependencies", label: "Dependencies", count: deps.length + dependents.length },
          { id: "raw", label: "Raw" },
        ]}
      />
      <div className="p-5">
        {tab === "columns" && (
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-line">
                <Th>name</Th>
                <Th>type</Th>
              </tr>
            </thead>
            <tbody>
              {view.columns.map((c) => (
                <tr
                  key={c.stableId}
                  className={cx("border-b border-line/60", highlight === c.name && "bg-cyan-wash")}
                >
                  <td className="px-3 py-1.5 font-mono">{c.name}</td>
                  <td className="px-3 py-1.5 font-mono text-fg-muted">{c.nativeType}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {tab === "definition" &&
          (view.definition ? (
            <pre className="overflow-auto rounded-sm border border-line bg-ink-0 p-3 font-mono text-[12px] leading-relaxed">
              {view.definition}
            </pre>
          ) : (
            <p className="text-fg-dim">The engine did not return a definition.</p>
          ))}
        {tab === "dependencies" && (
          <DependencyLists
            deps={deps.map((d) => d.to)}
            dependents={dependents.map((d) => d.from)}
            index={index}
          />
        )}
        {tab === "raw" && (
          <pre className="overflow-auto rounded-sm border border-line bg-ink-0 p-3 font-mono text-[11px] text-fg-muted">
            {JSON.stringify(view, null, 2)}
          </pre>
        )}
      </div>
    </div>
  );
}
