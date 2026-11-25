import { describe, expect, it } from "vitest";
import type { ModDbRelease } from "../../bindings";
import { releasesBetween } from "./changelog";

const rel = (version: string): ModDbRelease => ({
  version,
  downloadUrl: "https://x",
  fileName: null,
  gameVersions: [],
  compatible: true,
  changelog: version,
  downloads: null,
  createdMs: null,
});

// Newest first, with a pre-release on top, as the Mod DB lists them.
const releases = ["2.0.0-rc.1", "1.14.3", "1.14.2", "1.14.1", "1.13.0"].map(rel);

describe("releasesBetween", () => {
  it("lists releases newer than the installed one", () => {
    expect(releasesBetween(releases, "1.14.1", "1.14.3").map((r) => r.version)).toEqual(["1.14.3", "1.14.2"]);
  });
  it("handles unknown installed version", () => {
    expect(releasesBetween(releases, null, "1.14.2").map((r) => r.version)).toEqual(["1.14.2", "1.14.1", "1.13.0"]);
  });
  it("rollback shows only the target", () => {
    expect(releasesBetween(releases, "1.14.3", "1.13.0").map((r) => r.version)).toEqual(["1.13.0"]);
  });
  it("skips other branches newer than the target", () => {
    const mixed = ["1.13.0", "1.14.0-pre.3", "1.12.2", "1.12.1"].map(rel);
    expect(releasesBetween(mixed, "1.12.1", "1.13.0").map((r) => r.version)).toEqual(["1.13.0", "1.12.2"]);
  });
  it("unknown target is empty", () => {
    expect(releasesBetween(releases, "1.0", "9.9")).toEqual([]);
  });
});
