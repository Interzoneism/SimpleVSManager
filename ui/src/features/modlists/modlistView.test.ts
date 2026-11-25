import { describe, expect, it } from "vitest";
import type { ApplyPlan, ModlistSummary } from "../../bindings";
import { defaultMode, describePlan, filterModlists, modLocation, serverCommands } from "./modlistView";

const summary = (over: Partial<ModlistSummary>): ModlistSummary => ({
  id: "Modlists/a.json",
  kind: "modlist",
  fileName: "a.json",
  name: "A",
  description: null,
  icon: null,
  version: null,
  gameVersion: null,
  uploader: null,
  modifiedMs: null,
  mods: [],
  configCount: 0,
  hasVersions: true,
  error: null,
  group: null,
  active: false,
  downloadsNeeded: 0,
  subscription: null,
  ...over,
});

const emptyPlan: ApplyPlan = { install: [], changeVersion: [], remove: [], states: [], disabledEntries: null, skipped: [], toggles: 0 };

describe("modlist view helpers", () => {
  it("filters by kind and text, including mod ids", () => {
    const all = [
      summary({ id: "1", name: "Hardcore", mods: [{ modId: "carryon", version: "1.0", isActive: true }] }),
      summary({ id: "2", name: "Cozy", kind: "preset" }),
    ];
    expect(filterModlists(all, "modlist", "").map((m) => m.id)).toEqual(["1"]);
    expect(filterModlists(all, "modlist", "CARRY").map((m) => m.id)).toEqual(["1"]);
    expect(filterModlists(all, "modlist", "cozy")).toEqual([]);
  });

  it("picks safe default load modes", () => {
    expect(defaultMode("preset")).toBe("statesOnly");
    expect(defaultMode("backup")).toBe("replace");
    expect(defaultMode("modlist")).toBe("replace");
    expect(defaultMode(null)).toBe("add");
  });

  it("builds server commands only for mods with versions", () => {
    const s = summary({
      mods: [
        { modId: "a", version: "1.0.0", isActive: true },
        { modId: "b", version: null, isActive: true },
      ],
    });
    expect(serverCommands(s)).toBe("/moddb install a@1.0.0");
  });

  it("describes plans", () => {
    expect(describePlan(emptyPlan, 0, true)).toEqual([]);
    const plan = { ...emptyPlan, install: [{ modId: "x", version: null }], toggles: 2 };
    expect(describePlan(plan, 3, true)).toEqual(["Install 1 missing mod", "Turn 2 mods on or off", "Overwrite 3 config files"]);
    expect(describePlan(plan, 3, false)).toHaveLength(2);
  });

  it("says where a mod is", () => {
    expect(modLocation("1.0", "1.0", [])).toBe("installed");
    expect(modLocation("1.0", "2.0", ["1.0"])).toBe("in library");
    expect(modLocation("1.0", "2.0", [])).toBe("installed: 2.0");
    expect(modLocation("1.0", null, [])).toBe("not downloaded");
    expect(modLocation(null, null, [])).toBeNull();
  });
});
