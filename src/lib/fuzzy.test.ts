import { fuzzyFilter, fuzzyScore } from "./fuzzy";

describe("fuzzy", () => {
  it("matches subsequences case-insensitively", () => {
    expect(fuzzyScore("usr", "Users")).not.toBeNull();
    expect(fuzzyScore("xyz", "users")).toBeNull();
    expect(fuzzyScore("", "anything")).toBe(0);
  });

  it("prefers prefixes, boundaries and exact matches", () => {
    const exact = fuzzyScore("users", "users") ?? 0;
    const prefix = fuzzyScore("user", "users") ?? 0;
    const scattered = fuzzyScore("user", "u_s_e_r_x") ?? 0;
    expect(exact).toBeGreaterThan(prefix);
    expect(prefix).toBeGreaterThan(scattered);
  });

  it("ranks results", () => {
    const out = fuzzyFilter(["order_items", "orders", "users", "audit_orders"], "ord", (s) => s);
    expect(out[0]).toBe("orders");
    expect(out).not.toContain("users");
  });

  it("returns everything for an empty query", () => {
    expect(fuzzyFilter(["a", "b"], " ", (s) => s)).toEqual(["a", "b"]);
  });
});
