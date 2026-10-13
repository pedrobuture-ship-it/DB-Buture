import { formatAge, formatBytes, formatCount } from "./format";

describe("format", () => {
  it("formats bytes", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(8192)).toBe("8.0 KB");
    expect(formatBytes(5 * 1024 * 1024 * 1024)).toBe("5.0 GB");
  });

  it("formats counts", () => {
    expect(formatCount(null)).toBe("—");
    expect(formatCount(1234567)).toBe("1,234,567");
  });

  it("formats cache age", () => {
    const now = new Date("2026-01-01T12:00:00Z");
    expect(formatAge("2026-01-01T12:00:00Z", now)).toBe("just now");
    expect(formatAge("2026-01-01T11:59:30Z", now)).toBe("30s ago");
    expect(formatAge("2026-01-01T11:55:00Z", now)).toBe("5 min ago");
    expect(formatAge("2026-01-01T09:00:00Z", now)).toBe("3 h ago");
    expect(formatAge("2025-12-29T12:00:00Z", now)).toBe("3 d ago");
  });
});
