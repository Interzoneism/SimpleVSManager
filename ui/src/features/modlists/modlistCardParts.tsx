import type { CloudSlotView, ModlistKind, ModlistSummary } from "../../bindings";
import type { IconName } from "../../components/Icon";
import { formatAgo } from "../../lib/format";
import { kindLabel } from "./modlistView";

export const kindIcons: Record<ModlistKind, IconName> = { group: "layers", modlist: "lists", backup: "history" };

/** What a saved modlist's card shows (everything but selection and handlers), shared by the
 * Modlists page and the "Load a modlist" picker. `slot` adds followers and the online revision. */
export function modlistCardParts(m: ModlistSummary, slot?: CloudSlotView | null) {
  const rows = [
    m.group ? <>Group of <strong>{m.group.members.length}</strong> lists</>
      : m.uploader ? <>By <strong>{m.uploader}</strong></>
      : kindLabel(m.kind),
    m.error ? "Can't be read" : <><strong>{m.mods.length}</strong> mod{m.mods.length === 1 ? "" : "s"}</>,
    m.gameVersion ? `VS ${m.gameVersion}` : "Any game version",
  ];
  const facts = [
    m.subscription && `Revision ${m.subscription.syncedRevision}`,
    m.published && `Revision ${slot?.info.revision ?? m.published.revision}`,
    m.configCount > 0 && `${m.configCount} config${m.configCount === 1 ? "" : "s"}`,
    m.group && m.group.conflicts.length > 0 && `${m.group.conflicts.length} to check`,
    !m.active && m.downloadsNeeded > 0 && `${m.downloadsNeeded} to download`,
  ].filter((fact): fact is string => !!fact);
  return {
    title: m.name,
    icon: m.icon,
    iconColor: m.iconColor,
    kindIcon: kindIcons[m.kind],
    rows,
    facts,
    date: m.modifiedMs ? formatAgo(m.modifiedMs) : undefined,
    followers: slot?.subscribers,
    following: !!m.subscription,
    published: !!m.published,
    stacked: m.kind === "group",
    badges: (m.active || m.subscription?.updateAvailable || m.published?.changed) && <>
      {m.active && <span className="badge active-badge">Active</span>}
      {m.subscription?.updateAvailable && <span className="badge update-badge">Update available</span>}
      {m.published?.changed && <span className="badge published-badge" title="Not published yet">Changes to publish</span>}
    </>,
  };
}
