import { useEffect } from "react";
import type { InstalledMod } from "../../bindings";
import { useFixDependencies } from "../../api";
import { Icon } from "../../components/Icon";
import { useErrorToast } from "../../components/Toast";

/** Whether "Fix dependencies" can help: something it depends on is missing, too old, disabled or broken. */
export function canFixDependencies(mod: InstalledMod): boolean {
  return !mod.error && (mod.missingDependencies.length > 0 || mod.dependencyHasErrors);
}

/**
 * "Fix dependencies" button plus its result, for the details panel. Installs missing
 * dependencies from the Mod DB, updates outdated ones, and enables disabled ones.
 */
export function FixDependencies({ mod, busy }: { mod: InstalledMod; busy: boolean }) {
  const fix = useFixDependencies();
  useErrorToast(fix.error);
  const { reset } = fix;
  useEffect(() => reset(), [mod.key, reset]);

  const report = fix.data;
  if (!canFixDependencies(mod) && !report && !fix.error) return null;

  return (
    <div className="fix-dependencies">
      {canFixDependencies(mod) && (
        <button
          type="button"
          className="primary"
          disabled={busy || fix.isPending}
          onClick={() => fix.mutate(mod.key)}
        >
          <Icon name="wrench" />{fix.isPending ? "Fixing dependencies…" : "Fix dependencies"}
        </button>
      )}
      {report && report.done.length > 0 && (
        <ul className="details-list small">
          {report.done.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
      {report && report.done.length === 0 && report.failures.length === 0 && (
        <p className="muted small">Nothing to fix.</p>
      )}
      {report && report.failures.length > 0 && (
        <ul className="details-list small error-text">
          {report.failures.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
