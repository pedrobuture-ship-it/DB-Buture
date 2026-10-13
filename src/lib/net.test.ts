import { isLocalHost, weakTls } from "./net";

describe("net helpers", () => {
  it("recognizes loopback hosts", () => {
    for (const h of ["localhost", "LOCALHOST", "127.0.0.1", "127.1.2.3", "::1", "[::1]", ""])
      expect(isLocalHost(h)).toBe(true);
    for (const h of ["db.internal", "10.0.0.5", "127.0.0.1.evil.com", "192.168.1.10"])
      expect(isLocalHost(h)).toBe(false);
  });

  it("warns only for remote hosts without enforced TLS", () => {
    expect(weakTls("prefer", "db.example.com")).toBe(true);
    expect(weakTls("disable", "10.0.0.5")).toBe(true);
    expect(weakTls("require", "db.example.com")).toBe(false);
    expect(weakTls("verifyfull", "db.example.com")).toBe(false);
    expect(weakTls("disable", "localhost")).toBe(false);
  });
});
