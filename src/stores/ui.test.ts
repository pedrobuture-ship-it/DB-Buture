import { useUi } from "./ui";

describe("ui store", () => {
  beforeEach(() => useUi.getState().reset());

  it("selects an object and navigates", () => {
    useUi.getState().select("postgres:public.users", "explorer");
    expect(useUi.getState().selectedId).toBe("postgres:public.users");
    expect(useUi.getState().view).toBe("explorer");
  });

  it("keeps the current view when selecting without a target view", () => {
    useUi.getState().setView("erd");
    useUi.getState().select("x");
    expect(useUi.getState().view).toBe("erd");
  });

  it("reset returns to overview", () => {
    useUi.getState().select("x", "impact");
    useUi.getState().reset();
    expect(useUi.getState().view).toBe("overview");
    expect(useUi.getState().selectedId).toBeNull();
  });
});
