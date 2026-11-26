import { useMemo, useState } from "react";
import { Icon, type IconName } from "../../components/Icon";
import { Modal } from "./Modal";
import { NERD_ICONS } from "./nerdIcons";
import { useErrorToast } from "../../components/Toast";

const byName = new Map(NERD_ICONS.map(([name, label, codePoint]) => [name, { label, glyph: String.fromCodePoint(codePoint) }]));

/** Muted icon colors: [id, label]. The colors themselves are `.icon-theme-<id>` in styles.css. */
export const ICON_THEMES = [
  ["sage", "Sage"],
  ["slate", "Slate"],
  ["plum", "Plum"],
  ["clay", "Clay"],
  ["ochre", "Ochre"],
  ["teal", "Teal"],
  ["rose", "Rose"],
  ["indigo", "Indigo"],
  ["moss", "Moss"],
  ["parchment", "Parchment"],
] as const;

const themeIds = new Set<string>(ICON_THEMES.map(([id]) => id));

/** Class for a stored color theme id (nothing for the default or an unknown id). */
export function iconThemeClass(color: string | null | undefined) {
  return color && themeIds.has(color) ? ` icon-theme-${color}` : "";
}

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

/** Picks a card icon and its colors. `null` means the default icon / default colors. */
export function IconPicker({ current, currentColor, fallback, busy = false, error, onPick, onClose }: {
  current?: string | null;
  currentColor?: string | null;
  fallback: IconName;
  busy?: boolean;
  error?: string | null;
  onPick: (icon: string | null, color: string | null) => void;
  onClose: () => void;
}) {
  useErrorToast(error);
  const [query, setQuery] = useState("");
  const [icon, setIcon] = useState<string | null>(nerdGlyph(current) ? current! : null);
  const [color, setColor] = useState<string | null>(iconThemeClass(currentColor) ? currentColor! : null);
  const shown = useMemo(() => filterIcons(query), [query]);
  const theme = iconThemeClass(color);
  return (
    <Modal title="Choose an icon" onClose={onClose} busy={busy}>
      <div className="icon-picker">
        <div className="icon-preview-row">
          <span className={`details-icon-button icon-preview${theme}`} aria-hidden="true">
            <ModlistIcon icon={icon} fallback={fallback} />
          </span>
          <ul className="icon-swatches" aria-label="Colors">
            <li>
              <button
                type="button"
                className={`icon-swatch icon-swatch-default${color === null ? " selected" : ""}`}
                title="Default colors"
                aria-label="Default colors"
                aria-pressed={color === null}
                onClick={() => setColor(null)}
              />
            </li>
            {ICON_THEMES.map(([id, label]) => (
              <li key={id}>
                <button
                  type="button"
                  className={`icon-swatch icon-theme-${id}${color === id ? " selected" : ""}`}
                  title={label}
                  aria-label={label}
                  aria-pressed={color === id}
                  onClick={() => setColor(id)}
                >
                  <span className="icon-swatch-dot" />
                </button>
              </li>
            ))}
          </ul>
        </div>
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
                className={`icon-choice${icon === null ? " selected" : ""}`}
                title="Default icon"
                aria-label="Default icon"
                aria-pressed={icon === null}
                onClick={() => setIcon(null)}
                onDoubleClick={() => onPick(null, color)}
              >
                <Icon name={fallback} />
              </button>
            </li>
          )}
          {shown.map(([name, label, codePoint]) => (
            <li key={name}>
              <button
                type="button"
                className={`icon-choice${icon === name ? " selected" : ""}`}
                title={label}
                aria-label={label}
                aria-pressed={icon === name}
                onClick={() => setIcon(name)}
                onDoubleClick={() => onPick(name, color)}
              >
                <span className="nerd-icon" aria-hidden="true">{String.fromCodePoint(codePoint)}</span>
              </button>
            </li>
          ))}
        </ul>
        {shown.length === 0 && <p className="muted small">No icons match.</p>}
        <p className="muted small">Icons from Symbols Nerd Font (Font Awesome Free and Material Design Icons).</p>
      </div>
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" onClick={onClose} disabled={busy}><Icon name="close" />Cancel</button>
        <button type="button" className="primary" disabled={busy} onClick={() => onPick(icon, color)}>
          <Icon name="check" />{busy ? "Saving…" : "Use this icon"}
        </button>
      </div>
    </Modal>
  );
}
