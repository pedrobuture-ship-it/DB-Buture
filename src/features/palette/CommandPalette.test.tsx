import { act, fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { sampleSchema } from "@/lib/testSchema";
import { useUi } from "@/stores/ui";
import { renderWithQuery } from "@/test/helpers";
import { CommandPalette } from "./CommandPalette";

vi.mock("@/hooks/queries", () => ({
  useSchema: () => ({
    data: { schema: sampleSchema(), fetchedAt: "2026-01-01T00:00:00Z", fingerprint: "x" },
  }),
}));

describe("CommandPalette", () => {
  beforeEach(() => useUi.getState().reset());

  it("opens with Ctrl+K and closes with Escape", () => {
    renderWithQuery(<CommandPalette />);
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("searches tables, columns and navigates to the chosen object", () => {
    renderWithQuery(<CommandPalette />);
    act(() => useUi.getState().setPaletteOpen(true));
    const input = screen.getByLabelText("Search");
    fireEvent.change(input, { target: { value: "email" } });
    const option = screen.getAllByRole("option")[0];
    expect(option).toHaveTextContent("email");
    expect(option).toHaveTextContent("column");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(useUi.getState().selectedId).toBe("postgres:column:public.users:email");
    expect(useUi.getState().view).toBe("explorer");
    expect(useUi.getState().paletteOpen).toBe(false);
  });

  it("offers navigation actions and says so when nothing matches", () => {
    renderWithQuery(<CommandPalette />);
    act(() => useUi.getState().setPaletteOpen(true));
    const input = screen.getByLabelText("Search");
    fireEvent.change(input, { target: { value: "impact" } });
    fireEvent.click(screen.getByText("Go to Impact Analysis"));
    expect(useUi.getState().view).toBe("impact");

    act(() => useUi.getState().setPaletteOpen(true));
    fireEvent.change(screen.getByLabelText("Search"), { target: { value: "qqqqqqzz" } });
    expect(screen.getByText(/No matches/)).toBeInTheDocument();
  });

  it("supports keyboard navigation between results", () => {
    renderWithQuery(<CommandPalette />);
    act(() => useUi.getState().setPaletteOpen(true));
    const input = screen.getByLabelText("Search");
    fireEvent.change(input, { target: { value: "u" } });
    expect(screen.getAllByRole("option")[0]).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(screen.getAllByRole("option")[1]).toHaveAttribute("aria-selected", "true");
  });
});
