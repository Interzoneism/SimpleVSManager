import { useState } from "react";
import type { ModlistSummary } from "../../bindings";
import { useSaveModlistGroup } from "../../api";
import { kindLabel } from "./modlistView";
import { Modal } from "./Modal";
import { Icon } from "../../components/Icon";
import { useErrorToast } from "../../components/Toast";

interface Props {
  /** Group being edited, or `null` for a new one. */
  group: ModlistSummary | null;
  all: ModlistSummary[];
  initialMembers: string[];
  onSaved: (s: ModlistSummary) => void;
  onClose: () => void;
}

/** Creates or edits a modlist group: a name and the modlists it combines, in order. */
export function GroupDialog({ group, all, initialMembers, onSaved, onClose }: Props) {
  const save = useSaveModlistGroup();
  useErrorToast(save.error);
  const [name, setName] = useState(group?.name ?? "");
  const [description, setDescription] = useState(group?.description ?? "");
  // Members as ids for our modlists, references for anything else (kept as they are).
  const [members, setMembers] = useState<string[]>(
    group ? (group.group?.members ?? []).map((m) => m.id ?? m.reference) : initialMembers,
  );
  const choices = all.filter((m) => m.kind !== "group");
  const outside = (group?.group?.members ?? []).filter((m) => !m.id);

  const toggle = (id: string, on: boolean) => setMembers((prev) => (on ? [...prev, id] : prev.filter((m) => m !== id)));

  return (
    <Modal title={group ? `Edit group "${group.name}"` : "New modlist group"} onClose={onClose} busy={save.isPending}>
      <div className="form-grid">
        <label className="form-label" htmlFor="g-name">Name</label>
        <input id="g-name" type="text" autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        <label className="form-label" htmlFor="g-desc">Description</label>
        <textarea id="g-desc" rows={2} value={description} onChange={(e) => setDescription(e.target.value)} />
      </div>
      <section className="config-picker">
        <h3>Modlists in the group ({members.length})</h3>
        <p className="muted small">
          Loading the group installs every mod from these lists. When two lists disagree about a mod, the newest version
          wins unless you pick another one in the group's details.
        </p>
        <ul className="check-list">
          {outside.map((m) => (
            <li key={m.reference}>
              <label className="checkbox">
                <input type="checkbox" checked={members.includes(m.reference)} onChange={(e) => toggle(m.reference, e.target.checked)} />
                <span>{m.name}</span>
                <span className="muted small">{m.missing ? "missing file" : m.reference}</span>
              </label>
            </li>
          ))}
          {choices.map((m) => (
            <li key={m.id}>
              <label className="checkbox">
                <input type="checkbox" checked={members.includes(m.id)} onChange={(e) => toggle(m.id, e.target.checked)} />
                <span>{m.name}</span>
                <span className="muted small">{kindLabel(m.kind)} · {m.mods.length} mods</span>
              </label>
            </li>
          ))}
        </ul>
      </section>
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" onClick={onClose} disabled={save.isPending}><Icon name="close" />Cancel</button>
        <button
          type="button"
          className="primary"
          disabled={!name.trim() || members.length === 0 || save.isPending}
          onClick={() =>
            save.mutate(
              { id: group?.id ?? null, name, description: description || null, version: group?.version ?? null, members },
              { onSuccess: onSaved },
            )
          }
        >
          <Icon name="save" />Save group
        </button>
      </div>
    </Modal>
  );
}
