import { create } from "zustand";
import type { StableId } from "@/types/schema";

export type ViewId =
  | "overview"
  | "explorer"
  | "tables"
  | "views"
  | "indexes"
  | "erd"
  | "dependencies"
  | "lineage"
  | "impact"
  | "health"
  | "index-analysis"
  | "plans"
  | "snapshots"
  | "diff"
  | "environments"
  | "sql"
  | "raw"
  | "settings";

interface UiState {
  view: ViewId;
  selectedId: StableId | null;
  showSystemSchemas: boolean;
  paletteOpen: boolean;
  setView: (view: ViewId) => void;
  /** Select an object and optionally jump to the view that inspects it. */
  select: (id: StableId | null, view?: ViewId) => void;
  setShowSystemSchemas: (v: boolean) => void;
  setPaletteOpen: (v: boolean) => void;
  reset: () => void;
}

export const useUi = create<UiState>((set) => ({
  view: "overview",
  selectedId: null,
  showSystemSchemas: false,
  paletteOpen: false,
  setView: (view) => set({ view }),
  select: (selectedId, view) => set((s) => ({ selectedId, view: view ?? s.view })),
  setShowSystemSchemas: (showSystemSchemas) => set({ showSystemSchemas }),
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  reset: () => set({ view: "overview", selectedId: null, paletteOpen: false }),
}));
