import { useCloudSlots } from "../../api";
import type { ModlistSummary } from "../../bindings";
import { useLayoutPreference } from "../../components/SplitWorkspace";
import { Modal } from "./Modal";
import { ModlistCard } from "./ModlistCard";
import { modlistCardParts } from "./modlistCardParts";
import { pickerOrder, rememberPick } from "./modlistView";

/** Ids of the most recently picked modlists, newest first (stored as JSON). */
export function useRecentPicks() {
  const [raw, setRaw] = useLayoutPreference<string>("modlist-recent-picks", "[]");
  let recent: string[] = [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) recent = parsed.filter((id): id is string => typeof id === "string");
  } catch {
    // Start over with no recent picks.
  }
  return { recent, remember: (id: string) => setRaw(JSON.stringify(rememberPick(recent, id))) };
}

/** "Load a modlist": the saved, published and followed modlists and groups as cards (no backups).
 * Picking one hands it to the usual load flow. Recently picked lists come first. */
export function ModlistPicker({ lists, onPick, onClose }: {
  lists: ModlistSummary[];
  onPick: (entry: ModlistSummary) => void;
  onClose: () => void;
}) {
  const { recent, remember } = useRecentPicks();
  // Only what the Modlists page already fetched: followers and online revisions when known.
  const slots = useCloudSlots(false).data?.slots ?? [];
  const slotFor = (registryId: string | undefined) =>
    (registryId && slots.find((s) => s?.info.registryId === registryId)) || null;
  const pick = (entry: ModlistSummary) => {
    remember(entry.id);
    onPick(entry);
  };
  return (
    <Modal title="Load a modlist" onClose={onClose}>
      <ul className="card-grid modlist-card-grid modlist-picker" aria-label="Modlists">
        {pickerOrder(lists, recent).map((m) => (
          <ModlistCard
            key={m.id}
            {...modlistCardParts(m, slotFor(m.published?.registryId))}
            selected={false}
            onSelect={() => pick(m)}
            onContextMenu={(event) => event.preventDefault()}
          />
        ))}
      </ul>
      <div className="modal-actions">
        <button type="button" onClick={onClose}>Cancel</button>
      </div>
    </Modal>
  );
}
