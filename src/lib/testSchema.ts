/** Small fixture used only by tests. */
import type { Column, DatabaseSchema, Table } from "@/types/schema";

export function col(
  table: string,
  name: string,
  ordinal: number,
  type = "bigint",
  nullable = false,
): Column {
  return {
    stableId: `postgres:column:public.${table}:${name}`,
    name,
    namespace: "public",
    ordinal,
    dataType: type,
    nativeType: type,
    nullable,
    default: null,
    generated: null,
    identity: null,
    comment: null,
    engineSpecificMetadata: {},
  };
}

export function table(name: string, columns: Column[], extra: Partial<Table> = {}): Table {
  return {
    stableId: `postgres:public.${name}`,
    name,
    namespace: "public",
    comment: null,
    estimatedRows: null,
    sizeBytes: null,
    columns,
    primaryKey: null,
    foreignKeys: [],
    uniqueConstraints: [],
    checkConstraints: [],
    indexes: [],
    engineSpecificMetadata: {},
    ...extra,
  };
}

export function sampleSchema(): DatabaseSchema {
  const users = table("users", [col("users", "id", 1), col("users", "email", 2, "varchar")], {
    primaryKey: {
      stableId: "postgres:constraint:public.users:pk",
      name: "users_pkey",
      namespace: "public",
      columns: ["id"],
    },
    uniqueConstraints: [
      {
        stableId: "postgres:constraint:public.users:uq",
        name: "users_email_key",
        namespace: "public",
        columns: ["email"],
      },
    ],
  });
  const orders = table(
    "orders",
    [col("orders", "id", 1), col("orders", "user_id", 2, "bigint", true)],
    {
      primaryKey: {
        stableId: "postgres:constraint:public.orders:pk",
        name: "orders_pkey",
        namespace: "public",
        columns: ["id"],
      },
      foreignKeys: [
        {
          stableId: "postgres:constraint:public.orders:fk",
          name: "orders_user_fk",
          namespace: "public",
          sourceTable: "postgres:public.orders",
          sourceColumns: ["user_id"],
          targetTable: "postgres:public.users",
          targetColumns: ["id"],
          onDelete: "cascade",
          onUpdate: "no_action",
        },
      ],
    },
  );
  return {
    engine: "postgres",
    database: "app",
    serverVersion: "17.2",
    namespaces: [
      {
        stableId: "postgres:namespace:public",
        name: "public",
        isSystem: false,
        engineSpecificMetadata: {},
      },
    ],
    tables: [orders, users],
    views: [
      {
        stableId: "postgres:view:public.user_orders",
        name: "user_orders",
        namespace: "public",
        materialized: false,
        definition: "SELECT 1",
        columns: [col("user_orders", "user_id", 1)],
        comment: null,
      },
    ],
    triggers: [],
    routines: [],
    sequences: [],
    dependencies: [
      { from: "postgres:public.orders", to: "postgres:public.users", kind: "FOREIGN_KEY" },
      {
        from: "postgres:view:public.user_orders",
        to: "postgres:public.orders",
        kind: "DEPENDS_ON",
      },
    ],
    warnings: [],
  };
}
