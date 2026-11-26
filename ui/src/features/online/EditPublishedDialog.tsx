import { useState } from "react";
import { useEditPublished } from "../../api";
import { Modal } from "../modlists/Modal";
import { IconPicker, iconThemeClass, ModlistIcon } from "../modlists/ModlistIcon";
import { Icon } from "../../components/Icon";
import { useErrorToast } from "../../components/Toast";

/** What the dialog edits: one of your published modlists. */
export interface PublishedDetails {
  registryId: string;
  name: string;
  description: string | null;
  gameVersion: string | null;
  icon: string | null;
  iconColor: string | null;
  revision: number;
  subscribers: number;
}

/** Edits one of your published modlists, online and in its saved modlist: icon, description and
 * game version. The name stays. */
export function EditPublishedDialog({ list: e, onDone, onClose }: { list: PublishedDetails; onDone: (message: string) => void; onClose: () => void }) {
  const save = useEditPublished();
  useErrorToast(save.error);
  const [icon, setIcon] = useState<string | null>(e.icon);
  const [iconColor, setIconColor] = useState<string | null>(e.iconColor);
  const [description, setDescription] = useState(e.description ?? "");
  const [gameVersion, setGameVersion] = useState(e.gameVersion ?? "");
  const [picking, setPicking] = useState(false);
  const name = e.name;

  const submit = () =>
    save.mutate(
      { registryId: e.registryId, edit: { description, gameVersion, icon: icon ?? "", iconColor: iconColor ?? "" } },
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
          <button type="button" className="link-button small" onClick={() => setPicking(true)}><Icon name="edit" size={13} />Choose…</button>
        </div>
        <label className="form-label" htmlFor="ep-desc">Description</label>
        <textarea id="ep-desc" rows={4} autoFocus value={description} onChange={(ev) => setDescription(ev.target.value)} />
        <label className="form-label" htmlFor="ep-gv">For game version</label>
        <input id="ep-gv" type="text" value={gameVersion} onChange={(ev) => setGameVersion(ev.target.value)} />
      </div>
      <p className="muted small">
        This keeps revision {e.revision} and doesn't notify followers. To change the mods, update the saved modlist and
        publish its changes: that makes a new revision{e.subscribers > 0 ? ` and tells your ${e.subscribers} follower${e.subscribers === 1 ? "" : "s"}` : ""}.
      </p>
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" onClick={onClose} disabled={save.isPending}><Icon name="close" />Cancel</button>
        <button type="button" className="primary" disabled={save.isPending} onClick={submit}>
          <Icon name="save" />{save.isPending ? "Saving…" : "Save"}
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
