import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DiffReport, SchemaDiff } from "@/types/schema";
import { DiffList, DiffReportView, DiffSummary } from "./DiffList";

const diff: SchemaDiff = {
  summary: { added: 1, removed: 1, changed: 1 },
  entries: [
    {
      id: "a",
      objectType: "column",
      name: "nickname",
      parent: "postgres:public.users",
      change: "added",
      fields: [],
    },
    { id: "b", objectType: "table", name: "legacy", parent: null, change: "removed", fields: [] },
    {
      id: "c",
      objectType: "column",
      name: "total",
      parent: "postgres:public.orders",
      change: "changed",
      fields: [{ field: "type", before: "numeric(12,2)", after: "numeric(14,2)" }],
    },
  ],
};

describe("DiffList", () => {
  it("shows BEFORE and AFTER values for changed fields", () => {
    render(<DiffList diff={diff} />);
    expect(screen.getByText("numeric(12,2)")).toBeInTheDocument();
    expect(screen.getByText("numeric(14,2)")).toBeInTheDocument();
    expect(screen.getByText("before")).toBeInTheDocument();
    expect(screen.getByText("after")).toBeInTheDocument();
  });

  it("labels each change kind", () => {
    render(<DiffList diff={diff} />);
    expect(screen.getByText("added")).toBeInTheDocument();
    expect(screen.getByText("removed")).toBeInTheDocument();
    expect(screen.getByText("changed")).toBeInTheDocument();
  });

  it("summarizes counts and handles an empty diff", () => {
    const { rerender } = render(<DiffSummary diff={diff} />);
    expect(screen.getByTestId("diff-summary")).toHaveTextContent("+1 added");
    expect(screen.getByTestId("diff-summary")).toHaveTextContent("−1 removed");
    rerender(<DiffList diff={{ entries: [], summary: { added: 0, removed: 0, changed: 0 } }} />);
    expect(screen.getByText(/No structural differences/)).toBeInTheDocument();
  });

  it("report view says IDENTICAL STRUCTURE when hashes match", () => {
    const report: DiffReport = {
      beforeLabel: "snapshot “a”",
      afterLabel: "current",
      beforeSha: "a".repeat(64),
      afterSha: "a".repeat(64),
      identical: true,
      diff: { entries: [], summary: { added: 0, removed: 0, changed: 0 } },
    };
    render(<DiffReportView report={report} />);
    expect(screen.getByText("IDENTICAL STRUCTURE")).toBeInTheDocument();
    expect(screen.getByText(/snapshot “a”/)).toBeInTheDocument();
  });
});
