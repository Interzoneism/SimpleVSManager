import { useState } from "react";
import { useModlists, usePublishCurrent, usePublishModlist } from "../../api";
import { Modal } from "../modlists/Modal";
import { IconPicker, iconThemeClass, ModlistIcon } from "../modlists/ModlistIcon";

const CURRENT = "";

/** Publishes a new online modlist into an empty cloud slot: your enabled mods, or a saved modlist. */
export function PublishNewDialog({ slot, freeSlots, onDone, onClose }: {
  slot: string;
  freeSlots: number;
  onDone: (message: string) => void;
  onClose: () => void;
}) {
  const lists = useModlists();
  const publishSaved = usePublishModlist();
  const publishCurrent = usePublishCurrent();
  const [source, setSource] = useState(CURRENT);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [icon, setIcon] = useState<string | null>(null);
  const [iconColor, setIconColor] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const saved = (lists.data ?? []).filter((l) => l.kind !== "backup" && !l.error);
  const busy = publishSaved.isPending || publishCurrent.isPending;
  const error = publishSaved.error ?? publishCurrent.error;
  const done = (s: { name: string | null; revision: number }) => {
    onDone(`Published "${s.name ?? name}".`);
    onClose();
  };

  const submit = () => {
    if (source === CURRENT) {
      publishCurrent.mutate({ slot, name, description: description || null, icon, iconColor }, { onSuccess: done });
    } else {
      publishSaved.mutate({ id: source, slot, replaces: null }, { onSuccess: done });
    }
  };

  return (
    <Modal title="Publish a modlist" onClose={onClose} busy={busy}>
      <p className="muted small">
        Everyone can see and load it, and follow it to get your updates. {freeSlots} of 5 slots free.
      </p>
      <div className="form-grid">
        <label className="form-label" htmlFor="pn-source">Mods</label>
        <select id="pn-source" value={source} onChange={(e) => setSource(e.target.value)}>
          <option value={CURRENT}>Your enabled mods (as they are now)</option>
          {saved.map((l) => <option key={l.id} value={l.id}>Saved modlist: {l.name}</option>)}
        </select>
        {source === CURRENT && (
          <>
            <label className="form-label" htmlFor="pn-name">Name</label>
            <input id="pn-name" type="text" autoFocus value={name} onChange={(e) => setName(e.target.value)} />
            <span className="form-label">Icon</span>
            <div className="icon-field">
              <button type="button" className={`details-icon-button${iconThemeClass(iconColor)}`} aria-label="Choose an icon" title="Choose an icon" onClick={() => setPicking(true)}>
                <ModlistIcon icon={icon} fallback="community" />
              </button>
              <button type="button" className="link-button small" onClick={() => setPicking(true)}>Choose…</button>
            </div>
            <label className="form-label" htmlFor="pn-desc">Description</label>
            <textarea id="pn-desc" rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
          </>
        )}
      </div>
      <p className="muted small">The name can't be changed after publishing.</p>
      {error && <p className="notice error-text">{error.message}</p>}
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" onClick={onClose} disabled={busy}>Cancel</button>
        <button type="button" className="primary" disabled={busy || (source === CURRENT && !name.trim())} onClick={submit}>
          {busy ? "Publishing…" : "Publish"}
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
