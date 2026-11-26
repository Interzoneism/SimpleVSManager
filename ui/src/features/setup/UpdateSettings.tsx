import { useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { DetectedEnvironment } from "../../bindings";
import { useBrowseFacets, useClearModCache, useModCacheStats, useUpdatePreferences } from "../../api";
import { formatBytes } from "../../lib/format";
import { Icon } from "../../components/Icon";

const CACHE_CHOICES = [0, 512, 1024, 2048, 5120, 10240];

/** Mod DB and update preferences, on the Settings tab. */
export function UpdateSettings({ env }: { env: DetectedEnvironment }) {
  const prefs = useUpdatePreferences();
  const stats = useModCacheStats();
  const clear = useClearModCache();
  const s = env.settings;
  const facets = useBrowseFacets(!s.offlineMode);
  const [custom, setCustom] = useState(false);

  const target = s.targetGameVersion ?? "";
  const known = facets.data?.gameVersions ?? [];
  const options = target && target !== "latest" && !known.includes(target) ? [target, ...known] : known;

  return (
    <section className="settings-section" aria-labelledby="updates-heading">
      <h2 id="updates-heading">Mod DB and updates</h2>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={s.offlineMode}
          disabled={prefs.isPending}
          onChange={(e) => prefs.mutate({ offlineMode: e.target.checked })}
        />
        Offline mode
      </label>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={s.modUsageTracking}
          disabled={prefs.isPending}
          onChange={(e) => prefs.mutate({ modUsageTracking: e.target.checked })}
        />
        Ask me which mods work after long play sessions
      </label>

      <div className="setting-row">
        <label htmlFor="target-version">Find updates for</label>
        <select
          id="target-version"
          value={custom ? "__custom" : target}
          disabled={prefs.isPending}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "__custom") {
              setCustom(true);
              return;
            }
            setCustom(false);
            prefs.mutate({ targetGameVersion: v });
          }}
        >
          <option value="">The installed game ({env.gameVersion ?? "unknown"})</option>
          <option value="latest">Any game version</option>
          {options.length > 0 && (
            <optgroup label="A specific game version">
              {options.map((v) => (
                <option key={v} value={v}>
                  Vintage Story {v}
                </option>
              ))}
            </optgroup>
          )}
        </select>
      </div>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={s.requireExactVersionMatch}
          disabled={prefs.isPending || target === "latest"}
          onChange={(e) => prefs.mutate({ requireExactVersionMatch: e.target.checked })}
        />
        Require the exact game version (1.21.5, not 1.21.x)
      </label>

      <div className="setting-row">
        <label htmlFor="cache-limit">Keep old and downloaded mod versions</label>
        <select
          id="cache-limit"
          value={s.modCacheLimitMb}
          disabled={prefs.isPending}
          onChange={(e) => prefs.mutate({ modCacheLimitMb: Number(e.target.value) })}
        >
          {[...new Set([...CACHE_CHOICES, s.modCacheLimitMb])].sort((a, b) => a - b).map((mb) => (
            <option key={mb} value={mb}>
              {mb === 0 ? "Don't keep any" : `Up to ${formatBytes(mb * 1024 * 1024)}`}
            </option>
          ))}
        </select>
        {stats.data && <span className="muted small">Using {formatBytes(stats.data.bytes)}</span>}
        <button
          type="button"
          disabled={clear.isPending || !stats.data?.bytes}
          onClick={async () => {
            const ok = await ask("Delete every cached mod version and cached Mod DB data?", {
              title: "Clear cache",
              kind: "warning",
              okLabel: "Clear",
              cancelLabel: "Cancel",
            });
            if (ok) clear.mutate();
          }}
        >
          <Icon name="trash" />Clear cache
        </button>
      </div>
      {(prefs.error ?? clear.error) && <p className="error-text small">{(prefs.error ?? clear.error)!.message}</p>}
    </section>
  );
}
