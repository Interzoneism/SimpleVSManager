import type { InstalledMod } from "../../bindings";
import { filterMods, modStatus, sortMods, statusLabel, summarize } from "./modView";
import { compareVersions } from "../../lib/version";

function mod(partial: Partial<InstalledMod> & { name: string }): InstalledMod {
  return {
    key: `/mods/${partial.name}.zip`,
    modId: partial.name.toLowerCase(),
    manifestName: null,
    version: "1.0.0",
    networkVersion: null,
    description: null,
    website: null,
    authors: [],
    contributors: [],
    dependencies: [],
    side: null,
    requiredOnClient: null,
    requiredOnServer: null,
    sourcePath: `/mods/${partial.name}.zip`,
    sourceKind: "zip",
    fileName: `${partial.name}.zip`,
    sizeBytes: null,
    modifiedMs: null,
    iconKey: null,
    error: null,
    loadError: null,
    missingDependencies: [],
    dependencyHasErrors: false,
    duplicates: [],
    isActive: true,
    ...partial,
  };
}

const mods = [
  mod({ name: "Carry On", authors: ["copygirl"], version: "1.10.0" }),
  mod({ name: "Alpha", version: "1.9.2", isActive: false }),
  mod({ name: "Broken", error: "Missing modinfo.json" }),
  mod({
    name: "Needy",
    loadError: "Unable to load mod. Requires dependency lib",
    missingDependencies: [{ modId: "lib", version: "*" }],
  }),
];

test("status", () => {
  expect(mods.map(modStatus)).toEqual(["active", "disabled", "error", "problem"]);
  expect(statusLabel(mods[3])).toBe("Missing dependency");
  expect(statusLabel(mod({ name: "Dup", loadError: "x", duplicates: ["/other.zip"] }))).toBe("Duplicate");
  expect(statusLabel(mod({ name: "Code", sourceKind: "assembly", error: "x" }))).toBe("Not manageable");
});

test("filter by text and status", () => {
  expect(filterMods(mods, "copy", "all").map((m) => m.name)).toEqual(["Carry On"]);
  expect(filterMods(mods, "", "disabled").map((m) => m.name)).toEqual(["Alpha"]);
  expect(filterMods(mods, "", "problems").map((m) => m.name)).toEqual(["Broken", "Needy"]);
  expect(filterMods(mods, "carry on", "active")).toHaveLength(1);
});

test("sort", () => {
  expect(sortMods(mods, "name", "asc").map((m) => m.name)).toEqual(["Alpha", "Broken", "Carry On", "Needy"]);
  expect(sortMods(mods, "version", "desc")[0].name).toBe("Carry On"); // 1.10.0 > 1.9.2
  expect(sortMods(mods, "status", "asc").map((m) => m.name)).toEqual(["Broken", "Needy", "Alpha", "Carry On"]);
});

test("summary", () => {
  expect(summarize(mods)).toEqual({ total: 4, active: 2, problems: 2 });
});

test("compareVersions", () => {
  expect(compareVersions("1.10", "1.9.9")).toBeGreaterThan(0);
  expect(compareVersions("1.2", "1.2.0")).toBe(0);
  expect(compareVersions(null, "1.0")).toBeLessThan(0);
});
