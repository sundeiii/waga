import { describe, expect, it } from "vitest";
import { displayName, localCrumbs, remoteCrumbs } from "./paths";

describe("breadcrumbs", () => {
  it("remote", () => {
    expect(remoteCrumbs("/")).toEqual([{ label: "/", path: "/" }]);
    expect(remoteCrumbs("/home/u").map((c) => c.path)).toEqual(["/", "/home", "/home/u"]);
    expect(remoteCrumbs("/C:/Users/me").map((c) => c.label)).toEqual(["/", "C:", "Users", "me"]);
  });
  it("local", () => {
    expect(localCrumbs("")).toEqual([{ label: "This PC", path: "" }]);
    expect(localCrumbs("C:\\").map((c) => c.path)).toEqual(["", "C:\\"]);
    expect(localCrumbs("C:\\Users\\me").map((c) => c.path)).toEqual(["", "C:\\", "C:\\Users", "C:\\Users\\me"]);
  });
  it("control chars made visible", () => {
    expect(displayName("evil\u001b[31mname")).toBe("evil\uFFFD[31mname");
  });
});
