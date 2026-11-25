import { useMemo, useState } from "react";
import { Icon, type IconName } from "../../components/Icon";
import { Modal } from "./Modal";
import { NERD_ICONS } from "./nerdIcons";

const byName = new Map(NERD_ICONS.map(([name, label, codePoint]) => [name, { label, glyph: String.fromCodePoint(codePoint) }]));

/** The glyph for a stored icon name, or `null` when it isn't one we bundle. */
export function nerdGlyph(name: string | null | undefined) {
  return (name && byName.get(name)) || null;
}

/** A modlist's chosen Nerd Font icon, or the default icon for its kind. */
export function ModlistIcon({ icon, fallback }: { icon?: string | null; fallback: IconName }) {
  const glyph = nerdGlyph(icon);
  return glyph ? <span className="nerd-icon" aria-hidden="true">{glyph.glyph}</span> : <Icon name={fallback} />;
}

/** Searches labels and names (`sword` finds both Font Awesome and Material Design swords). */
export function filterIcons(query: string) {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return NERD_ICONS.filter(([name, label]) => words.every((w) => name.includes(w) || label.toLowerCase().includes(w)));
}

/** Picks a card icon. `onPick(null)` means "use the default icon". */
export function IconPicker({ current, fallback, busy = false, error, onPick, onClose }: {
  current?: string | null;
  fallback: IconName;
  busy?: boolean;
  error?: string | null;
  onPick: (icon: string | null) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const shown = useMemo(() => filterIcons(query), [query]);
  return (
    <Modal title="Choose an icon" onClose={onClose} busy={busy}>
      <div className="icon-picker">
        <input
          type="search"
          className="search"
          placeholder="Search icons, e.g. sword, tree, castle"
          aria-label="Search icons"
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <ul className="icon-picker-grid" aria-label="Icons">
          {!query && (
            <li>
              <button
                type="button"
                className={`icon-choice${!nerdGlyph(current) ? " selected" : ""}`}
                title="Default icon"
                aria-label="Default icon"
                aria-pressed={!nerdGlyph(current)}
                disabled={busy}
                onClick={() => onPick(null)}
              >
                <Icon name={fallback} />
              </button>
            </li>
          )}
          {shown.map(([name, label, codePoint]) => (
            <li key={name}>
              <button
                type="button"
                className={`icon-choice${current === name ? " selected" : ""}`}
                title={label}
                aria-label={label}
                aria-pressed={current === name}
                disabled={busy}
                onClick={() => onPick(name)}
              >
                <span className="nerd-icon" aria-hidden="true">{String.fromCodePoint(codePoint)}</span>
              </button>
            </li>
          ))}
        </ul>
        {shown.length === 0 && <p className="muted small">No icons match.</p>}
        {error && <p className="notice error-text">{error}</p>}
        <p className="muted small">
          Icons from Symbols Nerd Font (Font Awesome Free and Material Design Icons).
        </p>
      </div>
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" onClick={onClose} disabled={busy}>Cancel</button>
      </div>
    </Modal>
  );
}
