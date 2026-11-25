import { useMemo, useState, type MouseEvent } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { OnlineEntry } from "../../bindings";
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
import { ModCards } from "../modlists/ModCards";
import { ModlistCard } from "../modlists/ModlistCard";
import { PublishNewDialog } from "./PublishNewDialog";
import { filterOnline, splitOwn, type OnlineSort } from "./onlineView";
import { EditPublishedDialog } from "./EditPublishedDialog";
import { iconThemeClass, ModlistIcon } from "../modlists/ModlistIcon";
import { ModlistSection } from "../modlists/ModlistSection";
import { useContextMenu } from "../../components/ContextMenu";

export type OnlineModlistDialog = { kind: "publish"; slot: string } | { kind: "editPublished"; entry: OnlineEntry };

interface Options {
  offline: boolean;
  search: string;
  selected: string | null;
  onSelect: (id: string) => void;
  onDeselect: (id: string) => void;
  onLoad: (id: string, title: string) => void;
  onShowLocal: (id: string) => void;
  dialog: OnlineModlistDialog | null;
  onDialogChange: (dialog: OnlineModlistDialog | null) => void;
}

export function useOnlineModlistSections({
  offline, search, selected, onSelect, onDeselect, onLoad, onShowLocal, dialog, onDialogChange,
}: Options) {
  const status = useCloudStatus(!offline);
  const listing = useOnlineModlists(!offline);
  const hide = useHideOnline();
  const copy = useSaveOnlineCopy();
  const canPublish = !offline && !!status.data && !status.data.offline && !status.data.legacyIdentity && !status.data.playerProblem && !status.data.signInProblem;
  const slots = useCloudSlots(canPublish && !!status.data?.hasIdentity);
  const ack = useAcknowledgeNotices();
  const move = useMoveCloudIdentity();
  const openMenu = useContextMenu();
  const [sort, setSort] = useState<OnlineSort>("newest");
  const updateMods = useUpdatePublished();
  const delSlot = useDeleteCloudSlot();
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
    if (ok) delSlot.mutate(slot, {
      onSuccess: () => {
        onDeselect(entry.registryId);
        setFlash(`Deleted "${entry.name}" from the cloud.`);
      },
    });
  };

  const startLoad = ({ entry }: OnlineEntry) =>
    onLoad(entry.registryId, entry.name ?? "Online modlist");

  const saveCopy = ({ entry }: OnlineEntry, follow: boolean) =>
    copy.mutate(
      { registryId: entry.registryId, follow },
      { onSuccess: (s) => setFlash(follow ? `Following "${s.name}". Find it under Saved modlists.` : `Saved "${s.name}" under Saved modlists.`) },
    );

  const hideList = ({ entry }: OnlineEntry) =>
    hide.mutate({ registryId: entry.registryId }, { onSuccess: () => onDeselect(entry.registryId) });

  const onCardMenu = (event: MouseEvent<HTMLButtonElement>, list: OnlineEntry) => {
    const { entry, localId, mine } = list;
    onSelect(entry.registryId);
    openMenu(event, `Actions for ${entry.name ?? "this modlist"}`, [[
      { label: "Load…", disabled: busy, run: () => startLoad(list) },
      !!localId && { label: "Show my copy", run: () => onShowLocal(localId) },
      !localId && { label: "Follow", disabled: busy || mine, run: () => saveCopy(list, true) },
      !localId && { label: "Save a copy", disabled: busy, run: () => saveCopy(list, false) },
    ], [
      mine && { label: "Update with my enabled mods…", disabled: busy || !canPublish, run: () => void doUpdate(list) },
      mine && { label: "Edit details…", disabled: busy || !canPublish, run: () => onDialogChange({ kind: "editPublished", entry: list }) },
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
  const publishSlot = canPublish && firstFree >= 0 && !noMatches ? `slot${firstFree + 1}` : null;
  const sections = (
    <>
      <div className="mods-toolbar wrap">
        <select aria-label="Sort online modlists" value={sort} onChange={(e) => setSort(e.target.value as OnlineSort)}>
          <option value="newest">Recently updated</option>
          <option value="subscribers">Most followed</option>
          <option value="mods">Most mods</option>
          <option value="name">Name</option>
        </select>
        <span className="muted small">{listing.isSuccess && `${visible.length} of ${all.length} online`}</span>
        {hiddenCount > 0 && (
          <button type="button" className="link-button small" onClick={() => hide.mutate({ clear: true })}>
            Show {hiddenCount} hidden
          </button>
        )}
        <div className="header-spacer" />
        <button type="button" disabled={listing.isFetching} onClick={() => void listing.refetch()}>Refresh online modlists</button>
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
      {!listing.isPending && (
        <>
          <ModlistSection
            title="Your published modlists"
            count={own.mine.length}
            empty={own.mine.length || publishSlot ? undefined : noMatches ?? "No published modlists to show."}
          >
            {own.mine.map(card)}
            {publishSlot && (
              <li className="modlist-card-item">
                <button type="button" className="mod-card modlist-card publish-card" onClick={() => onDialogChange({ kind: "publish", slot: publishSlot })}>
                  <span className="publish-card-plus" aria-hidden="true">+</span>
                  <span className="publish-card-text">
                    <strong>Publish a modlist</strong>
                    <span className="muted small">{freeSlots} of 5 slots free</span>
                  </span>
                </button>
              </li>
            )}
          </ModlistSection>
          <ModlistSection
            title="From the community"
            count={own.others.length}
            empty={own.others.length ? undefined : noMatches ?? "No community modlists to show."}
          >
            {own.others.map(card)}
          </ModlistSection>
        </>
      )}
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
      onEdit={canPublish ? () => onDialogChange({ kind: "editPublished", entry: current }) : undefined}
      onUpdate={canPublish ? () => void doUpdate(current) : undefined}
      onDelete={canPublish && slotOf(current.entry.registryId) ? () => void doDelete(current) : undefined}
      onHideAuthor={() => current.entry.uploader && hide.mutate(
        { author: current.entry.uploader },
        { onSuccess: () => onDeselect(current.entry.registryId) },
      )}
    />
  );
  const closeDialog = () => onDialogChange(null);
  const modal = dialog?.kind === "editPublished" ? (
    <EditPublishedDialog list={dialog.entry} onDone={setFlash} onClose={closeDialog} />
  ) : dialog?.kind === "publish" ? (
    <PublishNewDialog slot={dialog.slot} freeSlots={freeSlots} onClose={closeDialog} onDone={setFlash} />
  ) : null;

  return { sections, details, modal };
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
