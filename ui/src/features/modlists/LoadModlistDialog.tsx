import { useCallback, useEffect, useRef, useState } from "react";
import type { LoadMode, ModlistKind, ModlistProgress, ModlistSource, Os, UpdateProgress } from "../../bindings";
import { cancelUpdates, useApplyModlist, useModlistPreview, useModlistProgress, useUpdateProgress } from "../../api";
import { defaultMode, describePlan, type QuickSwitchPreference } from "./modlistView";
import { Modal } from "./Modal";

interface Props {
  os: Os;
  source: ModlistSource;
  kind: ModlistKind | null;
  title: string;
  /** Start on this mode instead of the default for the kind ("Switch to" uses replace). */
  initialMode?: LoadMode;
  quickSwitch?: {
    start: boolean;
    preference: QuickSwitchPreference;
    onRemember: (preference: QuickSwitchPreference) => void;
    onDone: () => void;
  };
  onShowBackup: (id: string) => void;
  onClose: () => void;
}

const MODES: { mode: LoadMode; label: string; hint: string }[] = [
  { mode: "replace", label: "Switch to this list", hint: "Your Mods folder ends up with exactly this list. Other mods go to your mod library when possible, so you can switch back without downloading." },
  { mode: "add", label: "Add to my mods", hint: "Install what's missing, match versions and on/off. Your other mods stay." },
  { mode: "statesOnly", label: "Just switch mods on and off", hint: "Nothing is downloaded or removed." },
];

/** Shows what loading a modlist will change, runs it with progress, then shows the outcome. */
export function LoadModlistDialog({ os, source, kind, title, initialMode, quickSwitch, onShowBackup, onClose }: Props) {
  const [mode, setMode] = useState<LoadMode>(initialMode ?? defaultMode(kind));
  const [importConfigs, setImportConfigs] = useState(quickSwitch?.preference !== "keep-configs");
  const [remember, setRemember] = useState(!!quickSwitch && quickSwitch.preference !== "review");
  const [checkingQuick, setCheckingQuick] = useState(!!quickSwitch?.start);
  const quickStarted = useRef(false);
  const [step, setStep] = useState<ModlistProgress | null>(null);
  const [percent, setPercent] = useState<number | null>(null);
  const preview = useModlistPreview(source, mode);
  const apply = useApplyModlist();

  useModlistProgress(
    useCallback((p: ModlistProgress) => {
      setStep(p);
      setPercent(null);
    }, []),
  );
  useUpdateProgress(
    useCallback((p: UpdateProgress) => {
      if (p.stage === "downloading" && p.percent != null) setPercent(p.percent);
    }, []),
  );

  const running = apply.isPending;
  const report = apply.data;
  const p = preview.data;
  const plan = p?.plan;
  const lines = plan ? describePlan(plan, p.configFiles.length, importConfigs) : [];
  const trash = os === "windows" ? "Recycle Bin" : "Trash";

  useEffect(() => {
    if (!checkingQuick || preview.isFetching || preview.isPending || quickStarted.current) return;
    quickStarted.current = true;
    setCheckingQuick(false);
    if (!preview.isSuccess || !p || !quickSwitch || lines.length === 0 || p.toDownload > 0 || p.plan.skipped.length > 0 ||
        p.conflicts.some((conflict) => !conflict.resolvedByUser)) return;

    apply.mutate({ source, mode: "replace", importConfigs }, {
      onSuccess: (result) => {
        const movedToTrash = result.removed > result.toLibrary;
        if (!result.cancelled && !movedToTrash && result.failures.length === 0 && result.skipped.length === 0) {
          quickSwitch.onDone();
        }
      },
    });
  }, [apply, checkingQuick, importConfigs, lines.length, p, preview.isFetching, preview.isPending, preview.isSuccess, quickSwitch, source]);

  const confirm = () => {
    if (quickSwitch && mode === "replace" && p) {
      const rememberConfigs = p.configFiles.length > 0 ? importConfigs : quickSwitch.preference === "use-list-configs";
      quickSwitch.onRemember(remember ? (rememberConfigs ? "use-list-configs" : "keep-configs") : "review");
    }
    apply.mutate({ source, mode, importConfigs });
  };

  if (report) {
    const done = [
      report.installed && `installed ${report.installed}`,
      report.changed && `changed the version of ${report.changed}`,
      report.toLibrary && `put ${report.toLibrary} away in your mod library`,
      report.removed - report.toLibrary && `moved ${report.removed - report.toLibrary} to the ${trash}`,
      report.toggled && `switched ${report.toggled} on or off`,
      report.configsWritten && `wrote ${report.configsWritten} config file${report.configsWritten === 1 ? "" : "s"}`,
    ].filter(Boolean);
    return (
      <Modal title={report.failures.length ? `Loaded "${title}" with problems` : `Loaded "${title}"`} onClose={onClose}>
        <p>{done.length ? `Done: ${done.join(", ")}.` : "Nothing needed to change."}</p>
        {report.cancelled && <p className="warn-text">Stopped early; the rest was left as it was.</p>}
        {report.failures.length > 0 && (
          <div className="notice error-text">
            These didn't work:
            <ul>{report.failures.map((f) => <li key={f}>{f}</li>)}</ul>
          </div>
        )}
        {report.skipped.length > 0 && (
          <div className="notice warn-text">
            Left as they are:
            <ul>{report.skipped.map((f) => <li key={f}>{f}</li>)}</ul>
          </div>
        )}
        <div className="modal-actions">
          {report.backupId && (
            <button type="button" className="link-button small" onClick={() => onShowBackup(report.backupId!)}>
              Your previous mods were saved as a backup
            </button>
          )}
          <span className="header-spacer" />
          <button type="button" className="primary" onClick={onClose}>Close</button>
        </div>
      </Modal>
    );
  }

  return (
    <Modal title={`Load "${title}"`} onClose={onClose} busy={running}>
      {running ? (
        <div className="progress-block" aria-live="polite">
          <p>{step ? `${step.label}…` : "Starting…"}</p>
          <progress
            max={1}
            value={step ? Math.min(1, (step.step - 1 + (percent ?? 0) / 100) / step.total) : undefined}
          />
          {step && <p className="muted small">Step {step.step} of {step.total}</p>}
        </div>
      ) : checkingQuick ? (
        <p className="muted" role="status">Checking what would change…</p>
      ) : (
        <>
          <fieldset className="mode-picker">
            <legend className="sr-only">How to load it</legend>
            {MODES.map((m) => (
              <label key={m.mode} className={`mode-option${mode === m.mode ? " selected" : ""}`}>
                <input type="radio" name="mode" checked={mode === m.mode} onChange={() => setMode(m.mode)} />
                <span>
                  <strong>{m.label}</strong>
                  <span className="muted small block">{m.hint}</span>
                </span>
              </label>
            ))}
          </fieldset>
          {preview.isPending && <p className="muted">Checking what would change…</p>}
          {preview.isError && <p className="notice error-text">{preview.error.message}</p>}
          {p && plan && (
            <div className="plan">
              <p className="muted small">
                {p.modCount} mod{p.modCount === 1 ? "" : "s"} in the list
                {!p.hasVersions && mode !== "statesOnly" && " · no versions saved, so missing mods get their newest compatible release"}
              </p>
              {plan.install.length + plan.changeVersion.length > 0 && (
                <p className="muted small">
                  {p.fromLibrary > 0 && `${p.fromLibrary} from your mod library`}
                  {p.fromLibrary > 0 && p.toDownload > 0 && " · "}
                  {p.toDownload > 0 && `${p.toDownload} to download from the Mod DB`}
                </p>
              )}
              {lines.length === 0 ? (
                <p>Your mods already match this list.</p>
              ) : (
                <ul>{lines.map((l) => <li key={l}>{l}</li>)}</ul>
              )}
              {plan.remove.length > 0 && (
                <details>
                  <summary>Mods that will be put away</summary>
                  <p className="muted small">Mods that cannot be stored in the library go to the {trash}.</p>
                  <ul className="small">{plan.remove.map((r) => <li key={r.key}>{r.name}</li>)}</ul>
                </details>
              )}
              {plan.install.length > 0 && (
                <details>
                  <summary>Mods to install</summary>
                  <ul className="small">
                    {plan.install.map((i) => <li key={i.modId}>{i.modId}{i.version ? ` ${i.version}` : ""}</li>)}
                  </ul>
                </details>
              )}
              {plan.changeVersion.length > 0 && (
                <details>
                  <summary>Version changes</summary>
                  <ul className="small">
                    {plan.changeVersion.map((c) => <li key={c.key}>{c.name}: {c.from ?? "?"} → {c.to}</li>)}
                  </ul>
                </details>
              )}
              {p.configFiles.length > 0 && (
                <label className="checkbox config-toggle">
                  <input type="checkbox" checked={importConfigs} onChange={(e) => setImportConfigs(e.target.checked)} />
                  Use the list's config files ({p.configFiles.length}), replacing yours
                </label>
              )}
              {p.conflicts.length > 0 && (
                <p className="muted small">
                  {p.conflicts.length} mod{p.conflicts.length === 1 ? " is" : "s are"} listed differently by the group's modlists; the choices shown in the group are used.
                </p>
              )}
              {plan.skipped.length > 0 && <p className="warn-text small">Can't change: {plan.skipped.join(", ")}</p>}
              {quickSwitch && mode === "replace" && (
                <label className="checkbox quick-switch-option">
                  <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
                  <span>
                    <strong>Remember these options for Quick switch</strong>
                    <span className="muted small block">Switch between saved modlists and groups without opening this review each time. Your current mods are still backed up.</span>
                  </span>
                </label>
              )}
            </div>
          )}
          {apply.isError && <p className="notice error-text">{apply.error.message}</p>}
        </>
      )}
      <div className="modal-actions">
        {!running && lines.length > 0 && <span className="muted small">Your current mods are backed up first.</span>}
        <span className="header-spacer" />
        {running ? (
          <button type="button" onClick={() => void cancelUpdates()}>Stop after this one</button>
        ) : (
          <>
            <button type="button" onClick={onClose}>Cancel</button>
            {!checkingQuick && <button
              type="button"
              className="primary"
              disabled={!plan || lines.length === 0}
              onClick={confirm}
            >
              {mode === "replace" ? "Switch" : "Load"}
            </button>}
          </>
        )}
      </div>
    </Modal>
  );
}
