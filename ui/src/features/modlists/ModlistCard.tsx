import type { MouseEvent, ReactNode, Ref } from "react";
import { Icon, type IconName } from "../../components/Icon";
import { iconThemeClass, ModlistIcon } from "./ModlistIcon";

/** A modlist card: the title across the top, then the icon beside the facts. The description is
 * only in the details panel. */
export function ModlistCard({ title, icon, iconColor, kindIcon, rows, facts, date, badges, following, published, selected, buttonRef, onSelect, onLoad, onContextMenu }: {
  title: string;
  /** Follows an online modlist: a small icon in the top right corner. */
  following?: boolean;
  /** One of your published modlists: a thicker colored border and a cloud in the corner. */
  published?: boolean;
  /** Chosen Nerd Font icon name; `null` shows `kindIcon`. */
  icon?: string | null;
  /** Color theme id; `null` = default colors. */
  iconColor?: string | null;
  kindIcon: IconName;
  rows: ReactNode[];
  facts: string[];
  date?: string;
  badges?: ReactNode;
  selected: boolean;
  buttonRef?: Ref<HTMLButtonElement>;
  onSelect: (event: MouseEvent<HTMLButtonElement>) => void;
  onLoad?: () => void;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <li className="modlist-card-item">
      <button
        ref={buttonRef}
        type="button"
        className={`mod-card modlist-card${published ? " published" : ""}${selected ? " selected" : ""}`}
        title={title}
        aria-pressed={selected}
        aria-haspopup="menu"
        onClick={onSelect}
        onContextMenu={onContextMenu}
        onDoubleClick={onLoad}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || !onLoad) return;
          event.preventDefault();
          if (!event.repeat) onLoad();
        }}
      >
        <span className="card-title">{title}</span>
        {following && (
          <span className="card-following" title="Following: you get the author's updates" aria-label="Following">
            <Icon name="follow" />
          </span>
        )}
        {published && (
          <span className="card-published" title="Published: other players can find, load and follow it" aria-label="Published">
            <Icon name="published" />
          </span>
        )}
        <span className="modlist-card-foot">
          <span className={`card-logo modlist-card-icon${iconThemeClass(iconColor)}`}><ModlistIcon icon={icon} fallback={kindIcon} /></span>
          <span className="modlist-card-foot-meta">
            <span className="modlist-card-lines">
              {rows.map((row, i) => <span key={i} className="small">{row}</span>)}
            </span>
            <span className="modlist-card-foot-line">
              {badges && <span className="modlist-card-badges">{badges}</span>}
              <span className="muted small">{[...facts, date].filter(Boolean).join(" · ")}</span>
            </span>
          </span>
        </span>
      </button>
    </li>
  );
}
