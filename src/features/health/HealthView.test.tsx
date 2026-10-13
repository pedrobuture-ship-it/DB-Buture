import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { sampleSchema } from "@/lib/testSchema";
import { renderWithQuery } from "@/test/helpers";
import type { AuditReport } from "@/types/schema";
import { HealthView } from "./HealthView";

const report: AuditReport = {
  findings: [
    {
      ruleId: "table-without-pk",
      severity: "high",
      category: "integrity",
      evidence: "observed_fact",
      object: "postgres:public.orders",
      objectName: "orders",
      message: "orders has no primary key",
    },
    {
      ruleId: "naming-inconsistency",
      severity: "low",
      category: "maintainability",
      evidence: "heuristic",
      object: "postgres:public.users",
      objectName: "users",
      message: "users does not follow the dominant naming style",
    },
  ],
  rules: [
    {
      id: "table-without-pk",
      title: "Table without primary key",
      description: "d",
      severity: "high",
      category: "integrity",
      evidence: "observed_fact",
    },
  ],
  health: {
    overall: 71,
    budget: 40,
    method: "Each finding costs points by severity.",
    categories: [
      {
        category: "integrity",
        score: 75,
        penaltyTotal: 10,
        penalties: [
          {
            ruleId: "table-without-pk",
            object: "postgres:public.orders",
            objectName: "orders",
            points: 10,
          },
        ],
      },
      { category: "performance", score: 100, penaltyTotal: 0, penalties: [] },
    ],
  },
};
vi.mock("@/lib/ipc", async (orig) => ({
  ...(await orig<typeof import("@/lib/ipc")>()),
  ipc: { runAudit: () => Promise.resolve(report) },
}));
vi.mock("@/hooks/queries", () => ({
  useSchema: () => ({ data: { schema: sampleSchema(), fetchedAt: "t", fingerprint: "x" } }),
}));

describe("HealthView", () => {
  it("shows an explainable score and splits facts from heuristics", async () => {
    renderWithQuery(<HealthView />);
    await waitFor(() => expect(screen.getByText("71")).toBeInTheDocument());
    expect(screen.getByText(/observed facts · 1/)).toBeInTheDocument();
    expect(screen.getByText(/heuristics · 1/)).toBeInTheDocument();
    expect(screen.getByText("orders has no primary key")).toBeInTheDocument();
  });

  it("traces a category penalty back to its rule", async () => {
    renderWithQuery(<HealthView />);
    await waitFor(() => screen.getByText("Integrity"));
    fireEvent.click(screen.getByRole("button", { name: /Integrity/ }));
    expect(screen.getByText("−10")).toBeInTheDocument();
    expect(screen.getAllByText("table-without-pk").length).toBeGreaterThan(1);
  });
});
