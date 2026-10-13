import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { QueryPlan } from "@/types/schema";
import { PlanView } from "./PlanView";

const plan: QueryPlan = {
  engine: "postgres",
  totalCost: 120.5,
  notes: [
    "Estimated plan only (no ANALYZE): costs and row counts come from planner statistics, not from running the query.",
  ],
  observations: [
    {
      nodeId: 2,
      severity: "medium",
      evidence: "heuristic",
      message: "Full scan over shop.orders (~120000 estimated rows). An index might help.",
    },
  ],
  root: {
    id: 1,
    nodeType: "Hash Join",
    relation: null,
    index: null,
    filter: "(o.user_id = u.id)",
    estimatedCost: 120.5,
    startupCost: 1,
    estimatedRows: 25000,
    details: [],
    children: [
      {
        id: 2,
        nodeType: "Seq Scan",
        relation: "shop.orders",
        index: null,
        filter: "(total > 10)",
        estimatedCost: 90,
        startupCost: 0,
        estimatedRows: 120000,
        details: [],
        children: [],
      },
      {
        id: 3,
        nodeType: "Index Scan",
        relation: "shop.users",
        index: "users_pkey",
        filter: null,
        estimatedCost: 8,
        startupCost: 0,
        estimatedRows: 100,
        details: [],
        children: [],
      },
    ],
  },
};

describe("PlanView", () => {
  it("renders the plan tree with cost, rows, filter, index and relation", () => {
    render(<PlanView plan={plan} />);
    expect(screen.getAllByTestId("plan-node")).toHaveLength(3);
    expect(screen.getByText("Seq Scan")).toBeInTheDocument();
    expect(screen.getByText("shop.orders")).toBeInTheDocument();
    expect(screen.getByText("filter: (total > 10)")).toBeInTheDocument();
    expect(screen.getByText("index users_pkey")).toBeInTheDocument();
    expect(screen.getByText("cost 120.50")).toBeInTheDocument();
    expect(screen.getByText(/rows ≈ 120,000/)).toBeInTheDocument();
    expect(screen.getByText(/total cost 120.50/)).toBeInTheDocument();
  });

  it("labels observations as heuristics and states that the plan is an estimate", () => {
    render(<PlanView plan={plan} />);
    expect(screen.getByText("heuristic")).toBeInTheDocument();
    expect(screen.getByText(/An index might help/)).toBeInTheDocument();
    expect(screen.getByText(/no ANALYZE/)).toBeInTheDocument();
  });

  it("does not over-claim when there is nothing to observe", () => {
    render(<PlanView plan={{ ...plan, observations: [] }} />);
    expect(screen.getByText(/not a guarantee/)).toBeInTheDocument();
  });
});
