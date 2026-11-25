import type { MouseEvent, ReactNode } from "react";
import type { IconName } from "../../components/Icon";
import { iconThemeClass, ModlistIcon } from "./ModlistIcon";

/** A modlist card: the title across the top, then the icon beside the facts. The description is
 * only in the details panel. */
export function ModlistCard({ title, icon, iconColor, kindIcon, facts, date, badges, selected, onSelect, onLoad, onContextMenu }: {
  title: string;
  /** Chosen Nerd Font icon name; `null` shows `kindIcon`. */
  icon?: string | null;
  /** Color theme id; `null` = default colors. */
  iconColor?: string | null;
  kindIcon: IconName;
  facts: string[];
  date?: string;
  badges?: ReactNode;
  selected: boolean;
  onSelect: (event: MouseEvent<HTMLButtonElement>) => void;
  onLoad?: () => void;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <li className="modlist-card-item">
      <button
        type="button"
        className={`mod-card modlist-card${selected ? " selected" : ""}`}
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
        <span className="modlist-card-row">
          <span className={`card-logo modlist-card-icon icon-${kindIcon}${iconThemeClass(iconColor)}`}><ModlistIcon icon={icon} fallback={kindIcon} /></span>
          <span className="card-body">
            {badges && <span className="modlist-card-badges">{badges}</span>}
            <span className="muted small card-stats">{facts.join(" · ")}</span>
            {date && <span className="muted small">{date}</span>}
          </span>
        </span>
      </button>
    </li>
  );
}
