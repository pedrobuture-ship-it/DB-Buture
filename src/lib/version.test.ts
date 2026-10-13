import { APP_NAME } from "./version";

describe("version", () => {
  it("exposes the product name", () => {
    expect(APP_NAME).toBe("DB-BUTURE");
  });
});
