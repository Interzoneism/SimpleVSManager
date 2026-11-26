import { useEffect, useState } from "react";
import type { VoteOption, VoteSummary } from "../../bindings";
import { useCloudStatus, useModReports, useSubmitModReport } from "../../api";
import { Icon } from "../../components/Icon";
import { useErrorToast } from "../../components/Toast";

export const REPORT_OPTIONS: { value: VoteOption; label: string; short: string }[] = [
  { value: "fullyFunctional", label: "Fully functional", short: "Works great" },
  { value: "noIssuesSoFar", label: "No issues so far", short: "No issues" },
  { value: "someIssuesButWorks", label: "Some issues, but works", short: "Some issues" },
  { value: "notFunctional", label: "Not functional", short: "Broken" },
  { value: "crashesOrFreezesGame", label: "Crashes or freezes the game", short: "Crashes" },
];

const needsComment = (o: VoteOption | null) => o === "notFunctional" || o === "crashesOrFreezesGame";
const labelOf = (o: VoteOption) => REPORT_OPTIONS.find((x) => x.value === o)?.label ?? o;

/** Face icons, from a big grin down to a crashed face. */
const mouths: Record<VoteOption, string> = {
  fullyFunctional: "M9.2 9.5h.01 M14.8 9.5h.01 M7.5 13.2h9a4.5 4.5 0 0 1-9 0Z",
  noIssuesSoFar: "M9.2 9.5h.01 M14.8 9.5h.01 M8.5 14.2a4.2 4.2 0 0 0 7 0",
  someIssuesButWorks: "M9.2 9.5h.01 M14.8 9.5h.01 M8.3 15.5c1.2-1 2.5-1 3.7 0s2.5 1 3.7 0",
  notFunctional: "M9.2 9.5h.01 M14.8 9.5h.01 M8.5 16.6a4.2 4.2 0 0 1 7 0",
  crashesOrFreezesGame: "M7.9 8.2l2.4 2.4 M10.3 8.2 7.9 10.6 M13.7 8.2l2.4 2.4 M16.1 8.2l-2.4 2.4 M9 16h6",
};

export function Face({ option, size = 28 }: { option: VoteOption; size?: number }) {
  return (
    <svg className="report-face" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="9.2" />
      <path d={mouths[option]} />
    </svg>
  );
}

const countOf = (r: VoteSummary, o: VoteOption) => r[o];

/**
 * The face that sums up a set of reports (counts best to worst, as `REPORT_OPTIONS`): the
 * average answer, rounded. `null` without reports.
 */
export function verdictOf(counts: number[]): VoteOption | null {
  const total = counts.reduce((a, b) => a + b, 0);
  if (total === 0) return null;
  const mean = counts.reduce((sum, c, i) => sum + c * i, 0) / total;
  return REPORT_OPTIONS[Math.min(REPORT_OPTIONS.length - 1, Math.round(mean))].value;
}

/** What other players report about this mod version, and your own report. */
export function Reports({ modId, version }: { modId: string; version: string }) {
  const status = useCloudStatus();
  const online = !!status.data && !status.data.offline;
  const reports = useModReports(modId, version, online);
  const submit = useSubmitModReport();
  useErrorToast(submit.error);
  // A problem report waiting for its comment before it is sent.
  const [draft, setDraft] = useState<VoteOption | null>(null);
  const [comment, setComment] = useState("");
  const [thanks, setThanks] = useState<VoteOption | "removed" | null>(null);
  const canReport = online && !status.data?.legacyIdentity && !status.data?.signInProblem;

  useEffect(() => {
    if (!thanks) return;
    const t = window.setTimeout(() => setThanks(null), 2600);
    return () => window.clearTimeout(t);
  }, [thanks]);

  // A different mod or version starts with a clean form.
  useEffect(() => { setDraft(null); setComment(""); setThanks(null); }, [modId, version]);

  if (!online) return null;
  const r = reports.data;
  const total = r ? REPORT_OPTIONS.reduce((n, o) => n + countOf(r, o.value), 0) : 0;
  const works = r ? r.fullyFunctional + r.noIssuesSoFar + r.someIssuesButWorks : 0;
  const worksPct = total > 0 ? Math.round((works / total) * 100) : 0;
  const verdict = worksPct >= 80 ? "good" : worksPct >= 50 ? "mixed" : "bad";
  const myVote = r?.myVote ?? null;
  const sending = submit.isPending ? submit.variables?.option ?? null : null;

  const send = (option: VoteOption | null, text: string | null) =>
    submit.mutate(
      { modId, version, option, comment: text },
      { onSuccess: () => { setDraft(null); setComment(""); setThanks(option ?? "removed"); } },
    );

  const pick = (o: VoteOption) => {
    if (submit.isPending) return;
    submit.reset();
    if (needsComment(o)) {
      setDraft(o);
      setComment(o === myVote ? r?.myComment ?? "" : draft && needsComment(draft) ? comment : "");
      return;
    }
    setDraft(null);
    if (o !== myVote) send(o, null);
  };

  return (
    <section className="details-section reports">
      <div className="details-section-head">
        <h3>Player reports</h3>
        <span className="muted small">for v{version}</span>
      </div>

      {reports.isPending && <p className="muted small">Loading…</p>}
      {reports.isError && <p className="error-text small">{reports.error.message}</p>}

      {r && total === 0 && (
        <p className="report-empty small">No reports yet.</p>
      )}

      {r && total > 0 && (
        <div className="report-summary">
          <div className="report-headline">
            <span className={`report-score report-score-${verdict}`}>{worksPct}%</span>
            <span className="small">
              of {total} {total === 1 ? "player says" : "players say"} it works
            </span>
          </div>
          <div className="report-stack" role="img" aria-label={`${works} of ${total} players say it works`}>
            {REPORT_OPTIONS.filter((o) => countOf(r, o.value) > 0).map((o) => (
              <span
                key={o.value}
                className={`report-seg tone-${o.value}`}
                style={{ flexGrow: countOf(r, o.value) }}
                title={`${o.label}: ${countOf(r, o.value)}`}
              />
            ))}
          </div>
          <ul className="report-legend small" aria-label={`${total} player reports`}>
            {REPORT_OPTIONS.filter((o) => countOf(r, o.value) > 0).map((o) => (
              <li key={o.value} className={`tone-${o.value}`}>
                <span className="report-dot" />
                <span>{o.label}</span>
                <strong>{countOf(r, o.value)}</strong>
              </li>
            ))}
          </ul>
        </div>
      )}

      {r && r.problemComments.length > 0 && (
        <details className="report-problems small">
          <summary>
            <span>What goes wrong</span>
            <span className="report-problems-count">{r.problemComments.length}</span>
          </summary>
          <ul>{r.problemComments.map((c) => <li key={c}>{c}</li>)}</ul>
        </details>
      )}

      {canReport && r && (
        <div className="report-vote">
          <div className="report-vote-head small">
            {thanks ? (
              <span className="report-thanks" key={thanks}>
                {thanks === "removed" ? "Your report was removed." : "Thanks! Your report is in."}
              </span>
            ) : myVote ? (
              <span>You said <strong>{labelOf(myVote)}</strong>.</span>
            ) : (
              <span>How does it work for you?</span>
            )}
            {myVote && (
              <button type="button" className="link-button small" disabled={submit.isPending} onClick={() => { setDraft(null); send(null, null); }}>
                <Icon name="trash" size={13} />Remove
              </button>
            )}
          </div>
          <div className="report-choices" role="group" aria-label="Your report">
            {REPORT_OPTIONS.map((o) => {
              const selected = (draft ?? myVote) === o.value;
              return (
                <button
                  key={o.value}
                  type="button"
                  className={[
                    "report-choice",
                    `tone-${o.value}`,
                    selected && "is-selected",
                    myVote === o.value && "is-mine",
                    sending === o.value && "is-sending",
                    thanks === o.value && "is-thanked",
                  ].filter(Boolean).join(" ")}
                  aria-pressed={selected}
                  title={o.label}
                  disabled={submit.isPending && sending !== o.value}
                  onClick={() => pick(o.value)}
                >
                  <Face option={o.value} />
                  <span className="report-choice-label">{o.short}</span>
                </button>
              );
            })}
          </div>
          {draft && (
            <div className="report-comment">
              <textarea
                rows={2}
                autoFocus
                placeholder={draft === "crashesOrFreezesGame" ? "What happens when it crashes or freezes? (required)" : "What doesn't work? (required)"}
                value={comment}
                onChange={(e) => setComment(e.target.value)}
              />
              <div className="report-comment-actions">
                <button type="button" className="link-button small" onClick={() => { setDraft(null); setComment(""); }}>
                  <Icon name="close" size={13} />Cancel
                </button>
                <button
                  type="button"
                  className="primary"
                  disabled={!comment.trim() || submit.isPending}
                  onClick={() => send(draft, comment)}
                >
                  <Icon name="send" />{draft === myVote ? "Update report" : "Send report"}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {!canReport && status.data?.legacyIdentity && <p className="muted small">Move your account to send reports.</p>}
    </section>
  );
}
