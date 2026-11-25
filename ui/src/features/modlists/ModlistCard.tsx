import type { MouseEvent, ReactNode } from "react";
import { Icon, type IconName } from "../../components/Icon";

export function ModlistCard({ title, icon, description, facts, date, badges, selected, onSelect, onLoad, onContextMenu }: {
  title: string;
  icon: IconName;
  description?: string | null;
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
        <span className={`card-logo modlist-card-icon icon-${icon}`}><Icon name={icon} /></span>
        <span className="card-body">
          <span className="card-title">{title}</span>
          {badges && <span className="modlist-card-badges">{badges}</span>}
          {description && <span className="card-summary small">{description}</span>}
          <span className="muted small card-stats">{facts.join(" · ")}</span>
          {date && <span className="muted small">{date}</span>}
        </span>
      </button>
    </li>
  );
}
