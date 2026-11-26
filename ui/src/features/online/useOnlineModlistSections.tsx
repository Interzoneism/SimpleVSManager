import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { OnlineEntry } from "../../bindings";
import { useCloudStatus, useHideOnline, useMoveCloudIdentity, useOnlineModlists, useSaveOnlineCopy } from "../../api";
import { formatAgo } from "../../lib/format";
import { ModCards } from "../modlists/ModCards";
import { ModlistCard } from "../modlists/ModlistCard";
import { filterOnline, type OnlineSort } from "./onlineView";
import { iconThemeClass, ModlistIcon } from "../modlists/ModlistIcon";
import { ModlistSection } from "../modlists/ModlistSection";
import { useContextMenu } from "../../components/ContextMenu";
import { toast, useErrorToast } from "../../components/Toast";
import { Icon } from "../../components/Icon";

interface Options {
  offline: boolean;
  search: string;
  selected: string | null;
  onSelect: (id: string) => void;
  onDeselect: (id: string) => void;
  onLoad: (id: string, title: string) => void;
  onShowLocal: (id: string) => void;
  /** The online listing is only fetched while the section is open. */
  communityCollapsed: boolean;
  onToggleCommunity: () => void;
}

export function useOnlineModlistSections({
  offline, search, selected, onSelect, onDeselect, onLoad, onShowLocal, communityCollapsed, onToggleCommunity,
}: Options) {
  const status = useCloudStatus(!offline);
  const listing = useOnlineModlists(!offline && !communityCollapsed);
  const hide = useHideOnline();
  const copy = useSaveOnlineCopy();
  const move = useMoveCloudIdentity();
  const openMenu = useContextMenu();
  const [sort, setSort] = useState<OnlineSort>("newest");

  // Your own published modlists are under Saved modlists, so they aren't listed here.
  const all = useMemo(() => (listing.data?.entries ?? []).filter((e) => !e.mine), [listing.data]);
  const visible = useMemo(() => filterOnline(all, search, sort), [all, search, sort]);
  const current = all.find((e) => e.entry.registryId === selected) ?? null;
  const hiddenCount = (listing.data?.hiddenModlists ?? 0) + (listing.data?.hiddenAuthors ?? 0);
  useErrorToast(hide.error ?? copy.error ?? move.error);
  const staleReason = listing.data?.error ?? null;
  useEffect(() => {
    if (staleReason) toast.warning(`Showing the saved list: ${staleReason}`, { id: "online-modlists-stale", title: "Couldn't refresh online modlists" });
  }, [staleReason]);
  const busy = copy.isPending || hide.isPending;

  const startLoad = ({ entry }: OnlineEntry) =>
    onLoad(entry.registryId, entry.name ?? "Online modlist");

  const saveCopy = ({ entry }: OnlineEntry, follow: boolean) =>
    copy.mutate(
      { registryId: entry.registryId, follow },
      { onSuccess: (s) => toast.success(follow ? `Following "${s.name}". Find it under Saved modlists.` : `Saved "${s.name}" under Saved modlists.`) },
    );

  const hideList = ({ entry }: OnlineEntry) =>
    hide.mutate({ registryId: entry.registryId }, { onSuccess: () => onDeselect(entry.registryId) });

  const onCardMenu = (event: MouseEvent<HTMLButtonElement>, list: OnlineEntry) => {
    const { entry, localId } = list;
    onSelect(entry.registryId);
    openMenu(event, `Actions for ${entry.name ?? "this modlist"}`, [[
      { label: "Load…", disabled: busy, run: () => startLoad(list) },
      !!localId && { label: "Show my copy", run: () => onShowLocal(localId) },
      !localId && { label: "Follow", disabled: busy, run: () => saveCopy(list, true) },
      !localId && { label: "Save a copy", disabled: busy, run: () => saveCopy(list, false) },
    ], [
      { label: "Hide modlist", disabled: busy, run: () => hideList(list) },
    ]]);
  };

  const doMove = async () => {
    const ok = await ask(
      "This moves your cloud identity and your published modlists from the old cloud project to the current one, like Simple VS Manager 2.3 does.\n\n" +
        "Your current identity file is kept as firebase-auth.legacy.json (and the SVSM Backup copy likewise). If anything fails, everything is put back.",
      { title: "Move cloud identity", okLabel: "Move", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    move.mutate(undefined, {
      onSuccess: (r) =>
        toast.success(
          r.moved.length
            ? `Moved. Your modlists ${r.moved.map((n) => `"${n}"`).join(", ")} are published on the current project.`
            : "Moved. You had no published modlists on the old project.",
        ),
    });
  };

  const card = (list: OnlineEntry) => {
    const { entry, subscribed, subscribers } = list;
    const rows = [
      <>By <strong>{entry.uploader ?? "unknown author"}</strong></>,
      <><strong>{entry.mods.length}</strong> mod{entry.mods.length === 1 ? "" : "s"}</>,
      entry.gameVersion ? `VS ${entry.gameVersion}` : "Any game version",
    ];
    const facts = [`Revision ${entry.revision}`];
    return (
      <ModlistCard
        key={entry.registryId}
        title={entry.name ?? "Unnamed modlist"}
        icon={entry.icon}
        iconColor={entry.iconColor}
        kindIcon="community"
        rows={rows}
        facts={facts}
        date={entry.updatedMs ? formatAgo(entry.updatedMs) : undefined}
        followers={subscribers}
        following={subscribed}
        selected={selected === entry.registryId}
        onSelect={() => onSelect(entry.registryId)}
        onLoad={busy ? undefined : () => startLoad(list)}
        onContextMenu={(event) => onCardMenu(event, list)}
      />
    );
  };

  if (offline) {
    return { sections: <p className="muted small section-empty">Online modlists are unavailable in offline mode.</p> };
  }

  const noMatches = search.trim() ? "No matches." : null;
  const communityToolbar = (
    <div className="mods-toolbar wrap section-toolbar">
        <select aria-label="Sort online modlists" value={sort} onChange={(e) => setSort(e.target.value as OnlineSort)}>
          <option value="newest">Recently updated</option>
          <option value="subscribers">Most followed</option>
          <option value="mods">Most mods</option>
          <option value="name">Name</option>
        </select>
        <span className="muted small">{listing.isSuccess && `${visible.length} of ${all.length} online`}</span>
        {hiddenCount > 0 && (
          <button type="button" className="link-button small" onClick={() => hide.mutate({ clear: true })}>
            <Icon name="eye" size={13} />Show {hiddenCount} hidden
          </button>
        )}
        <div className="header-spacer" />
        <button type="button" disabled={listing.isFetching} onClick={() => void listing.refetch()}>
          <Icon name="refresh" />{listing.isFetching ? "Refreshing…" : "Refresh online modlists"}
        </button>
    </div>
  );
  const loaded = listing.isSuccess;
  const sections = (
    <>
      {status.data?.legacyIdentity && (
        <p className="notice warn-text">
          Your cloud identity is from the old cloud project. You can browse, follow and load modlists; publishing and
          reports need the one-time move to the new project.{" "}
          <button type="button" className="link-button" disabled={move.isPending} onClick={() => void doMove()}>
            <Icon name="goTo" size={13} />{move.isPending ? "Moving…" : "Move my cloud identity…"}
          </button>
        </p>
      )}
      {status.data?.signInProblem && <p className="notice warn-text">{status.data.signInProblem}</p>}
      {listing.isError && <p className="empty-state error-text">{listing.error.message}</p>}
          <ModlistSection
            title="From the community"
            count={loaded && !communityCollapsed ? visible.length : undefined}
            collapsed={communityCollapsed}
            onToggle={onToggleCommunity}
            toolbar={communityToolbar}
            empty={!loaded ? "Loading online modlists…" : visible.length ? undefined : noMatches ?? "No community modlists to show."}
          >
            {visible.map(card)}
          </ModlistSection>
    </>
  );

  const details = current && (
    <OnlineDetails
      entry={current}
      busy={busy}
      onLoad={() => startLoad(current)}
      onCopy={(follow) => saveCopy(current, follow)}
      onShowLocal={onShowLocal}
      onHide={() => hideList(current)}
      onHideAuthor={() => current.entry.uploader && hide.mutate(
        { author: current.entry.uploader },
        { onSuccess: () => onDeselect(current.entry.registryId) },
      )}
    />
  );
  return { sections, details };
}

interface DetailsProps {
  entry: OnlineEntry;
  busy: boolean;
  onLoad: () => void;
  onCopy: (follow: boolean) => void;
  onShowLocal: (id: string) => void;
  onHide: () => void;
  onHideAuthor: () => void;
}

function OnlineDetails({ entry, busy, onLoad, onCopy, onShowLocal, onHide, onHideAuthor }: DetailsProps) {
  const e = entry.entry;
  return (
    <>
      <div className="details-header">
        <h2>
          <span className={`details-icon-button${iconThemeClass(e.iconColor)}`} aria-hidden="true"><ModlistIcon icon={e.icon} fallback="community" /></span>
          {e.name ?? "Unnamed modlist"}
        </h2>
      </div>
      {e.description && <p className="description">{e.description}</p>}
      <dl className="details-facts">
        <dt>Made by</dt>
        <dd>{e.uploader ?? "Unknown"}</dd>
        {e.gameVersion && (<><dt>Game version</dt><dd>{e.gameVersion}</dd></>)}
        <dt>Revision</dt>
        <dd>{e.revision}</dd>
        {e.updatedMs && (<><dt>Updated</dt><dd>{new Date(e.updatedMs).toLocaleString()}</dd></>)}
        <dt>Following</dt>
        <dd>{entry.subscribers}</dd>
      </dl>
      <div className="details-actions">
        <button type="button" className="primary" disabled={busy} onClick={onLoad}><Icon name="load" />Load…</button>
        {entry.localId ? (
          <button type="button" onClick={() => onShowLocal(entry.localId!)}><Icon name="goTo" />Show my copy</button>
        ) : (
          <>
            <button type="button" disabled={busy} title="Keep a copy that you can update when the author publishes changes" onClick={() => onCopy(true)}>
              <Icon name="follow" />Follow
            </button>
            <button type="button" disabled={busy} onClick={() => onCopy(false)}><Icon name="copy" />Save a copy</button>
          </>
        )}
        <button type="button" disabled={busy} onClick={onHide}><Icon name="hide" />Hide</button>
        {e.uploader && <button type="button" disabled={busy} onClick={onHideAuthor}><Icon name="hide" />Hide all from {e.uploader}</button>}
      </div>
      <ModCards mods={e.mods.map((m) => ({ modId: m.modId, version: m.version, isActive: null }))} />
    </>
  );
}
