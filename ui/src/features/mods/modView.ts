// Pure helpers for the installed mods table: status, filtering and sorting. Kept free of React
// so they're easy to test.
import type { InstalledMod, ModDbSummary } from "../../bindings";
import { compareVersions } from "../../lib/version";

export type ModStatus = "active" | "disabled" | "error" | "problem";
export type StatusFilter = "all" | "active" | "disabled" | "problems" | "updates";
/** Mod DB summaries by installed mod key. */
export type DbIndex = Map<string, ModDbSummary>;
export type SortKey = "name" | "version" | "latest" | "authors" | "tags" | "side" | "status" | "modified";
export type SortDirection = "asc" | "desc";

export function canToggleMod(mod: InstalledMod): boolean {
  return !!mod.modId && mod.sourceKind !== "sourceCode" && mod.sourceKind !== "assembly";
}

export function modStatus(mod: InstalledMod): ModStatus {
  if (mod.error) return "error";
  if (!mod.isActive) return "disabled";
  if (mod.loadError) return "problem";
  return "active";
}

/** Short status text for the table. */
export function statusLabel(mod: InstalledMod): string {
  switch (modStatus(mod)) {
    case "error":
      return mod.sourceKind === "sourceCode" || mod.sourceKind === "assembly" ? "Not manageable" : "Can't read";
    case "disabled":
      return "Disabled";
    case "problem":
      if (mod.duplicates.length > 0 && !mod.missingDependencies.length && !mod.dependencyHasErrors) return "Duplicate";
      if (mod.missingDependencies.length > 0) return "Missing dependency";
      return "Dependency problem";
    case "active":
      return "Active";
  }
}

export function hasProblem(mod: InstalledMod): boolean {
  return !!mod.error || !!mod.loadError;
}

const STATUS_ORDER: Record<ModStatus, number> = { error: 0, problem: 1, disabled: 2, active: 3 };

export function filterMods(
  mods: InstalledMod[],
  search: string,
  status: StatusFilter,
  db: DbIndex = new Map(),
): InstalledMod[] {
  const terms = search.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return mods.filter((m) => {
    if (status === "active" && !m.isActive) return false;
    if (status === "disabled" && m.isActive) return false;
    if (status === "problems" && !hasProblem(m)) return false;
    if (status === "updates" && !db.get(m.key)?.update) return false;
    if (terms.length === 0) return true;
    const haystack = [m.name, m.modId, m.fileName, ...m.authors, m.description ?? ""].join(" ").toLowerCase();
    return terms.every((t) => haystack.includes(t));
  });
}

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

export function sortMods(
  mods: InstalledMod[],
  key: SortKey,
  direction: SortDirection,
  db: DbIndex = new Map(),
): InstalledMod[] {
  const sign = direction === "asc" ? 1 : -1;
  const byName = (a: InstalledMod, b: InstalledMod) => collator.compare(a.name, b.name);
  const cmp = (a: InstalledMod, b: InstalledMod): number => {
    switch (key) {
      case "name":
        return byName(a, b);
      case "version":
        return compareVersions(a.version, b.version);
      case "latest": {
        // Mods with updates first, then by latest version.
        const ua = db.get(a.key)?.update ? 0 : 1;
        const ub = db.get(b.key)?.update ? 0 : 1;
        return ua - ub || compareVersions(latestShown(db.get(a.key)), latestShown(db.get(b.key)));
      }
      case "authors":
        return collator.compare(a.authors.join(", "), b.authors.join(", "));
      case "tags": {
        // Mods without tags (not on the Mod DB, or not looked up yet) go last.
        const ta = modTags(db, a).join(", ");
        const tb = modTags(db, b).join(", ");
        return (ta ? 0 : 1) - (tb ? 0 : 1) || collator.compare(ta, tb);
      }
      case "side":
        return collator.compare(a.side ?? "", b.side ?? "");
      case "status":
        return STATUS_ORDER[modStatus(a)] - STATUS_ORDER[modStatus(b)];
      case "modified":
        return (a.modifiedMs ?? 0) - (b.modifiedMs ?? 0);
    }
  };
  // Ties always fall back to name ascending so the order is stable.
  return [...mods].sort((a, b) => sign * cmp(a, b) || byName(a, b));
}

export function summarize(mods: InstalledMod[]) {
  return {
    total: mods.length,
    active: mods.filter((m) => m.isActive && !m.error).length,
    problems: mods.filter(hasProblem).length,
  };
}

/** The mod's Mod DB category tags, as the Tags column shows them. */
export function modTags(db: DbIndex, mod: InstalledMod): string[] {
  return db.get(mod.key)?.tags ?? [];
}

/** The version shown in the "Latest" column. */
export function latestShown(s: ModDbSummary | undefined): string | null {
  if (!s) return null;
  return s.update?.version ?? s.latestCompatibleVersion ?? s.latestVersion;
}

/** Keys of mods "Update all" would update (offered and not excluded). */
export function bulkUpdateKeys(mods: InstalledMod[], db: DbIndex): string[] {
  return mods.filter((m) => {
    const s = db.get(m.key);
    return !!s?.update && !s.excludedFromBulkUpdate;
  }).map((m) => m.key);
}
