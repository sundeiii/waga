import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

const { formatBytes, formatRate, formatUptime, memPercent, rootDisk, diskPercent } = await import("./monitoring");

describe("monitoring formatters", () => {
  it("bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(formatBytes(null)).toBe("n/a");
    expect(formatRate(2048)).toBe("2.0 KB/s");
    expect(formatRate(null)).toBe("n/a");
  });
  it("uptime", () => {
    expect(formatUptime(59)).toBe("0m");
    expect(formatUptime(3 * 3600 + 120)).toBe("3h 2m");
    expect(formatUptime(2 * 86400 + 5 * 3600)).toBe("2d 5h");
    expect(formatUptime(null)).toBe("n/a");
  });
  it("percentages never invent data", () => {
    const base = { serverId: "x", ts: "", cpuPercent: null, cpuCores: null, netRxBps: null, netTxBps: null, load: null, uptimeSecs: null, hostname: null, os: null };
    expect(memPercent({ ...base, memTotalBytes: null, memUsedBytes: 5, disks: [] })).toBeNull();
    expect(memPercent({ ...base, memTotalBytes: 200, memUsedBytes: 50, disks: [] })).toBe(25);
    const s = { ...base, memTotalBytes: null, memUsedBytes: null, disks: [{ mount: "/srv", totalBytes: 10, usedBytes: 1 }, { mount: "/", totalBytes: 100, usedBytes: 40 }] };
    expect(rootDisk(s)?.mount).toBe("/");
    expect(diskPercent(rootDisk(s))).toBe(40);
    expect(diskPercent(null)).toBeNull();
  });
});
