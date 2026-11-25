import { useState } from "react";
import type { UsageMod } from "../../bindings";
import { useAnswerPlaySessions, useCloudStatus, usePlaySessions, useSubmitModReport } from "../../api";
import { Modal } from "../modlists/Modal";

/** Banner + dialog after several long play sessions: "did these mods work?" (feeds player reports). */
export function ConfirmMods({ enabled, onOpenMod }: { enabled: boolean; onOpenMod: (modId: string) => void }) {
  const status = useCloudStatus(enabled);
  const canReport = !!status.data && !status.data.offline && !status.data.legacyIdentity && !status.data.signInProblem;
  const sessions = usePlaySessions(enabled && canReport);
  const answer = useAnswerPlaySessions();
  const submit = useSubmitModReport();
  const [open, setOpen] = useState(false);
  const [worked, setWorked] = useState<Set<string> | null>(null);
  const [sent, setSent] = useState(0);

  const data = sessions.data;
  if (!canReport || !data?.due || data.mods.length === 0) return null;
  const chosen = worked ?? new Set(data.mods.map((m) => m.key));

  const send = async () => {
    const picked = data.mods.filter((m) => chosen.has(m.key));
    let n = 0;
    for (const m of picked) {
      try {
        await submit.mutateAsync({ modId: m.modId, version: m.version, option: "fullyFunctional", comment: null });
        n++;
        setSent(n);
      } catch {
        // Keep going; the error shows below.
      }
    }
    answer.mutate({ keys: data.mods.map((m) => m.key), stop: false }, { onSuccess: () => setOpen(false) });
  };

  return (
    <>
      <p className="notice banner">
        You've played {data.longSessions} long sessions. Help other players: confirm which of your {data.mods.length} mods work.{" "}
        <button type="button" className="link-button" onClick={() => setOpen(true)}>Confirm working mods</button>
      </p>
      {open && (
        <Modal title="Confirm working mods" onClose={() => setOpen(false)} busy={submit.isPending}>
          <p className="muted small">
            Ticked mods are reported as "Fully functional" for your game version. Untick any that gave you trouble, and
            report those from the mod's details instead.
          </p>
          <ul className="check-list tall">
            {data.mods.map((m: UsageMod) => (
              <li key={m.key}>
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={chosen.has(m.key)}
                    onChange={(e) => {
                      const next = new Set(chosen);
                      if (e.target.checked) next.add(m.key);
                      else next.delete(m.key);
                      setWorked(next);
                    }}
                  />
                  <span>{m.name}</span>
                  <span className="muted small">{m.version} · VS {m.gameVersion}</span>
                </label>
                <button type="button" className="link-button small" onClick={() => { setOpen(false); onOpenMod(m.modId); }}>
                  Had problems
                </button>
              </li>
            ))}
          </ul>
          {submit.isPending && <p className="muted small">Sending {sent + 1} of {chosen.size}…</p>}
          {submit.isError && <p className="notice error-text">{submit.error.message}</p>}
          <div className="modal-actions">
            <button type="button" className="link-button small" disabled={answer.isPending} onClick={() => answer.mutate({ keys: [], stop: true }, { onSuccess: () => setOpen(false) })}>
              Don't ask again
            </button>
            <span className="header-spacer" />
            <button type="button" disabled={answer.isPending} onClick={() => answer.mutate({ keys: data.mods.map((m) => m.key), stop: false }, { onSuccess: () => setOpen(false) })}>
              Skip these
            </button>
            <button type="button" className="primary" disabled={chosen.size === 0 || submit.isPending} onClick={() => void send()}>
              Report {chosen.size} as working
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
