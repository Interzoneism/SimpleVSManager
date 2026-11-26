import type { ApplyPlan, LoadMode, ModlistKind, ModlistSummary } from "../../bindings";

export type QuickSwitchPreference = "review" | "keep-configs" | "use-list-configs";

export function canQuickSwitch(kind: ModlistKind | null) {
  return kind === "modlist" || kind === "group";
}

/** Entries matching a search by name, description, uploader or mod ID. */
export function filterModlists(all: ModlistSummary[], kinds: ModlistKind[], search: string): ModlistSummary[] {
  const q = search.trim().toLowerCase();
  return all.filter(
    (m) =>
      kinds.includes(m.kind) &&
      (!q ||
        m.name.toLowerCase().includes(q) ||
        (m.description ?? "").toLowerCase().includes(q) ||
        (m.uploader ?? "").toLowerCase().includes(q) ||
        m.mods.some((x) => x.modId.toLowerCase().includes(q))),
  );
}

/** Saved modlists: your published ones first, then by name. */
export function sortSaved(list: ModlistSummary[]): ModlistSummary[] {
  return [...list].sort((a, b) => Number(!!b.published) - Number(!!a.published) || a.name.localeCompare(b.name));
}

/** Saved lists switch over by default. Imported files and online lists add to what's there. */
export function defaultMode(kind: ModlistKind | null): LoadMode {
  if (kind === null) return "add";
  return "replace";
}

/** One `/moddb install id@version` line per mod with a version (for a server console). */
export function serverCommands(list: ModlistSummary): string {
  return list.mods
    .filter((m) => m.version)
    .map((m) => `/moddb install ${m.modId}@${m.version}`)
    .join("\n");
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** Lines describing a plan, for the load dialog. Empty when nothing would change. */
export function describePlan(plan: ApplyPlan, configs: number, importConfigs: boolean): string[] {
  const lines: string[] = [];
  if (plan.install.length) lines.push(`Install ${plural(plan.install.length, "missing mod")}`);
  if (plan.changeVersion.length) lines.push(`Switch ${plural(plan.changeVersion.length, "mod")} to another version`);
  if (plan.remove.length) lines.push(`Remove ${plural(plan.remove.length, "mod")} not in the list`);
  if (plan.toggles) lines.push(`Turn ${plural(plan.toggles, "mod")} on or off`);
  if (plan.disabledEntries) lines.push("Set which mods are disabled");
  if (configs && importConfigs) lines.push(`Overwrite ${plural(configs, "config file")}`);
  return lines;
}

export function kindLabel(kind: ModlistKind): string {
  return { modlist: "Modlist", backup: "Backup", group: "Group" }[kind];
}

/** Where a modlist's mod is: installed (at the wanted version), in the mod library, or neither. */
export function modLocation(wanted: string | null, installed: string | null, library: string[]): string | null {
  if (!wanted) return installed ? "installed" : null;
  if (installed === wanted) return "installed";
  if (library.includes(wanted)) return "in library";
  return installed ? `installed: ${installed}` : "not downloaded";
}

/** How many recently picked modlists the "Load a modlist" picker shows first. */
export const RECENT_PICKS = 5;

/** The recent-picks list after picking `id`: it moves to the front, capped at `RECENT_PICKS`. */
export function rememberPick(recent: string[], id: string): string[] {
  return [id, ...recent.filter((r) => r !== id)].slice(0, RECENT_PICKS);
}

/** Order for the picker: the recently picked lists that still exist (most recent first), then
 * the rest in the usual saved-modlists order. */
export function pickerOrder(list: ModlistSummary[], recent: string[]): ModlistSummary[] {
  const byId = new Map(list.map((m) => [m.id, m]));
  const first = recent.slice(0, RECENT_PICKS).map((id) => byId.get(id)).filter((m): m is ModlistSummary => !!m);
  const shown = new Set(first.map((m) => m.id));
  return [...first, ...sortSaved(list.filter((m) => !shown.has(m.id)))];
}
