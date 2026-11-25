// Sorting and library status for modlist mod tables. No React, so the builder store can use it.
import type { ModlistModCard } from "../../bindings";
import { compareVersions } from "../../lib/version";
import { modLocation } from "./modlistView";

/** One mod of a modlist, as the table shows it. Missing details come from the Mod DB cache. */
export interface ModlistTableRow {
  modId: string;
  version: string | null;
  /** `false`: the version isn't tagged for this game version. */
  compatible?: boolean | null;
  /** `false`: the list has the mod switched off. */
  isActive?: boolean | null;
  name?: string | null;
  author?: string | null;
  side?: string | null;
  logoUrl?: string | null;
}

export type ModlistSortKey = "name" | "version" | "library" | "author" | "side";

export const SORT_KEYS: ModlistSortKey[] = ["name", "version", "library", "author", "side"];

/** Where the list's version of a mod is: installed, kept in the mod library, or neither. */
export function libraryStatus(version: string | null, card: ModlistModCard | undefined): { label: string; detail: string; rank: number; have: boolean } | null {
  if (!card) return null;
  const where = modLocation(version, card.installedVersion, card.libraryVersions);
  switch (where) {
    case "installed":
      return { label: "Installed", detail: "This version is installed", rank: 0, have: true };
    case "in library":
      return { label: "In library", detail: "This version is kept in the mod library", rank: 1, have: true };
    case null:
      return card.libraryVersions.length
        ? { label: "In library", detail: `Kept in the mod library: ${card.libraryVersions.join(", ")}`, rank: 1, have: true }
        : { label: "Not downloaded", detail: "Loading the list downloads it", rank: 3, have: false };
    case "not downloaded":
      return { label: "Not downloaded", detail: "Loading the list downloads it", rank: 3, have: false };
    default:
      return { label: "Other version", detail: `${card.installedVersion} is installed; loading the list switches it`, rank: 2, have: false };
  }
}

/** The rows in table order. Unsorted (`null`) = the list's own order. */
export function sortRows<R extends ModlistTableRow>(rows: R[], key: ModlistSortKey | null, dir: "asc" | "desc", cards?: Map<string, ModlistModCard>): R[] {
  if (!key) return rows;
  const text = (v: string | null | undefined) => (v ?? "").toLowerCase();
  const name = (r: R) => text(r.name ?? cards?.get(r.modId.toLowerCase())?.name ?? r.modId);
  const byName = (a: R, b: R) => name(a).localeCompare(name(b));
  const info = (r: R, f: "author" | "side") => r[f] ?? cards?.get(r.modId.toLowerCase())?.[f] ?? null;
  const rank = (r: R) => libraryStatus(r.version, cards?.get(r.modId.toLowerCase()))?.rank ?? 4;
  const cmp = (a: R, b: R): number => {
    switch (key) {
      case "name":
        return byName(a, b);
      case "version":
        return compareVersions(a.version, b.version);
      case "library":
        return rank(a) - rank(b);
      case "author":
      case "side":
        return text(info(a, key)).localeCompare(text(info(b, key)));
    }
  };
  const sign = dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => sign * cmp(a, b) || byName(a, b));
}

