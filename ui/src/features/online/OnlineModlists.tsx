import { useMemo, useState, type MouseEvent } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { ModlistSource, OnlineEntry, Os } from "../../bindings";
import {
  useAcknowledgeNotices,
  useCloudSlots,
  useCloudStatus,
  useHideOnline,
  useMoveCloudIdentity,
  useOnlineModlists,
  useDeleteCloudSlot,
  useSaveOnlineCopy,
  useUpdatePublished,
} from "../../api";
import { formatAgo } from "../../lib/format";
import { LoadModlistDialog } from "../modlists/LoadModlistDialog";
import { ModCards } from "../modlists/ModCards";
import { ModlistCard } from "../modlists/ModlistCard";
import { PublishNewDialog } from "./PublishNewDialog";
import { filterOnline, splitOwn, type OnlineSort } from "./onlineView";
import { EditPublishedDialog } from "./EditPublishedDialog";
import { iconThemeClass, ModlistIcon } from "../modlists/ModlistIcon";
import { CloseDetails, SplitWorkspace } from "../../components/SplitWorkspace";
import { useContextMenu } from "../../components/ContextMenu";

interface Props {
  os: Os;
  enabled: boolean;
  /** Show a local modlist on the Modlists tab. */
  onShowLocal: (id: string) => void;
}

/** The Online tab: modlists other players published, and publishing your own. */
export function OnlineModlists({ os, enabled, onShowLocal }: Props) {
  const status = useCloudStatus(enabled);
  const offline = !!status.data?.offline;
  const listing = useOnlineModlists(enabled && !offline);
  const hide = useHideOnline();
  const copy = useSaveOnlineCopy();
  const canPublish = !!status.data && !status.data.offline && !status.data.legacyIdentity && !status.data.playerProblem && !status.data.signInProblem;
  const slots = useCloudSlots(enabled && canPublish && !!status.data?.hasIdentity);
  const ack = useAcknowledgeNotices();
  const move = useMoveCloudIdentity();
  const openMenu = useContextMenu();
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<OnlineSort>("newest");
  const [selected, setSelected] = useState<string | null>(null);
  const [load, setLoad] = useState<{ source: ModlistSource; title: string } | null>(null);
  const [publishing, setPublishing] = useState<string | null>(null);
  const updateMods = useUpdatePublished();
  const delSlot = useDeleteCloudSlot();
  const [editing, setEditing] = useState<OnlineEntry | null>(null);
  const [flash, setFlash] = useState<string | null>(null);

  const all = useMemo(() => listing.data?.entries ?? [], [listing.data]);
  const visible = useMemo(() => filterOnline(all, search, sort), [all, search, sort]);
  const own = useMemo(() => splitOwn(visible), [visible]);
  const current = all.find((e) => e.entry.registryId === selected) ?? null;
  const hiddenCount = (listing.data?.hiddenModlists ?? 0) + (listing.data?.hiddenAuthors ?? 0);
  const notices = slots.data?.newSubscribers ?? [];
  const error = hide.error ?? copy.error ?? ack.error ?? move.error ?? updateMods.error ?? delSlot.error;
  const busy = copy.isPending || hide.isPending || updateMods.isPending || delSlot.isPending;
  const slotList = slots.data?.slots ?? [];
  const freeSlots = slotList.filter((s) => !s).length;
  const firstFree = slotList.findIndex((s) => !s);
  const slotOf = (registryId: string) => {
    const i = slotList.findIndex((s) => s?.info.registryId === registryId);
    return i >= 0 ? `slot${i + 1}` : null;
  };

  const doUpdate = async ({ entry, subscribers }: OnlineEntry) => {
    const ok = await ask(
      `Replace the mods in "${entry.name ?? "this modlist"}" with the mods you have enabled now? The name, description and icon stay.` +
        (subscribers ? ` If the mods changed, your ${subscribers} follower${subscribers === 1 ? "" : "s"} will see an update.` : ""),
      { title: "Update online modlist", okLabel: "Update", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    updateMods.mutate(entry.registryId, {
      onSuccess: (s) =>
        setFlash(s.revision > entry.revision ? `Updated "${entry.name}" to revision ${s.revision}.` : `"${entry.name}" already has these mods; nothing changed.`),
    });
  };

  const doDelete = async ({ entry, subscribers }: OnlineEntry) => {
    const slot = slotOf(entry.registryId);
    if (!slot) return;
    const ok = await ask(
      `Delete "${entry.name ?? "this modlist"}" from the cloud?${subscribers ? ` ${subscribers} follower${subscribers === 1 ? "" : "s"} keep their copies, but get no more updates.` : ""} This can't be undone.`,
      { title: "Delete online modlist", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
    );
    if (ok) delSlot.mutate(slot, { onSuccess: () => { setSelected(null); setFlash(`Deleted "${entry.name}" from the cloud.`); } });
  };

  const startLoad = ({ entry }: OnlineEntry) =>
    setLoad({ source: { id: null, file: null, online: entry.registryId }, title: entry.name ?? "Online modlist" });

  const saveCopy = ({ entry }: OnlineEntry, follow: boolean) =>
    copy.mutate(
      { registryId: entry.registryId, follow },
      { onSuccess: (s) => setFlash(follow ? `Following "${s.name}". It's on the Modlists tab.` : `Saved "${s.name}" to your modlists.`) },
    );

  const hideList = ({ entry }: OnlineEntry) =>
    hide.mutate({ registryId: entry.registryId }, { onSuccess: () => setSelected(null) });

  const onCardMenu = (event: MouseEvent<HTMLButtonElement>, list: OnlineEntry) => {
    const { entry, localId, mine } = list;
    setSelected(entry.registryId);
    openMenu(event, `Actions for ${entry.name ?? "this modlist"}`, [[
      { label: "Load…", disabled: busy, run: () => startLoad(list) },
      !!localId && { label: "Show my copy", run: () => onShowLocal(localId) },
      !localId && { label: "Follow", disabled: busy || mine, run: () => saveCopy(list, true) },
      !localId && { label: "Save a copy", disabled: busy, run: () => saveCopy(list, false) },
    ], [
      mine && { label: "Update with my enabled mods…", disabled: busy || !canPublish, run: () => void doUpdate(list) },
      mine && { label: "Edit details…", disabled: busy || !canPublish, run: () => setEditing(list) },
      mine && { label: "Delete from the cloud…", disabled: busy || !canPublish || !slotOf(entry.registryId), danger: true, run: () => void doDelete(list) },
      !mine && { label: "Hide modlist", disabled: busy, run: () => hideList(list) },
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
        setFlash(
          r.moved.length
            ? `Moved. Your modlists ${r.moved.map((n) => `"${n}"`).join(", ")} are published on the current project.`
            : "Moved. You had no published modlists on the old project.",
        ),
    });
  };

  const card = (list: OnlineEntry) => {
    const { entry, mine, subscribed, subscribers } = list;
    const rows = [
      <>By <strong>{entry.uploader ?? "unknown author"}</strong></>,
      <><strong>{entry.mods.length}</strong> mod{entry.mods.length === 1 ? "" : "s"}</>,
      entry.gameVersion ? `VS ${entry.gameVersion}` : "Any game version",
    ];
    const facts = [`Revision ${entry.revision}`, subscribers > 0 && `${subscribers} following`].filter((fact): fact is string => !!fact);
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
        badges={subscribed && !mine && <span className="badge active-badge">Following</span>}
        selected={selected === entry.registryId}
        onSelect={() => setSelected(entry.registryId)}
        onContextMenu={(event) => onCardMenu(event, list)}
      />
    );
  };

  if (!enabled) {
    return <p className="empty-state">The Vintage Story data folder wasn't found. Pick it on the <strong>Settings</strong> tab.</p>;
  }
  if (offline) return <p className="empty-state">Turn off offline mode in Settings to see community modlists.</p>;

  return (
    <SplitWorkspace name="community-modlists">
      <div className="installed-main">
        <div className="mods-toolbar wrap">
          <input
            type="search"
            className="search"
            placeholder="Search names, authors and mod ids"
            aria-label="Search online modlists"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <select aria-label="Sort" value={sort} onChange={(e) => setSort(e.target.value as OnlineSort)}>
            <option value="newest">Recently updated</option>
            <option value="subscribers">Most followed</option>
            <option value="mods">Most mods</option>
            <option value="name">Name</option>
          </select>
          <span className="muted small">{listing.isSuccess && `${visible.length} of ${all.length}`}</span>
          {hiddenCount > 0 && (
            <button type="button" className="link-button small" onClick={() => hide.mutate({ clear: true })}>
              Show {hiddenCount} hidden
            </button>
          )}
          <div className="header-spacer" />
          <button type="button" disabled={listing.isFetching} onClick={() => void listing.refetch()}>Refresh</button>
        </div>
        {status.data?.legacyIdentity && (
          <p className="notice warn-text">
            Your cloud identity is from the old cloud project. You can browse, follow and load modlists; publishing and
            reports need the one-time move to the new project.{" "}
            <button type="button" className="link-button" disabled={move.isPending} onClick={() => void doMove()}>
              {move.isPending ? "Moving…" : "Move my cloud identity…"}
            </button>
          </p>
        )}
        {status.data?.signInProblem && <p className="notice warn-text">{status.data.signInProblem}</p>}
        {notices.length > 0 && (
          <p className="notice">
            New followers:{" "}
            {notices.map(([id, n]) => `${slots.data?.slots.find((s) => s?.info.registryId === id)?.info.name ?? "a modlist"} (+${n})`).join(", ")}{" "}
            <button type="button" className="link-button small" onClick={() => ack.mutate(notices.map(([id]) => id))}>
              Dismiss
            </button>
          </p>
        )}
        {flash && <p className="notice" role="status">{flash}</p>}
        {error && <p className="notice error-text">{error.message}</p>}
        {listing.data?.error && <p className="notice warn-text">Showing the saved list: {listing.data.error}</p>}
        {listing.isPending && <p className="empty-state">Loading online modlists…</p>}
        {listing.isError && <p className="empty-state error-text">{listing.error.message}</p>}
        <div className="modlist-scroll">
          {listing.isSuccess && visible.length === 0 && (
            <p className="empty-state">{search.trim() ? "No modlists match your search." : "No community modlists to show."}</p>
          )}
          {(own.mine.length > 0 || (canPublish && firstFree >= 0)) && (
            <section className="modlist-section">
              <h3 className="online-section-title">Your published modlists <span className="muted small">({own.mine.length})</span></h3>
              <ul className="card-grid modlist-card-grid online-card-grid" aria-label="Your published modlists">
                {own.mine.map(card)}
                {canPublish && firstFree >= 0 && !search.trim() && (
                  <li className="modlist-card-item">
                    <button type="button" className="mod-card modlist-card publish-card" onClick={() => setPublishing(`slot${firstFree + 1}`)}>
                      <span className="publish-card-plus" aria-hidden="true">+</span>
                      <span className="publish-card-text">
                        <strong>Publish a modlist</strong>
                        <span className="muted small">{freeSlots} of 5 slots free</span>
                      </span>
                    </button>
                  </li>
                )}
              </ul>
            </section>
          )}
          {(own.mine.length === 0 || own.others.length > 0) && (
            <section className="modlist-section">
              {(own.mine.length > 0 || (canPublish && firstFree >= 0)) && (
                <h3 className="online-section-title">Community modlists <span className="muted small">({own.others.length})</span></h3>
              )}
              <ul className="card-grid modlist-card-grid online-card-grid" aria-label="Online modlists">{own.others.map(card)}</ul>
            </section>
          )}
        </div>
      </div>
      <aside className={`details${current ? "" : " empty"}`} aria-label="Community modlist details">
        {current && <CloseDetails onClose={() => setSelected(null)} />}
        {!current ? (
          <p className="muted">Select a modlist to see its mods. Load it straight away, or follow it to get its updates.</p>
        ) : (
          <OnlineDetails
            entry={current}
            busy={busy}
            onLoad={() => startLoad(current)}
            onCopy={(follow) => saveCopy(current, follow)}
            onShowLocal={onShowLocal}
            onHide={() => hideList(current)}
            onEdit={canPublish ? () => setEditing(current) : undefined}
            onUpdate={canPublish ? () => void doUpdate(current) : undefined}
            onDelete={canPublish && slotOf(current.entry.registryId) ? () => void doDelete(current) : undefined}
            onHideAuthor={() => current.entry.uploader && hide.mutate({ author: current.entry.uploader }, { onSuccess: () => setSelected(null) })}
          />
        )}
      </aside>
      {load && (
        <LoadModlistDialog os={os} source={load.source} kind={null} title={load.title} onShowBackup={onShowLocal} onClose={() => setLoad(null)} />
      )}
      {editing && <EditPublishedDialog list={editing} onDone={(msg) => setFlash(msg)} onClose={() => setEditing(null)} />}
      {publishing && <PublishNewDialog slot={publishing} freeSlots={freeSlots} onClose={() => setPublishing(null)} onDone={(msg) => setFlash(msg)} />}
    </SplitWorkspace>
  );
}

interface DetailsProps {
  entry: OnlineEntry;
  busy: boolean;
  onLoad: () => void;
  onCopy: (follow: boolean) => void;
  onShowLocal: (id: string) => void;
  onHide: () => void;
  onHideAuthor: () => void;
  /** Edit your own published modlist (when publishing is possible). */
  onEdit?: () => void;
  /** Replace the mods of your own published modlist with your enabled mods. */
  onUpdate?: () => void;
  onDelete?: () => void;
}

function OnlineDetails({ entry, busy, onLoad, onCopy, onShowLocal, onHide, onHideAuthor, onEdit, onUpdate, onDelete }: DetailsProps) {
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
        <button type="button" className="primary" disabled={busy} onClick={onLoad}>Load…</button>
        {entry.mine && onUpdate && (
          <button type="button" disabled={busy} title="Replace its mods with the mods you have enabled now" onClick={onUpdate}>Update</button>
        )}
        {entry.mine ? null : entry.localId ? (
          <button type="button" onClick={() => onShowLocal(entry.localId!)}>Show my copy</button>
        ) : (
          <>
            <button type="button" disabled={busy || entry.mine} title="Keep a copy that you can update when the author publishes changes" onClick={() => onCopy(true)}>
              Follow
            </button>
            <button type="button" disabled={busy} onClick={() => onCopy(false)}>Save a copy</button>
          </>
        )}
        {entry.mine && onEdit && <button type="button" disabled={busy} onClick={onEdit}>Edit details…</button>}
        {entry.mine && onDelete && <button type="button" className="danger" disabled={busy} onClick={onDelete}>Delete…</button>}
        {!entry.mine && (
          <>
            <button type="button" disabled={busy} onClick={onHide}>Hide</button>
            {e.uploader && <button type="button" disabled={busy} onClick={onHideAuthor}>Hide all from {e.uploader}</button>}
          </>
        )}
      </div>
      <ModCards mods={e.mods.map((m) => ({ modId: m.modId, version: m.version, isActive: null }))} />
    </>
  );
}
