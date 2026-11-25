/** Helpers for editing JSON config values in place, keeping key order. */

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json };
export type PathPart = string | number;

export function getAt(root: Json, path: PathPart[]): Json {
  let cur: Json = root;
  for (const p of path) {
    if (cur === null || typeof cur !== "object") return null;
    cur = (cur as Record<string, Json>)[p as string] ?? null;
  }
  return cur;
}

/** A copy of `root` with the value at `path` replaced. */
export function setAt(root: Json, path: PathPart[], value: Json): Json {
  if (path.length === 0) return value;
  const [head, ...rest] = path;
  if (Array.isArray(root)) {
    const copy = root.slice();
    copy[head as number] = setAt(root[head as number] ?? null, rest, value);
    return copy;
  }
  if (root !== null && typeof root === "object") {
    // Rebuild in the same key order.
    const out: Record<string, Json> = {};
    for (const [k, v] of Object.entries(root)) out[k] = k === head ? setAt(v, rest, value) : v;
    return out;
  }
  return root;
}

/** Parses what the user typed into a value of the same kind as before. */
export function coerce(previous: Json, typed: string): { ok: true; value: Json } | { ok: false; error: string } {
  if (typeof previous === "number") {
    const n = Number(typed.trim());
    return typed.trim() !== "" && Number.isFinite(n) ? { ok: true, value: n } : { ok: false, error: "Enter a number." };
  }
  if (typeof previous === "boolean") return { ok: true, value: typed === "true" };
  if (previous === null) {
    if (typed.trim() === "" || typed.trim() === "null") return { ok: true, value: null };
    return { ok: true, value: typed };
  }
  return { ok: true, value: typed };
}

/** Pretty JSON with 2-space indent (the format most mods write). */
export function stringify(value: Json): string {
  return JSON.stringify(value, null, 2);
}

export function matchesSearch(key: string, value: Json, q: string): boolean {
  if (!q) return true;
  const s = q.toLowerCase();
  if (key.toLowerCase().includes(s)) return true;
  if (value === null || typeof value !== "object") return String(value).toLowerCase().includes(s);
  return Object.entries(value).some(([k, v]) => matchesSearch(k, v, q));
}
