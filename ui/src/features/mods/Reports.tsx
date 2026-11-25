import { useState } from "react";
import type { VoteOption } from "../../bindings";
import { useCloudStatus, useModReports, useSubmitModReport } from "../../api";

export const REPORT_OPTIONS: { value: VoteOption; label: string }[] = [
  { value: "fullyFunctional", label: "Fully functional" },
  { value: "noIssuesSoFar", label: "No issues so far" },
  { value: "someIssuesButWorks", label: "Some issues, but works" },
  { value: "notFunctional", label: "Not functional" },
  { value: "crashesOrFreezesGame", label: "Crashes or freezes the game" },
];

const needsComment = (o: VoteOption | "") => o === "notFunctional" || o === "crashesOrFreezesGame";

/** What other players report about this mod version, and your own report. */
export function Reports({ modId, version }: { modId: string; version: string }) {
  const status = useCloudStatus();
  const online = !!status.data && !status.data.offline;
  const reports = useModReports(modId, version, online);
  const submit = useSubmitModReport();
  const [editing, setEditing] = useState(false);
  const [option, setOption] = useState<VoteOption | "">("");
  const [comment, setComment] = useState("");
  const canReport = online && !status.data?.legacyIdentity && !status.data?.signInProblem;

  if (!online) return null;
  const r = reports.data;
  const total = r ? r.fullyFunctional + r.noIssuesSoFar + r.someIssuesButWorks + r.notFunctional + r.crashesOrFreezesGame : 0;
  const count = (o: VoteOption) =>
    r ? { fullyFunctional: r.fullyFunctional, noIssuesSoFar: r.noIssuesSoFar, someIssuesButWorks: r.someIssuesButWorks, notFunctional: r.notFunctional, crashesOrFreezesGame: r.crashesOrFreezesGame }[o] : 0;

  return (
    <section className="details-section">
      <div className="details-section-head">
        <h3>Player reports</h3>
        <span className="muted small">for v{version}</span>
      </div>
      {reports.isPending && <p className="muted small">Loading…</p>}
      {reports.isError && <p className="error-text small">{reports.error.message}</p>}
      {r && total === 0 && <p className="muted small">No one has reported on this version yet.</p>}
      {r && total > 0 && (
        <ul className="report-bars small" aria-label={`${total} player reports`}>
          {REPORT_OPTIONS.filter((o) => count(o.value) > 0).map((o) => (
            <li key={o.value} className={`report-${o.value}`}>
              <span className="report-label">{o.label}</span>
              <span className="report-bar"><span style={{ width: `${(count(o.value) / total) * 100}%` }} /></span>
              <span className="report-count">{count(o.value)} {count(o.value) === 1 ? "player" : "players"}</span>
            </li>
          ))}
        </ul>
      )}
      {r && r.problemComments.length > 0 && (
        <details className="small">
          <summary>What goes wrong ({r.problemComments.length})</summary>
          <ul className="details-list">{r.problemComments.map((c) => <li key={c}>{c}</li>)}</ul>
        </details>
      )}
      {r?.myVote && !editing && (
        <p className="small">
          You reported: {REPORT_OPTIONS.find((o) => o.value === r.myVote)?.label}{" "}
          <button type="button" className="link-button small" onClick={() => { setOption(r.myVote!); setComment(r.myComment ?? ""); setEditing(true); }}>
            Change
          </button>{" "}
          <button type="button" className="link-button small" disabled={submit.isPending} onClick={() => submit.mutate({ modId, version, option: null, comment: null })}>
            Remove
          </button>
        </p>
      )}
      {canReport && (!r?.myVote || editing) && (
        <div className="report-form">
          <div className="version-picker-row">
            <select aria-label="Your report" value={option} onChange={(e) => setOption(e.target.value as VoteOption)}>
              <option value="">How does it work for you?</option>
              {REPORT_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
            <button
              type="button"
              disabled={!option || submit.isPending || (needsComment(option) && !comment.trim())}
              onClick={() =>
                option && submit.mutate({ modId, version, option, comment: comment || null }, { onSuccess: () => setEditing(false) })
              }
            >
              Send
            </button>
          </div>
          {needsComment(option) && (
            <textarea rows={2} placeholder="What goes wrong? (required)" value={comment} onChange={(e) => setComment(e.target.value)} />
          )}
          {editing && <button type="button" className="link-button small" onClick={() => setEditing(false)}>Cancel</button>}
        </div>
      )}
      {!canReport && status.data?.legacyIdentity && <p className="muted small">Sending reports needs the move to the new cloud project.</p>}
      {submit.isError && <p className="error-text small">{submit.error.message}</p>}
    </section>
  );
}
