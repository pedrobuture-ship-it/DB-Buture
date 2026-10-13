import { buildSchemaIndex, columnMarkers, indexSignature } from "./schemaIndex";
import { sampleSchema } from "./testSchema";

describe("schemaIndex", () => {
  const schema = sampleSchema();
  const idx = buildSchemaIndex(schema);

  it("indexes tables, views and columns", () => {
    expect(idx.tables.size).toBe(2);
    expect(idx.views.size).toBe(1);
    expect(idx.columns.get("postgres:column:public.users:email")?.parent.name).toBe("users");
    expect(idx.names.get("postgres:public.users")).toBe("public.users");
  });

  it("collects incoming foreign keys", () => {
    expect(idx.incomingFks.get("postgres:public.users")?.[0]?.name).toBe("orders_user_fk");
    expect(idx.incomingFks.get("postgres:public.orders")).toBeUndefined();
  });

  it("derives column markers", () => {
    const users = schema.tables.find((t) => t.name === "users");
    const orders = schema.tables.find((t) => t.name === "orders");
    if (!users || !orders) throw new Error("fixture missing tables");
    expect(columnMarkers(users, "id")).toEqual(["PK"]);
    expect(columnMarkers(users, "email")).toEqual(["UQ"]);
    expect(columnMarkers(orders, "user_id")).toEqual(["FK"]);
  });

  it("describes an index", () => {
    expect(
      indexSignature({
        stableId: "x",
        name: "i",
        namespace: "public",
        table: "t",
        columns: ["a"],
        unique: true,
        method: "btree",
        primary: false,
        predicate: null,
        expressions: ["lower(b)"],
        includedColumns: [],
        definition: null,
        engineSpecificMetadata: {},
      }),
    ).toBe("btree unique (a, (lower(b)))");
  });
});
