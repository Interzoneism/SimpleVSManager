import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { CloudSlotView, ModlistSummary } from "../../bindings";
import { useAcknowledgeNotices, useCloudSlots, useCloudStatus, usePublishSaved, useUnpublish } from "../../api";
import { EditPublishedDialog, type PublishedDetails } from "./EditPublishedDialog";
import { toast, useErrorToast } from "../../components/Toast";

/** The cloud holds at most this many modlists per player (slot1..slot5 in the database rules). */
export const PUBLISH_LIMIT = 5;
export const LIMIT_MESSAGE = `You can publish up to ${PUBLISH_LIMIT} modlists. Unpublish one first.`;

/** Only your own saved modlists can be published: not groups, backups or lists that follow someone else's. */
export const canBePublished = (entry: ModlistSummary) => entry.kind === "modlist" && !entry.subscription && !entry.error;

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/**
 * Publishing saved modlists. A published modlist is a saved modlist marked as published in one of
 * your five cloud slots; reading the slots also turns published lists without a saved modlist
 * into saved modlists (see `reconcile_published`).
 */
export function usePublishing({ offline, onFlash }: { offline: boolean; onFlash: (message: string) => void }) {
  const status = useCloudStatus(!offline);
  const ready = !offline && !!status.data && !status.data.offline && !status.data.legacyIdentity && !status.data.playerProblem && !status.data.signInProblem;
  const slots = useCloudSlots(ready && !!status.data?.hasIdentity);
  const publishMutation = usePublishSaved();
  const unpublishMutation = useUnpublish();
  const ack = useAcknowledgeNotices();
  const [editing, setEditing] = useState<PublishedDetails | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  /** Why publishing isn't possible right now, or `null`. */
  const blocked = offline || status.data?.offline
    ? "Turn off offline mode to publish."
    : !status.data
      ? "Checking the cloud…"
      : status.data.legacyIdentity
        ? "Move your account first (see Online modlists)."
        : status.data.playerProblem ?? status.data.signInProblem ?? null;
  const slotList = slots.data?.slots ?? [];
  const slotFor = (registryId: string | undefined): CloudSlotView | null =>
    (registryId && slotList.find((s) => s?.info.registryId === registryId)) || null;
  const full = !!slots.data && slotList.every(Boolean);
  const busy = publishMutation.isPending || unpublishMutation.isPending;

  const publish = (entry: ModlistSummary, onDone?: () => void) => {
    setProblem(null);
    if (blocked) return setProblem(blocked);
    if (!entry.published && full) return setProblem(LIMIT_MESSAGE);
    const before = entry.published?.revision ?? null;
    publishMutation.mutate(entry.id, {
      onSuccess: (s) => {
        const revision = s.published?.revision ?? 1;
        onFlash(
          before === null
            ? `Published "${s.name}".`
            : revision > before
              ? `Published "${s.name}" (revision ${revision}).`
              : `Published "${s.name}".`,
        );
        onDone?.();
      },
    });
  };

  /** Asks first (unless `confirmed`). Resolves to whether the list was unpublished. */
  const unpublish = async (entry: ModlistSummary, confirmed = false): Promise<boolean> => {
    setProblem(null);
    if (blocked) {
      setProblem(blocked);
      return false;
    }
    const followers = slotFor(entry.published?.registryId)?.subscribers ?? 0;
    const ok = confirmed || await ask(
      `Unpublish "${entry.name}"? Your saved copy stays.` +
        (followers ? ` Your ${plural(followers, "follower")} keep their copies but get no more updates.` : ""),
      { title: "Unpublish modlist", kind: "warning", okLabel: "Unpublish", cancelLabel: "Cancel" },
    );
    if (!ok) return false;
    try {
      await unpublishMutation.mutateAsync(entry.id);
      if (!confirmed) onFlash(`"${entry.name}" is no longer published.`);
      return true;
    } catch {
      return false;
    }
  };

  const editDetails = (entry: ModlistSummary) => {
    const slot = slotFor(entry.published?.registryId);
    if (!entry.published) return;
    if (blocked) return setProblem(blocked);
    setEditing({
      registryId: entry.published.registryId,
      name: entry.name,
      description: entry.description,
      gameVersion: entry.gameVersion,
      icon: entry.icon,
      iconColor: entry.iconColor,
      revision: slot?.info.revision ?? entry.published.revision,
      subscribers: slot?.subscribers ?? 0,
    });
  };

  const notices = slots.data?.newSubscribers ?? [];
  const noticeText = notices.map(([id, n]) => `${slotFor(id)?.info.name ?? "a modlist"} (+${n})`).join(", ");
  const noticeIds = notices.map(([id]) => id).join("\n");
  useEffect(() => {
    if (!noticeIds) return;
    // Seen once it goes away, whether it timed out or was closed.
    toast.info(noticeText, {
      id: "new-followers",
      title: "New followers",
      duration: 12000,
      onDismiss: () => ack.mutate(noticeIds.split("\n")),
    });
    // ack is a fresh object each render; the toast follows the notices themselves.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noticeIds, noticeText]);
  // Each problem shows once; the mutations are reset so the same problem can show again later.
  useEffect(() => {
    if (!problem) return;
    toast.error(problem, { title: "Can't publish right now" });
    setProblem(null);
  }, [problem]);
  useErrorToast(publishMutation.error ?? unpublishMutation.error ?? ack.error);
  const modal = editing && <EditPublishedDialog list={editing} onDone={onFlash} onClose={() => setEditing(null)} />;

  return {
    /** Why publishing isn't possible now, or `null`. */
    blocked,
    busy,
    usedSlots: slotList.filter(Boolean).length,
    slotFor,
    publish,
    unpublish,
    editDetails,
    modal,
  };
}

export type Publishing = ReturnType<typeof usePublishing>;
