import type { ModlistModView } from "../../bindings";
import { useModlistModCards, useOpenUrl } from "../../api";
import { formatCount } from "../../lib/format";
import { Logo } from "../browser/ModBrowser";
import { modLocation } from "./modlistView";

/** The list's mods with names, logos and downloads from the Mod DB, and where each one is. */
export function ModCards({ mods }: { mods: ModlistModView[] }) {
  const ids = mods.map((m) => m.modId);
  const cards = useModlistModCards(ids, true);
  const openUrl = useOpenUrl();
  const byId = new Map((cards.data ?? []).map((c) => [c.modId.toLowerCase(), c]));
  return (
    <div className="details-section">
      <h3>
        Mods ({mods.length}){cards.isFetching && <span className="muted small"> · looking up…</span>}
      </h3>
      <ul className="mod-cards">
        {mods.map((m) => {
          const c = byId.get(m.modId.toLowerCase());
          const name = c?.name ?? m.modId;
          const where = modLocation(m.version, c?.installedVersion ?? null, c?.libraryVersions ?? []);
          const facts = [m.version, c?.author, c?.downloads != null ? `${formatCount(c.downloads)} downloads` : null, c?.notOnModDb ? "not on the Mod DB" : null];
          return (
            <li key={m.modId} className={`mod-card-row${m.isActive === false ? " off" : ""}`}>
              <Logo url={c?.logoUrl ?? null} name={name} className="mini-logo" />
              <span className="mod-card-text">
                {c?.pageUrl ? (
                  <button type="button" className="link-button mod-card-name" onClick={() => openUrl.mutate(c.pageUrl!)}>
                    {name}
                  </button>
                ) : (
                  <span className="mod-card-name">{name}</span>
                )}
                <span className="muted small">{facts.filter(Boolean).join(" · ")}</span>
              </span>
              {(m.isActive === false || where) && (
                <span className="small mod-card-where">
                  {m.isActive === false && <span className="muted">{where ? "off · " : "off"}</span>}
                  {where && <span className={where === "not downloaded" ? "muted" : undefined}>{where}</span>}
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
