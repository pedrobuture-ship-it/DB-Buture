import { fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { sampleSchema } from "@/lib/testSchema";
import { useUi } from "@/stores/ui";
import { renderWithQuery } from "@/test/helpers";
import { Explorer } from "./Explorer";

vi.mock("@/hooks/queries", () => ({
  useSchema: () => ({
    data: { schema: sampleSchema(), fetchedAt: "2026-01-01T00:00:00Z", fingerprint: "x" },
    isLoading: false,
    error: null,
  }),
}));

describe("Explorer", () => {
  beforeEach(() => useUi.getState().reset());

  it("lists the namespace and expands tables on click", () => {
    renderWithQuery(<Explorer />);
    const tree = screen.getByRole("tree");
    expect(within(tree).getByText("public")).toBeInTheDocument();
    expect(within(tree).getByText("users")).toBeInTheDocument();
    fireEvent.click(within(tree).getByText("users"));
    expect(useUi.getState().selectedId).toBe("postgres:public.users");
    expect(within(tree).getByText("Columns")).toBeInTheDocument();
  });

  it("filters the tree with fuzzy search and reports empty results", () => {
    renderWithQuery(<Explorer />);
    const filter = screen.getByLabelText("Filter objects");
    fireEvent.change(filter, { target: { value: "email" } });
    // only the column that matches, plus the ancestors that keep its path visible
    expect(within(screen.getByRole("tree")).queryByText("orders")).toBeNull();
    expect(within(screen.getByRole("tree")).getByText("users")).toBeInTheDocument();
    expect(within(screen.getByRole("tree")).getByText("email")).toBeInTheDocument();
    fireEvent.change(filter, { target: { value: "zzzzzz" } });
    expect(screen.getByText(/No matching objects/i)).toBeInTheDocument();
  });

  it("shows the table inspector with keys and a clickable foreign key target", () => {
    renderWithQuery(<Explorer />);
    fireEvent.click(within(screen.getByRole("tree")).getByText("orders"));
    expect(screen.getByRole("tab", { name: /Columns/ })).toBeInTheDocument();
    const target = screen.getByRole("button", { name: /public\.users\.id/ });
    fireEvent.click(target);
    expect(useUi.getState().selectedId).toBe("postgres:public.users");
  });

  it("separates inspector tabs", () => {
    renderWithQuery(<Explorer />);
    fireEvent.click(within(screen.getByRole("tree")).getByText("orders"));
    fireEvent.click(screen.getByRole("tab", { name: /Relationships/ }));
    expect(screen.getByText(/references \(outgoing\)/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: /Raw/ }));
    expect(screen.getByText(/"stableId": "postgres:public.orders"/)).toBeInTheDocument();
  });
});
