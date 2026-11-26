import type { OnlineEntry } from "../../bindings";

export type OnlineSort = "newest" | "subscribers" | "mods" | "name";

/** Search (name, description, author, mod ids) and sort the online listing. */
export function filterOnline(all: OnlineEntry[], search: string, sort: OnlineSort): OnlineEntry[] {
  const q = search.trim().toLowerCase();
  const hit = (o: OnlineEntry) =>
    !q ||
    [o.entry.name, o.entry.description, o.entry.uploader].some((t) => (t ?? "").toLowerCase().includes(q)) ||
    o.entry.mods.some((m) => m.modId.toLowerCase().includes(q));
  const out = all.filter(hit);
  const name = (o: OnlineEntry) => (o.entry.name ?? "").toLowerCase();
  out.sort((a, b) => {
    switch (sort) {
      case "subscribers":
        return b.subscribers - a.subscribers || name(a).localeCompare(name(b));
      case "mods":
        return b.entry.mods.length - a.entry.mods.length;
      case "name":
        return name(a).localeCompare(name(b));
      default:
        return (b.entry.updatedMs ?? 0) - (a.entry.updatedMs ?? 0);
    }
  });
  // Your own and followed lists first, whatever the sort.
  return [...out.filter((o) => o.mine), ...out.filter((o) => !o.mine && o.subscribed), ...out.filter((o) => !o.mine && !o.subscribed)];
}
