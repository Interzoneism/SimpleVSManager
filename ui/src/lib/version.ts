/** Same normalisation as the backend (`svsm_core::version::normalize`): up to four numeric parts. */
export function normalizeVersion(version: string | null | undefined): number[] | null {
  if (!version) return null;
  const parts = version.match(/\d+/g);
  return parts ? parts.slice(0, 4).map(Number) : null;
}

/** Compares loose versions numerically; missing versions sort first. */
export function compareVersions(a: string | null | undefined, b: string | null | undefined): number {
  const pa = normalizeVersion(a);
  const pb = normalizeVersion(b);
  if (!pa || !pb) return (pa ? 1 : 0) - (pb ? 1 : 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}
