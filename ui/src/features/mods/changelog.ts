import type { ModDbRelease } from "../../bindings";
import { compareVersions } from "../../lib/version";

/**
 * Releases a user gets by moving from `installed` to `target`, newest first: everything from
 * the target release down to (not including) the installed one. Rollbacks give just the target.
 */
export function releasesBetween(
  releases: ModDbRelease[],
  installed: string | null,
  target: string,
): ModDbRelease[] {
  const start = releases.findIndex((r) => r.version === target);
  if (start < 0) return [];
  if (installed && compareVersions(target, installed) <= 0) return [releases[start]];
  const out: ModDbRelease[] = [];
  for (const r of releases.slice(start)) {
    if (installed && (r.version === installed || compareVersions(r.version, installed) <= 0)) break;
    // The Mod DB interleaves branches (a 1.22-only 1.14 pre-release between 1.13 builds); skip
    // anything newer than the target.
    if (compareVersions(r.version, target) > 0) continue;
    out.push(r);
    if (out.length >= 15) break;
  }
  return out;
}
