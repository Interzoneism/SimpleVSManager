import type { ReactNode } from "react";

export function ModlistSection({ title, count, empty, collapsed = false, onToggle, children }: {
  title: string;
  count: number;
  empty?: string;
  collapsed?: boolean;
  onToggle?: () => void;
  children: ReactNode;
}) {
  const heading = <>{title} <span className="muted small">({count})</span></>;
  return (
    <section className="modlist-section">
      <h2 className="modlist-section-title">
        {onToggle ? (
          <button type="button" className="section-toggle" aria-expanded={!collapsed} onClick={onToggle}>
            <span aria-hidden="true">{collapsed ? "▸" : "▾"} </span>{heading}
          </button>
        ) : heading}
      </h2>
      {!collapsed && (empty
        ? <p className="muted small section-empty">{empty}</p>
        : <ul className="card-grid modlist-card-grid" aria-label={title}>{children}</ul>)}
    </section>
  );
}
