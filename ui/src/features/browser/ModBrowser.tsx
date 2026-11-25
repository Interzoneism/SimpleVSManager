import { useEffect, useMemo, useRef, useState, type MouseEvent, type PointerEvent as ReactPointerEvent } from "react";
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
import { SplitWorkspace } from "../../components/SplitWorkspace";
import { pointIn, usePointerDrag } from "../../components/pointerDrag";
import { recall, remember, useRememberedScroll } from "../../lib/pageMemory";
import { builder, entryFromBrowse, useBuilder, type BuilderEntry } from "./builderStore";
import { ModlistBuilder } from "./ModlistBuilder";

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

/** A remembered query is only used if it still looks like one. */
function isBrowseQuery(v: unknown): v is BrowseQuery {
  const q = v as BrowseQuery;
  return !!q && typeof q === "object" && typeof q.sort === "string" && SORTS.some((s) => s.value === q.sort)
    && Array.isArray(q.tags) && typeof q.limit === "number" && q.limit > 0 && typeof q.offset === "number"
    && typeof q.onlyFavourites === "boolean" && typeof q.hideInstalled === "boolean" && typeof q.reverse === "boolean";
}

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
  // Search, filters and how many cards are shown stay put when you leave the page and come back.
  const [text, setText] = useState(() => recall("browse-text", ""));
  const [query, setQuery] = useState<BrowseQuery>(() => recall("browse-query", defaultBrowseQuery(), isBrowseQuery));
  const [gameVersion, setGameVersion] = useState<string>(() => recall("browse-game-version", ""));
  useEffect(() => remember("browse-text", text), [text]);
  useEffect(() => remember("browse-query", query), [query]);
  useEffect(() => remember("browse-game-version", gameVersion), [gameVersion]);
  const [selected, setSelected] = useState<BrowseEntry | null>(null);
  const debouncedText = useDebounced(text, 200);

  const effective = useMemo(() => ({ ...query, text: debouncedText }), [query, debouncedText]);
  const results = useBrowseMods(effective, gameVersion || null, enabled);
  const facets = useBrowseFacets(enabled);
  const refresh = useRefreshCatalogue();
  const prefs = useUpdatePreferences();
  const openUrl = useOpenUrl();
  const openMenu = useContextMenu();

  // New search: back to the first page. (Not when the page comes back with its old search.)
  const lastSearch = useRef({ debouncedText, gameVersion });
  useEffect(() => {
    if (lastSearch.current.debouncedText === debouncedText && lastSearch.current.gameVersion === gameVersion) return;
    lastSearch.current = { debouncedText, gameVersion };
    setQuery((q) => ({ ...q, limit: PAGE }));
  }, [debouncedText, gameVersion]);

  const set = <K extends keyof BrowseQuery>(k: K, v: BrowseQuery[K]) => setQuery((q) => ({ ...q, [k]: v, limit: PAGE }));
  const page = results.data;
  const pageIds = useMemo(() => page?.items.map((i) => i.modId) ?? [], [page]);
  const thumbs = useBrowseThumbnails(pageIds, !offline);
  const thumbFor = (e: BrowseEntry) => {
    const t = thumbs[e.modId.toLowerCase()];
    return t === undefined ? e.logoUrl : t;
  };
  const current = selected && page?.items.find((i) => i.modId === selected.modId);

  const scrollRef = useRef<HTMLDivElement>(null);
  useRememberedScroll("browse-scroll", scrollRef, !!page);

  // The modlist builder: cards are dragged into it.
  const b = useBuilder();
  const inList = useMemo(() => new Set(b.entries.map((e) => e.modId.toLowerCase())), [b.entries]);
  const builderPane = useRef<HTMLElement | null>(null);
  const [overBuilder, setOverBuilder] = useState(false);
  const cardDrag = usePointerDrag<BuilderEntry>({
    onMove: (p) => setOverBuilder(pointIn(builderPane.current, p)),
    onDrop: (p, entry) => {
      if (pointIn(builderPane.current, p)) builder.add([entry]);
    },
    onEnd: () => setOverBuilder(false),
  });
  const startCardDrag = (event: ReactPointerEvent, entry: BrowseEntry) => {
    if (!b.open) return;
    const thumb = thumbFor(entry);
    cardDrag.begin(event, entryFromBrowse(entry, thumb), (
      <>
        <Logo url={thumb} name={entry.name} className="drag-ghost-icon" />
        <span>{entry.name}</span>
      </>
    ));
  };
  const addToBuilder = (entry: BrowseEntry) => {
    builder.setOpen(true);
    builder.add([entryFromBrowse(entry, thumbFor(entry))]);
  };

  const toggleFavourite = (entry: BrowseEntry) =>
    prefs.mutate({ favourite: { modIds: [entry.modId], on: !entry.favourite } });

  const onCardMenu = (event: MouseEvent<HTMLButtonElement>, entry: BrowseEntry) => {
    const pageUrl = entry.urlAlias
      ? `https://mods.vintagestory.at/${encodeURIComponent(entry.urlAlias)}`
      : entry.assetId != null ? `https://mods.vintagestory.at/show/mod/${entry.assetId}` : null;
    const listed = inList.has(entry.modId.toLowerCase());
    openMenu(event, `Actions for ${entry.name}`, [[
      { label: entry.installed ? "View details" : "View versions and install options", run: () => setSelected(entry) },
      { label: entry.favourite ? "Remove from favourites" : "Add to favourites", disabled: prefs.isPending, run: () => toggleFavourite(entry) },
    ], [
      listed
        ? { label: "Remove from the modlist builder", run: () => builder.remove([entry.modId]) }
        : { label: "Add to the modlist builder", run: () => addToBuilder(entry) },
    ], [
      !!pageUrl && { label: "Open Mod DB page", run: () => openUrl.mutate(pageUrl) },
      { label: "Copy mod ID", run: () => copyText(entry.modId) },
    ]]);
  };

  if (!enabled) {
    return <p className="empty-state">Set the data folder on the Settings tab first, so mods have somewhere to go.</p>;
  }

  return (
    <SplitWorkspace name="browse-builder" widePane className={`browse-page${b.open ? "" : " builder-closed"}`}>
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
            className={`builder-toggle${b.open ? " active" : ""}`}
            aria-pressed={b.open}
            title={b.open ? "Hide the modlist builder (your list is kept)" : "Build a modlist by dragging mods into it"}
            onClick={() => builder.setOpen(!b.open)}
          >
            <Icon name="lists" />
            Modlist builder{b.entries.length > 0 && <span className="count-badge">{b.entries.length}</span>}
          </button>
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
        <div className="browse-scroll" ref={scrollRef}>
          {results.isPending && <p className="empty-state">Loading the Mod DB… (the first time takes a few seconds)</p>}
          {page && page.items.length === 0 && <p className="empty-state">No mods match.</p>}
          <div className="db-card-grid" role="list">
            {page?.items.map((e) => (
              <ModCard key={e.modId} entry={e} thumbUrl={thumbFor(e)} selected={selected?.modId === e.modId}
                inList={inList.has(e.modId.toLowerCase())} draggable={b.open}
                onSelect={() => {
                  if (!cardDrag.endedDrag()) setSelected(e);
                }}
                onPointerDown={(event) => startCardDrag(event, e)}
                onContextMenu={(event) => onCardMenu(event, e)} />
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
      {b.open ? <ModlistBuilder paneRef={builderPane} dropActive={overBuilder} onClose={() => builder.setOpen(false)} /> : null}
      {selected && (
        <ModDetailsWindow entry={current ?? selected} offline={offline} onClose={() => setSelected(null)}
          thumbUrl={thumbFor(current ?? selected)}
          onFavourite={() => toggleFavourite(current ?? selected)} favouriteBusy={prefs.isPending} />
      )}
      {cardDrag.overlay}
    </SplitWorkspace>
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
  return <img className={className} src={url} alt="" loading="lazy" draggable={false} onError={() => setFailed(true)} />;
}

/** Mod DB style card: the mod's picture on top, name, summary and counts below. */
function ModCard({ entry, thumbUrl, selected, inList, draggable, onSelect, onPointerDown, onContextMenu }: {
  entry: BrowseEntry;
  thumbUrl: string | null;
  selected: boolean;
  inList: boolean;
  draggable: boolean;
  onSelect: () => void;
  onPointerDown: (event: ReactPointerEvent<HTMLButtonElement>) => void;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      type="button"
      role="listitem"
      className={`db-card${selected ? " selected" : ""}${draggable ? " can-drag" : ""}`}
      onClick={onSelect}
      onPointerDown={onPointerDown}
      onDragStart={(e) => e.preventDefault()}
      onContextMenu={onContextMenu}
      aria-haspopup="menu"
      title={entry.summary ? `${entry.name}
${entry.summary}` : entry.name}
    >
      <Thumbnail url={thumbUrl} name={entry.name} />
      {(entry.installed || entry.favourite || inList) && (
        <span className="db-card-badges">
          {entry.favourite && <span className="db-card-badge fav" aria-label="Favourite">★</span>}
          {entry.installed && <span className="db-card-badge">Installed</span>}
          {inList && <span className="db-card-badge in-list">In list</span>}
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
      <img src={url} alt="" loading="lazy" draggable={false} onError={() => setFailed(true)} />
    </span>
  );
}
