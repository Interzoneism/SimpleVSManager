import type { ApplyPlan, LoadMode, ModlistKind, ModlistSummary } from "../../bindings";

export type QuickSwitchPreference = "review" | "keep-configs" | "use-list-configs";

export function canQuickSwitch(kind: ModlistKind | null) {
  return kind === "modlist" || kind === "group";
}

export const SECTIONS: { kind: ModlistKind; title: string; empty: string }[] = [
  { kind: "modlist", title: "Modlists", empty: "Save your mods as a modlist to share it or switch back later." },
  { kind: "group", title: "Groups", empty: "Combine several modlists into one. Select modlists and choose New group." },
  { kind: "preset", title: "Presets", empty: "Presets remember which mods are on and off." },
  { kind: "backup", title: "Backups", empty: "Saved automatically before a modlist is loaded or mods are updated or deleted." },
];

/** Entries of one kind matching a search (name, description, uploader or a mod id). */
export function filterModlists(all: ModlistSummary[], kind: ModlistKind, search: string): ModlistSummary[] {
  const q = search.trim().toLowerCase();
  return all.filter(
    (m) =>
      m.kind === kind &&
      (!q ||
        m.name.toLowerCase().includes(q) ||
        (m.description ?? "").toLowerCase().includes(q) ||
        (m.uploader ?? "").toLowerCase().includes(q) ||
        m.mods.some((x) => x.modId.toLowerCase().includes(q))),
  );
}

/** Starting choice: presets only switch mods on/off, saved lists switch over (other mods go to
 * the mod library), outside files add to what's there. */
export function defaultMode(kind: ModlistKind | null): LoadMode {
  if (kind === "preset") return "statesOnly";
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
  if (plan.remove.length) lines.push(`Put away ${plural(plan.remove.length, "mod")} that aren't in the list (kept in your mod library)`);
  if (plan.toggles) lines.push(`Turn ${plural(plan.toggles, "mod")} on or off`);
  if (plan.disabledEntries) lines.push("Set which mods are disabled (older preset)");
  if (configs && importConfigs) lines.push(`Overwrite ${plural(configs, "config file")}`);
  return lines;
}

export function kindLabel(kind: ModlistKind): string {
  return { modlist: "Modlist", preset: "Preset", backup: "Backup", group: "Group" }[kind];
}

/** Where a modlist's mod is: installed (at the wanted version), in the mod library, or neither. */
export function modLocation(wanted: string | null, installed: string | null, library: string[]): string | null {
  if (!wanted) return installed ? "installed" : null;
  if (installed === wanted) return "installed";
  if (library.includes(wanted)) return "in library";
  return installed ? `installed: ${installed}` : "not downloaded";
}
