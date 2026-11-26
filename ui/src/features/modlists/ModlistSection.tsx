import type { ReactNode } from "react";
import { Icon } from "../../components/Icon";

/** A titled group of modlist cards. With `onToggle` the title collapses it; nothing inside
 * (toolbar, cards) is rendered while collapsed. */
export function ModlistSection({ title, count, empty, collapsed = false, onToggle, toolbar, children }: {
  title: string;
  /** Shown after the title; leave out when it isn't known (not loaded). */
  count?: number;
  empty?: string;
  collapsed?: boolean;
  onToggle?: () => void;
  /** Controls shown under the title while expanded. */
  toolbar?: ReactNode;
  children: ReactNode;
}) {
  const heading = <>{title}{count != null && <> <span className="muted small">({count})</span></>}</>;
  return (
    <section className="modlist-section">
      <h2 className="modlist-section-title">
        {onToggle ? (
          <button type="button" className="section-toggle" aria-expanded={!collapsed} onClick={onToggle}>
            <Icon name={collapsed ? "chevronRight" : "chevronDown"} size={15} />{heading}
          </button>
        ) : heading}
      </h2>
      {!collapsed && toolbar}
      {!collapsed && (empty
        ? <p className="muted small section-empty">{empty}</p>
        : <ul className="card-grid modlist-card-grid" aria-label={title}>{children}</ul>)}
    </section>
  );
}
