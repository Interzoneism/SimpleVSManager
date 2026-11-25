// The modlist builder's draft. Lives outside React so it survives leaving the Browse page, and
// in localStorage so it survives a restart. Async lookups (versions, names) patch it even when
// the page isn't showing.
import { useSyncExternalStore } from "react";
import { commands, type BrowseEntry, type ModlistSummary } from "../../bindings";
import { compareVersions } from "../../lib/version";

export interface BuilderEntry {
  modId: string;
  /** Version saved in the list. `null` until the newest compatible one is looked up (or unknown). */
  version: string | null;
  /** `version` is tagged for this game version; `null` = not known (pinned by a saved list). */
  compatible: boolean | null;
  name: string | null;
  author: string | null;
  side: string | null;
  logoUrl: string | null;
  downloads: number | null;
}

export type BuilderSortKey = "name" | "version" | "author" | "side" | "downloads";

export interface BuilderState {
  open: boolean;
  /** The saved modlist being edited; `null` = a new list. */
  sourceId: string | null;
  name: string;
  entries: BuilderEntry[];
  /** Signature of the last saved or loaded state, to tell if there are unsaved changes. */
  baseline: string;
  sortKey: BuilderSortKey | null;
  sortDirection: "asc" | "desc";
  /** Mod ids being looked up. */
  resolving: string[];
}

const STORAGE_KEY = "svsm-builder";
const lower = (s: string) => s.toLowerCase();

export function signature(s: Pick<BuilderState, "sourceId" | "name" | "entries">): string {
  const mods = s.entries.map((e) => `${lower(e.modId)}@${e.version ?? ""}`).sort();
  return JSON.stringify([s.sourceId, s.sourceId ? "" : s.name.trim(), mods]);
}

export const emptyState = (): BuilderState => {
  const base = { sourceId: null, name: "", entries: [] };
  return { ...base, open: false, baseline: signature(base), sortKey: null, sortDirection: "asc", resolving: [] };
};

const str = (v: unknown) => (typeof v === "string" ? v : null);

function load(): BuilderState {
  const base = emptyState();
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return base;
    const v = JSON.parse(raw) as Partial<Record<keyof BuilderState, unknown>>;
    const entries: BuilderEntry[] = Array.isArray(v.entries)
      ? (v.entries as Record<string, unknown>[])
          .filter((e) => !!e && typeof e.modId === "string" && e.modId.length > 0)
          .map((e) => ({
            modId: e.modId as string,
            version: str(e.version),
            compatible: typeof e.compatible === "boolean" ? e.compatible : null,
            name: str(e.name),
            author: str(e.author),
            side: str(e.side),
            logoUrl: str(e.logoUrl),
            downloads: typeof e.downloads === "number" ? e.downloads : null,
          }))
      : [];
    const sortKeys: BuilderSortKey[] = ["name", "version", "author", "side", "downloads"];
    return {
      ...base,
      open: v.open === true,
      sourceId: str(v.sourceId),
      name: str(v.name) ?? "",
      entries,
      baseline: str(v.baseline) ?? base.baseline,
      sortKey: sortKeys.includes(v.sortKey as BuilderSortKey) ? (v.sortKey as BuilderSortKey) : null,
      sortDirection: v.sortDirection === "desc" ? "desc" : "asc",
    };
  } catch {
    return base;
  }
}

let state: BuilderState = load();
const listeners = new Set<() => void>();
let saveTimer: ReturnType<typeof setTimeout> | undefined;

function persist() {
  clearTimeout(saveTimer);
  saveTimer = undefined;
  try {
    const { resolving: _resolving, ...rest } = state;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(rest));
  } catch {
    // Memory still has it for this session.
  }
}

if (typeof window !== "undefined") window.addEventListener("pagehide", persist);

function set(patch: Partial<BuilderState> | ((s: BuilderState) => Partial<BuilderState>)) {
  const next = typeof patch === "function" ? patch(state) : patch;
  state = { ...state, ...next };
  clearTimeout(saveTimer);
  saveTimer = setTimeout(persist, 100);
  for (const l of listeners) l();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};

export const getBuilder = () => state;
export const useBuilder = (): BuilderState => useSyncExternalStore(subscribe, getBuilder);

export const isDirty = (s: BuilderState) => signature(s) !== s.baseline;
export const hasEntry = (s: BuilderState, modId: string) => s.entries.some((e) => lower(e.modId) === lower(modId));

export function entryFromBrowse(e: BrowseEntry, logoUrl: string | null): BuilderEntry {
  return { modId: e.modId, version: null, compatible: null, name: e.name, author: e.author, side: e.side, logoUrl, downloads: e.downloads };
}

function patchEntries(patches: Map<string, Partial<BuilderEntry>>) {
  if (!patches.size) return;
  set((s) => ({
    entries: s.entries.map((e) => {
      const p = patches.get(lower(e.modId));
      return p ? { ...e, ...p } : e;
    }),
  }));
}

/** Looks up the newest compatible version of entries that have none. */
async function resolveVersions() {
  const ids = state.entries.filter((e) => e.version == null && !state.resolving.includes(e.modId)).map((e) => e.modId);
  if (!ids.length) return;
  set((s) => ({ resolving: [...s.resolving, ...ids] }));
  try {
    const r = await commands.builderVersions(ids);
    if (r.status === "ok") {
      const patches = new Map<string, Partial<BuilderEntry>>();
      for (const v of r.data) {
        // Only fill in versions still missing.
        if (v.version && state.entries.some((e) => lower(e.modId) === lower(v.modId) && e.version == null)) {
          patches.set(lower(v.modId), { version: v.version, compatible: v.compatible });
        }
      }
      patchEntries(patches);
    }
  } catch {
    // Saving looks the versions up again.
  } finally {
    set((s) => ({ resolving: s.resolving.filter((id) => !ids.includes(id)) }));
  }
}

/** Fills in names, authors and pictures from the Mod DB for entries that lack them. */
async function fillInfo() {
  const ids = state.entries.filter((e) => e.name == null).map((e) => e.modId);
  if (!ids.length) return;
  try {
    const r = await commands.modlistModCards(ids, true);
    if (r.status !== "ok") return;
    const patches = new Map<string, Partial<BuilderEntry>>();
    for (const c of r.data) {
      patches.set(lower(c.modId), { name: c.name ?? c.modId, author: c.author, side: c.side, logoUrl: c.logoUrl, downloads: c.downloads });
    }
    patchEntries(patches);
  } catch {
    // Rows show the mod id instead.
  }
}

/** Finishes lookups that a restart or page change interrupted. */
export function resumeLookups() {
  void resolveVersions();
  void fillInfo();
}

const newList = () => {
  const base = { sourceId: null, name: "", entries: [] };
  return { ...base, baseline: signature(base) };
};

export const builder = {
  setOpen: (open: boolean) => set({ open }),
  setName: (name: string) => set({ name }),

  /** Adds mods that aren't in the list yet. Returns how many were added. */
  add(entries: BuilderEntry[]): number {
    const seen = new Set(state.entries.map((e) => lower(e.modId)));
    const fresh = entries.filter((e) => !seen.has(lower(e.modId)) && seen.add(lower(e.modId)));
    if (!fresh.length) return 0;
    set((s) => ({ entries: [...s.entries, ...fresh] }));
    void resolveVersions();
    return fresh.length;
  },

  remove(modIds: string[]) {
    const gone = new Set(modIds.map(lower));
    set((s) => ({ entries: s.entries.filter((e) => !gone.has(lower(e.modId))) }));
  },

  /** Starts editing a saved modlist, keeping its versions; `null` starts a new, empty list. */
  load(list: ModlistSummary | null) {
    if (!list) {
      set(newList());
      return;
    }
    const seen = new Set<string>();
    const entries: BuilderEntry[] = list.mods
      .filter((m) => m.modId && !seen.has(lower(m.modId)) && seen.add(lower(m.modId)))
      .map((m) => ({ modId: m.modId, version: m.version, compatible: null, name: null, author: null, side: null, logoUrl: null, downloads: null }));
    const base = { sourceId: list.id, name: list.name, entries };
    set({ ...base, baseline: signature(base) });
    resumeLookups();
  },

  /** The saved modlist is gone: keep the mods as a new list. */
  detach: () => set((s) => ({ sourceId: null, baseline: signature({ sourceId: null, name: "", entries: [] }), name: s.name })),

  /** Cancel: drop the list and start over with a new, empty one. */
  discard: () => set(newList()),

  /** After saving: the list now matches the saved modlist. */
  saved(list: ModlistSummary) {
    set((s) => {
      // The save filled in versions that were still being looked up.
      const versions = new Map(list.mods.map((m) => [lower(m.modId), m.version]));
      const entries = s.entries.map((e) => (e.version == null && versions.get(lower(e.modId)) ? { ...e, version: versions.get(lower(e.modId))! } : e));
      const next = { sourceId: list.id, name: list.name, entries };
      return { ...next, baseline: signature(next) };
    });
  },

  sort(key: BuilderSortKey) {
    set((s) =>
      s.sortKey === key
        ? { sortDirection: s.sortDirection === "asc" ? "desc" : "asc" }
        : { sortKey: key, sortDirection: key === "downloads" ? "desc" : "asc" },
    );
  },
};

/** Test helper: replace the whole state. */
export function resetBuilderForTests(next: BuilderState = emptyState()) {
  state = next;
  for (const l of listeners) l();
}

/** The entries in table order. Unsorted = the order they were added. */
export function sortedEntries(entries: BuilderEntry[], key: BuilderSortKey | null, dir: "asc" | "desc"): BuilderEntry[] {
  if (!key) return entries;
  const text = (v: string | null) => (v ?? "").toLowerCase();
  const byName = (a: BuilderEntry, b: BuilderEntry) => text(a.name ?? a.modId).localeCompare(text(b.name ?? b.modId));
  const cmp = (a: BuilderEntry, b: BuilderEntry): number => {
    switch (key) {
      case "name":
        return byName(a, b);
      case "version":
        return compareVersions(a.version, b.version);
      case "author":
        return text(a.author).localeCompare(text(b.author));
      case "side":
        return text(a.side).localeCompare(text(b.side));
      case "downloads":
        return (a.downloads ?? -1) - (b.downloads ?? -1);
    }
  };
  const sign = dir === "asc" ? 1 : -1;
  return [...entries].sort((a, b) => sign * cmp(a, b) || byName(a, b));
}
