import { useState } from "react";
import type { OnlineEntry } from "../../bindings";
import { useEditPublished } from "../../api";
import { Modal } from "../modlists/Modal";
import { IconPicker, iconThemeClass, ModlistIcon } from "../modlists/ModlistIcon";

/** Edits one of your published modlists: icon, description and versions. The name stays. */
export function EditPublishedDialog({ list, onDone, onClose }: { list: OnlineEntry; onDone: (message: string) => void; onClose: () => void }) {
  const e = list.entry;
  const save = useEditPublished();
  const [icon, setIcon] = useState<string | null>(e.icon);
  const [iconColor, setIconColor] = useState<string | null>(e.iconColor);
  const [description, setDescription] = useState(e.description ?? "");
  const [version, setVersion] = useState(e.version ?? "");
  const [gameVersion, setGameVersion] = useState(e.gameVersion ?? "");
  const [picking, setPicking] = useState(false);
  const name = e.name ?? "Unnamed modlist";

  const submit = () =>
    save.mutate(
      { registryId: e.registryId, edit: { description, version, gameVersion, icon: icon ?? "", iconColor: iconColor ?? "" } },
      {
        onSuccess: (slot) => {
          onDone(`Updated "${name}" (revision ${slot.revision}).`);
          onClose();
        },
      },
    );

  return (
    <Modal title="Edit published modlist" onClose={onClose} busy={save.isPending}>
      <div className="form-grid">
        <label className="form-label" htmlFor="ep-name">Name</label>
        <input id="ep-name" type="text" value={name} disabled title="The name of a published modlist can't be changed" />
        <span className="form-label">Icon</span>
        <div className="icon-field">
          <button type="button" className={`details-icon-button${iconThemeClass(iconColor)}`} aria-label="Choose an icon" title="Choose an icon" onClick={() => setPicking(true)}>
            <ModlistIcon icon={icon} fallback="community" />
          </button>
          <button type="button" className="link-button small" onClick={() => setPicking(true)}>Choose…</button>
        </div>
        <label className="form-label" htmlFor="ep-desc">Description</label>
        <textarea id="ep-desc" rows={4} autoFocus value={description} onChange={(ev) => setDescription(ev.target.value)} />
        <label className="form-label" htmlFor="ep-version">Modlist version</label>
        <input id="ep-version" type="text" placeholder="e.g. 1.0" value={version} onChange={(ev) => setVersion(ev.target.value)} />
        <label className="form-label" htmlFor="ep-gv">For game version</label>
        <input id="ep-gv" type="text" value={gameVersion} onChange={(ev) => setGameVersion(ev.target.value)} />
      </div>
      <p className="muted small">
        The mods stay as they are; to change them, publish a new revision from your saved modlist.
        {list.subscribers > 0 && ` ${list.subscribers} follower${list.subscribers === 1 ? "" : "s"} will see an update.`}
      </p>
      {save.isError && <p className="notice error-text">{save.error.message}</p>}
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" onClick={onClose} disabled={save.isPending}>Cancel</button>
        <button type="button" className="primary" disabled={save.isPending} onClick={submit}>
          {save.isPending ? "Saving…" : "Save"}
        </button>
      </div>
      {picking && (
        <IconPicker
          current={icon}
          currentColor={iconColor}
          fallback="community"
          onPick={(picked, color) => {
            setIcon(picked);
            setIconColor(color);
            setPicking(false);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </Modal>
  );
}
