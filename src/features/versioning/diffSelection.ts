import { create } from "zustand";

/** What the Schema Diff view compares. Either side is a snapshot id or the literal "current". */
interface DiffSelection {
  before: string;
  after: string;
  pick: (side: "before" | "after", value: string) => void;
}

export const useDiffSelection = create<DiffSelection>((set) => ({
  before: "",
  after: "current",
  pick: (side, value) => set({ [side]: value } as Pick<DiffSelection, "before" | "after">),
}));
