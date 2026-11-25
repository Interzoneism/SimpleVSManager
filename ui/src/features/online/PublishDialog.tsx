import { useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { useCloudSlots, useDeleteCloudSlot, useModlists, usePublishModlist } from "../../api";
import { Modal } from "../modlists/Modal";

/** Your five cloud slots: publish a saved modlist, publish a new revision, or delete. */
export function PublishDialog({ onClose, onDone }: { onClose: () => void; onDone: (message: string) => void }) {
  const slots = useCloudSlots(true);
  const lists = useModlists();
  const publish = usePublishModlist();
  const del = useDeleteCloudSlot();
  const [choice, setChoice] = useState<Record<string, string>>({});
  const local = (lists.data ?? []).filter((l) => l.kind !== "backup" && !l.error);
  const busy = publish.isPending || del.isPending;

  const doPublish = async (slot: string, replaces: string | null, subscribers: number, name: string | null) => {
    const id = choice[slot];
    if (!id) return;
    if (replaces) {
      const ok = await ask(
        `Replace "${name ?? "this modlist"}" online? If the mods changed, this makes a new revision${subscribers ? ` and ${subscribers} follower${subscribers === 1 ? "" : "s"} will see that an update is available` : ""}.`,
        { title: "Publish", okLabel: "Publish", cancelLabel: "Cancel" },
      );
      if (!ok) return;
    }
    publish.mutate({ id, slot, replaces }, { onSuccess: (s) => onDone(`Published "${s.name}" (revision ${s.revision}).`) });
  };

  const doDelete = async (slot: string, name: string | null, subscribers: number) => {
    const ok = await ask(
      `Delete "${name ?? slot}" from the cloud?${subscribers ? ` ${subscribers} follower${subscribers === 1 ? "" : "s"} keep their copies, but get no more updates.` : ""} This can't be undone.`,
      { title: "Delete online modlist", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
    );
    if (ok) del.mutate(slot, { onSuccess: () => onDone(`Deleted "${name ?? slot}" from the cloud.`) });
  };

  return (
    <Modal title="Your online modlists" onClose={onClose} busy={busy}>
      {slots.isPending && <p className="muted">Reading your cloud slots…</p>}
      {slots.isError && <p className="notice error-text">{slots.error.message}</p>}
      {slots.data && (
        <>
          <p className="muted small">
            Published as <strong>{slots.data.playerName}</strong> (your player name). Everyone can see and load these.
          </p>
          <ul className="slot-list">
            {slots.data.slots.map((s, i) => {
              const slot = `slot${i + 1}`;
              return (
                <li key={slot} className="slot-row">
                  <div className="slot-head">
                    <strong>Slot {i + 1}</strong>
                    {s ? (
                      <span className="muted small">
                        {s.info.name ?? "Unnamed"} · revision {s.info.revision} · {s.info.modCount} mods · {s.subscribers} following
                      </span>
                    ) : (
                      <span className="muted small">empty</span>
                    )}
                  </div>
                  <div className="slot-actions">
                    <select
                      aria-label={`Modlist for slot ${i + 1}`}
                      value={choice[slot] ?? ""}
                      onChange={(e) => setChoice({ ...choice, [slot]: e.target.value })}
                    >
                      <option value="">{s ? "Replace with…" : "Publish…"}</option>
                      {local.map((l) => (
                        <option key={l.id} value={l.id}>{l.name}</option>
                      ))}
                    </select>
                    <button
                      type="button"
                      className="primary"
                      disabled={busy || !choice[slot]}
                      onClick={() => void doPublish(slot, s?.info.registryId ?? null, s?.subscribers ?? 0, s?.info.name ?? null)}
                    >
                      {s ? "Publish new revision" : "Publish"}
                    </button>
                    {s && (
                      <button type="button" className="danger" disabled={busy} onClick={() => void doDelete(slot, s.info.name, s.subscribers)}>
                        Delete
                      </button>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      )}
      {(publish.error ?? del.error) && <p className="notice error-text">{(publish.error ?? del.error)!.message}</p>}
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" onClick={onClose} disabled={busy}>Close</button>
      </div>
    </Modal>
  );
}
