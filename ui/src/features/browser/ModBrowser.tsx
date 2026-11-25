import { useEffect, useMemo, useState, type MouseEvent } from "react";
import type { BrowseEntry, BrowseQuery, BrowseSort } from "../../bindings";
import {
  defaultBrowseQuery,
  useBrowseFacets,
  useBrowseMods,
  useBrowseThumbnails,
  useEnvironment,
  useOpenUrl,
  useRefreshCatalogue,
  useUpdatePreferences,
} from "../../api";
import { formatAgo, formatCount } from "../../lib/format";
import { ModDetailsWindow } from "./ModDetailsWindow";
import { copyText, useContextMenu } from "../../components/ContextMenu";
import { Icon } from "../../components/Icon";

const SORTS: { value: BrowseSort; label: string }[] = [
  { value: "relevance", label: "Best match" },
  { value: "downloads", label: "Most downloaded" },
  { value: "follows", label: "Most followed" },
  { value: "trending", label: "Trending" },
  { value: "comments", label: "Most comments" },
  { value: "recentlyUpdated", label: "Recently updated" },
  { value: "recentlyAdded", label: "Newest" },
  { value: "name", label: "Name" },
];

const PAGE = 60;

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

/** The "Mod DB" tab: search the whole catalogue and install mods. */
export function ModBrowser({ enabled }: { enabled: boolean }) {
  const env = useEnvironment();
  const offline = !!env.data?.settings.offlineMode;
  const [text, setText] = useState("");
  const [query, setQuery] = useState<BrowseQuery>(defaultBrowseQuery);
  const [gameVersion, setGameVersion] = useState<string>("");
  const [selected, setSelected] = useState<BrowseEntry | null>(null);
  const debouncedText = useDebounced(text, 200);

  const effective = useMemo(() => ({ ...query, text: debouncedText }), [query, debouncedText]);
  const results = useBrowseMods(effective, gameVersion || null, enabled);
  const facets = useBrowseFacets(enabled);
  const refresh = useRefreshCatalogue();
  const prefs = useUpdatePreferences();
  const openUrl = useOpenUrl();
  const openMenu = useContextMenu();

  // New search: back to the first page.
  useEffect(() => setQuery((q) => ({ ...q, limit: PAGE })), [debouncedText, gameVersion]);

  const set = <K extends keyof BrowseQuery>(k: K, v: BrowseQuery[K]) => setQuery((q) => ({ ...q, [k]: v, limit: PAGE }));
  const page = results.data;
  const pageIds = useMemo(() => page?.items.map((i) => i.modId) ?? [], [page]);
  const thumbs = useBrowseThumbnails(pageIds, !offline);
  const thumbFor = (e: BrowseEntry) => {
    const t = thumbs[e.modId.toLowerCase()];
    return t === undefined ? e.logoUrl : t;
  };
  const current = selected && page?.items.find((i) => i.modId === selected.modId);

  const toggleFavourite = (entry: BrowseEntry) =>
    prefs.mutate({ favourite: { modIds: [entry.modId], on: !entry.favourite } });

  const onCardMenu = (event: MouseEvent<HTMLButtonElement>, entry: BrowseEntry) => {
    const pageUrl = entry.urlAlias
      ? `https://mods.vintagestory.at/${encodeURIComponent(entry.urlAlias)}`
      : entry.assetId != null ? `https://mods.vintagestory.at/show/mod/${entry.assetId}` : null;
    openMenu(event, `Actions for ${entry.name}`, [[
      { label: entry.installed ? "View details" : "View versions and install options", run: () => setSelected(entry) },
      { label: entry.favourite ? "Remove from favourites" : "Add to favourites", disabled: prefs.isPending, run: () => toggleFavourite(entry) },
    ], [
      !!pageUrl && { label: "Open Mod DB page", run: () => openUrl.mutate(pageUrl) },
      { label: "Copy mod ID", run: () => copyText(entry.modId) },
    ]]);
  };

  if (!enabled) {
    return <p className="empty-state">Set the data folder on the Settings tab first, so mods have somewhere to go.</p>;
  }

  return (
    <div className="installed browse-page">
      <div className="installed-main">
        <div className="mods-toolbar wrap">
          <input
            type="search"
            className="search"
            placeholder="Search the Mod DB"
            aria-label="Search the Mod DB"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
          <select aria-label="Sort by" value={query.sort} onChange={(e) => set("sort", e.target.value as BrowseSort)}>
            {SORTS.map((s) => (
              <option key={s.value} value={s.value}>
                {s.label}
              </option>
            ))}
          </select>
          <select
            aria-label="Category"
            value={query.tags[0] ?? ""}
            onChange={(e) => set("tags", e.target.value ? [e.target.value] : [])}
          >
            <option value="">All categories</option>
            {(facets.data?.tags ?? []).map((t) => (
              <option key={t.name} value={t.name}>
                {t.name} ({t.count})
              </option>
            ))}
          </select>
          <select aria-label="Game version" value={gameVersion} onChange={(e) => setGameVersion(e.target.value)}>
            <option value="">Any game version</option>
            {(facets.data?.gameVersions ?? []).map((v) => (
              <option key={v} value={v}>
                For {v}
              </option>
            ))}
          </select>
          <select aria-label="Side" value={query.side ?? ""} onChange={(e) => set("side", e.target.value || null)}>
            <option value="">Client and server</option>
            <option value="both">Both sides</option>
            <option value="client">Client only</option>
            <option value="server">Server only</option>
          </select>
          <label className="checkbox small">
            <input type="checkbox" checked={query.onlyFavourites} onChange={(e) => set("onlyFavourites", e.target.checked)} />
            Favourites
          </label>
          <label className="checkbox small">
            <input type="checkbox" checked={query.hideInstalled} onChange={(e) => set("hideInstalled", e.target.checked)} />
            Hide installed
          </label>
          <div className="header-spacer" />
          {page && (
            <span className="muted small">
              {formatCount(page.total)} mods · list from {formatAgo(page.fetchedMs)}
            </span>
          )}
          <button
            type="button"
            disabled={refresh.isPending || offline}
            title={offline ? "Offline mode is on (Settings)" : "Download the Mod DB list again"}
            onClick={() => refresh.mutate()}
          >
            {refresh.isPending ? "Refreshing…" : "Refresh"}
          </button>
        </div>
        {(results.error ?? refresh.error) && (
          <p className="notice error-text">
            {offline && !page
              ? "Offline mode is on and the Mod DB list hasn't been downloaded yet."
              : (results.error ?? refresh.error)!.message}
          </p>
        )}
        {(prefs.error || openUrl.error) && <p className="notice error-text">{(prefs.error ?? openUrl.error)?.message}</p>}
        <div className="browse-scroll">
          {results.isPending && <p className="empty-state">Loading the Mod DB… (the first time takes a few seconds)</p>}
          {page && page.items.length === 0 && <p className="empty-state">No mods match.</p>}
          <div className="db-card-grid" role="list">
            {page?.items.map((e) => (
              <ModCard key={e.modId} entry={e} thumbUrl={thumbFor(e)} selected={selected?.modId === e.modId}
                onSelect={() => setSelected(e)} onContextMenu={(event) => onCardMenu(event, e)} />
            ))}
          </div>
          {page && page.items.length < page.total && (
            <div className="load-more">
              <button type="button" onClick={() => setQuery((q) => ({ ...q, limit: q.limit + PAGE }))}>
                Show more ({formatCount(page.total - page.items.length)} left)
              </button>
            </div>
          )}
        </div>
      </div>
      {selected && (
        <ModDetailsWindow entry={current ?? selected} offline={offline} onClose={() => setSelected(null)}
          thumbUrl={thumbFor(current ?? selected)}
          onFavourite={() => toggleFavourite(current ?? selected)} favouriteBusy={prefs.isPending} />
      )}
    </div>
  );
}

/** Remote logo with a letter placeholder when it's missing or fails to load (offline). */
export function Logo({ url, name, className }: { url: string | null; name: string; className: string }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [url]);
  if (!url || failed) {
    return (
      <span className={`${className} placeholder`} aria-hidden>
        {name.slice(0, 1).toUpperCase()}
      </span>
    );
  }
  return <img className={className} src={url} alt="" loading="lazy" onError={() => setFailed(true)} />;
}

/** Mod DB style card: the mod's picture on top, name, summary and counts below. */
function ModCard({ entry, thumbUrl, selected, onSelect, onContextMenu }: {
  entry: BrowseEntry;
  thumbUrl: string | null;
  selected: boolean;
  onSelect: () => void;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      type="button"
      role="listitem"
      className={`db-card${selected ? " selected" : ""}`}
      onClick={onSelect}
      onContextMenu={onContextMenu}
      aria-haspopup="menu"
      title={entry.summary ? `${entry.name}
${entry.summary}` : entry.name}
    >
      <Thumbnail url={thumbUrl} name={entry.name} />
      {(entry.installed || entry.favourite) && (
        <span className="db-card-badges">
          {entry.favourite && <span className="db-card-badge fav" aria-label="Favourite">★</span>}
          {entry.installed && <span className="db-card-badge">Installed</span>}
        </span>
      )}
      <span className="db-card-desc">
        <span className="db-card-stats">
          <span title={`${entry.downloads.toLocaleString()} downloads`}>
            <Icon name="download" />
            {formatCount(entry.downloads)}
          </span>
          <span title={`${entry.comments.toLocaleString()} comments`}>
            <Icon name="comment" />
            {formatCount(entry.comments)}
          </span>
        </span>
        <span className="db-card-title">{entry.name}</span>
        {entry.summary && <span className="db-card-summary">{entry.summary}</span>}
      </span>
    </button>
  );
}

/**
 * The card picture, fit to the card's width and hung from the top as on the Mod DB. Falls back
 * to the mod's initial.
 */
function Thumbnail({ url, name }: { url: string | null; name: string }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [url]);
  if (!url || failed) {
    return (
      <span className="db-card-thumb placeholder" aria-hidden>
        <span>{name.slice(0, 1).toUpperCase()}</span>
      </span>
    );
  }
  return (
    <span className="db-card-thumb">
      <img src={url} alt="" loading="lazy" onError={() => setFailed(true)} />
    </span>
  );
}
