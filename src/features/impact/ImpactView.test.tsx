import { screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { sampleSchema } from "@/lib/testSchema";
import { useUi } from "@/stores/ui";
import { renderWithQuery } from "@/test/helpers";
import type { ImpactReport } from "@/types/schema";
import { ImpactView } from "./ImpactView";

const report: ImpactReport = {
  target: "postgres:public.users",
  targetName: "users",
  targetKind: "table",
  column: null,
  level: "medium",
  levelBasis:
    "Potential impact by number of dependent database objects: 0 none, 1-3 low, 4-9 medium, 10+ high.",
  direct: [
    {
      id: "postgres:public.orders",
      name: "orders",
      namespace: "public",
      kind: "table",
      reach: "DIRECT",
      relation: "FOREIGN_KEY",
      depth: 1,
      via: "postgres:public.users",
      evidence: "observed_fact",
    },
  ],
  transitive: [
    {
      id: "postgres:view:public.user_orders",
      name: "user_orders",
      namespace: "public",
      kind: "view",
      reach: "TRANSITIVE",
      relation: "DEPENDS_ON",
      depth: 2,
      via: "postgres:public.orders",
      evidence: "observed_fact",
    },
  ],
  affectedIndexes: [],
  counts: {
    direct: 1,
    transitive: 1,
    foreignKeys: 1,
    tables: 1,
    views: 1,
    materializedViews: 0,
    triggers: 0,
    routines: 0,
  },
  notes: [
    "Only database objects are analyzed. Application code, ORMs, reports and other external consumers are not visible to DB-BUTURE.",
  ],
};

const analyzeImpact = vi.fn();
vi.mock("@/lib/ipc", async (orig) => ({
  ...(await orig<typeof import("@/lib/ipc")>()),
  ipc: { analyzeImpact: (id: string) => analyzeImpact(id) },
}));
vi.mock("@/hooks/queries", () => ({
  useSchema: () => ({
    data: { schema: sampleSchema(), fetchedAt: "t", fingerprint: "x" },
    isLoading: false,
  }),
}));
vi.mock("@/components/MiniGraph", () => ({
  MiniGraph: () => <div data-testid="graph" />,
  EdgeLegend: () => null,
}));

describe("ImpactView", () => {
  beforeEach(() => {
    useUi.getState().reset();
    analyzeImpact.mockReset();
  });

  it("asks the user to choose an object first", () => {
    renderWithQuery(<ImpactView />);
    expect(screen.getByText(/Choose an object/i)).toBeInTheDocument();
    expect(analyzeImpact).not.toHaveBeenCalled();
  });

  it("separates DIRECT from TRANSITIVE and states the limits of the analysis", async () => {
    analyzeImpact.mockResolvedValue(report);
    useUi.getState().select("postgres:public.users");
    renderWithQuery(<ImpactView />);
    await waitFor(() => expect(screen.getByText("POTENTIAL IMPACT")).toBeInTheDocument());
    expect(analyzeImpact).toHaveBeenCalledWith("postgres:public.users");
    expect(screen.getAllByText("medium").length).toBeGreaterThan(0);
    expect(screen.getByText(/DIRECT · 1/)).toBeInTheDocument();
    expect(screen.getByText(/TRANSITIVE · 1/)).toBeInTheDocument();
    expect(screen.getAllByText("observed fact").length).toBe(2);
    expect(screen.getByText(/Application code, ORMs, reports/)).toBeInTheDocument();
    expect(screen.getByTestId("graph")).toBeInTheDocument();
  });

  it("surfaces backend errors", async () => {
    analyzeImpact.mockRejectedValue({
      kind: "validation",
      message: "object not found in the loaded schema",
    });
    useUi.getState().select("postgres:public.users");
    renderWithQuery(<ImpactView />);
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("object not found"));
  });
});
