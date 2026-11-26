/**
 * Helpers for editing JSON config values in place.
 *
 * The tree keeps what plain `JSON.parse` would lose: objects keep their exact key order (JS
 * objects move integer-like keys such as "10" to the front) and numbers keep the text they were
 * written with (`1.0` stays `1.0`, a 64-bit id stays exact). Saving an untouched value writes it
 * back as it came.
 */

/** A number, kept as written. */
export class JsonNumber {
  constructor(readonly raw: string) {}
  get value(): number {
    return Number(this.raw);
  }
}

/** An object, kept in key order. */
export class JsonObject {
  constructor(readonly entries: [string, Json][]) {}
  get(key: string): Json | undefined {
    return this.entries.find(([k]) => k === key)?.[1];
  }
  has(key: string): boolean {
    return this.entries.some(([k]) => k === key);
  }
}

export type Json = null | boolean | string | JsonNumber | Json[] | JsonObject;
export type PathPart = string | number;

export const isContainer = (v: Json): v is Json[] | JsonObject => Array.isArray(v) || v instanceof JsonObject;
export const num = (raw: string | number) => new JsonNumber(String(raw));
export const obj = (o: Record<string, Json>) => new JsonObject(Object.entries(o));

/** Children of a container as [key, value] pairs (array indexes as numbers). */
export function entriesOf(v: Json[] | JsonObject): [PathPart, Json][] {
  return Array.isArray(v) ? v.map((x, i) => [i, x]) : v.entries;
}

// ---------------------------------------------------------------------------------------------
// Parse / stringify

const NUMBER = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?$/;

/** Strict JSON parser that builds the order-keeping tree. Throws a SyntaxError on bad input. */
export function parse(text: string): Json {
  let i = 0;
  const fail = (what: string): never => {
    throw new SyntaxError(`${what} at position ${i}`);
  };
  const ws = () => {
    while (i < text.length && " \t\n\r".includes(text[i])) i++;
  };
  const expect = (word: string) => {
    if (text.startsWith(word, i)) i += word.length;
    else fail(`Expected ${word}`);
  };
  const str = (): string => {
    const start = i;
    i++; // opening quote
    while (i < text.length && text[i] !== '"') i += text[i] === "\\" ? 2 : 1;
    if (i >= text.length) fail("Unterminated string");
    i++;
    return JSON.parse(text.slice(start, i)) as string;
  };
  const value = (): Json => {
    ws();
    const c = text[i];
    if (c === "{") {
      i++;
      const entries: [string, Json][] = [];
      ws();
      if (text[i] === "}") {
        i++;
        return new JsonObject(entries);
      }
      for (;;) {
        ws();
        if (text[i] !== '"') fail("Expected a key");
        const key = str();
        ws();
        expect(":");
        const v = value();
        const at = entries.findIndex(([k]) => k === key);
        if (at >= 0) entries[at] = [key, v]; // a repeated key wins, as in JSON.parse
        else entries.push([key, v]);
        ws();
        if (text[i] === ",") i++;
        else if (text[i] === "}") {
          i++;
          return new JsonObject(entries);
        } else fail("Expected , or }");
      }
    }
    if (c === "[") {
      i++;
      const items: Json[] = [];
      ws();
      if (text[i] === "]") {
        i++;
        return items;
      }
      for (;;) {
        items.push(value());
        ws();
        if (text[i] === ",") i++;
        else if (text[i] === "]") {
          i++;
          return items;
        } else fail("Expected , or ]");
      }
    }
    if (c === '"') return str();
    if (text.startsWith("true", i)) return (i += 4), true;
    if (text.startsWith("false", i)) return (i += 5), false;
    if (text.startsWith("null", i)) return (i += 4), null;
    const m = /^-?\d[\d.eE+-]*/.exec(text.slice(i));
    if (m && NUMBER.test(m[0])) {
      i += m[0].length;
      return new JsonNumber(m[0]);
    }
    return fail("Unexpected token");
  };
  const out = value();
  ws();
  if (i < text.length) fail("Unexpected text after the end");
  return out;
}

/** Pretty JSON with 2-space indent (the format most mods write), like JSON.stringify(v, null, 2). */
export function stringify(value: Json, indent = ""): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return String(value);
  if (typeof value === "string") return JSON.stringify(value);
  if (value instanceof JsonNumber) return value.raw;
  const inner = indent + "  ";
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    return `[\n${value.map((v) => inner + stringify(v, inner)).join(",\n")}\n${indent}]`;
  }
  if (value.entries.length === 0) return "{}";
  return `{\n${value.entries.map(([k, v]) => `${inner}${JSON.stringify(k)}: ${stringify(v, inner)}`).join(",\n")}\n${indent}}`;
}

export const sameJson = (a: Json | undefined, b: Json | undefined) =>
  a === undefined || b === undefined ? a === b : stringify(a) === stringify(b);

// ---------------------------------------------------------------------------------------------
// Reading and changing

/** The value at `path`, or undefined when it isn't there. */
export function lookup(root: Json | undefined, path: PathPart[]): Json | undefined {
  let cur: Json | undefined = root;
  for (const p of path) {
    if (Array.isArray(cur)) cur = typeof p === "number" ? cur[p] : undefined;
    else if (cur instanceof JsonObject) cur = cur.get(String(p));
    else return undefined;
  }
  return cur;
}

export const getAt = (root: Json, path: PathPart[]): Json => lookup(root, path) ?? null;

/** A copy of `root` with the value at `path` replaced (same key order). */
export function setAt(root: Json, path: PathPart[], value: Json): Json {
  if (path.length === 0) return value;
  const [head, ...rest] = path;
  if (Array.isArray(root)) {
    const copy = root.slice();
    copy[head as number] = setAt(root[head as number] ?? null, rest, value);
    return copy;
  }
  if (root instanceof JsonObject) return new JsonObject(root.entries.map(([k, v]) => [k, k === head ? setAt(v, rest, value) : v]));
  return root;
}

/** A value to add to a list: a fresh copy of the last item's shape, or empty text. */
export function newItemLike(items: Json[]): Json {
  const blank = (v: Json): Json => {
    if (v === null) return null;
    if (typeof v === "boolean") return false;
    if (typeof v === "string") return "";
    if (v instanceof JsonNumber) return num(0);
    if (Array.isArray(v)) return [];
    return new JsonObject(v.entries.map(([k, x]) => [k, blank(x)]));
  };
  return items.length === 0 ? "" : blank(items[items.length - 1]);
}

/** Parses what the user typed into a value of the same kind as before. */
export function coerce(previous: Json, typed: string): { ok: true; value: Json } | { ok: false; error: string } {
  if (previous instanceof JsonNumber) {
    const t = typed.trim();
    if (NUMBER.test(t)) return { ok: true, value: num(t) };
    const n = Number(t);
    return t !== "" && Number.isFinite(n) ? { ok: true, value: num(n) } : { ok: false, error: "Enter a number." };
  }
  if (typeof previous === "boolean") return { ok: true, value: typed === "true" };
  if (previous === null) {
    if (typed.trim() === "" || typed.trim() === "null") return { ok: true, value: null };
    return { ok: true, value: typed };
  }
  return { ok: true, value: typed };
}

/**
 * The number `steps` steps away. The step suits the value's size (1 for 1 and up, 0.1 for 0.25,
 * 0.01 for 0.05), and the result keeps at least as many decimals as the value had.
 */
export function stepNumber(value: JsonNumber, steps: number): JsonNumber {
  if (/e/i.test(value.raw)) return num(value.value + steps);
  const decimals = /\.(\d+)$/.exec(value.raw)?.[1].length ?? 0;
  const v = value.value;
  const exp = v === 0 ? -decimals : Math.min(0, Math.floor(Math.log10(Math.abs(v))));
  const places = Math.max(decimals, -exp);
  return num((v + steps * 10 ** exp).toFixed(places));
}

/** How many settings differ between two trees (an added or removed list item counts once). */
export function countChanges(a: Json | undefined, b: Json | undefined): number {
  if (a === undefined || b === undefined) return a === b ? 0 : 1;
  if (Array.isArray(a) && Array.isArray(b)) {
    let n = Math.abs(a.length - b.length);
    for (let i = 0; i < Math.min(a.length, b.length); i++) n += countChanges(a[i], b[i]);
    return n;
  }
  if (a instanceof JsonObject && b instanceof JsonObject) {
    let n = 0;
    for (const [k, v] of a.entries) n += countChanges(v, b.get(k));
    for (const [k] of b.entries) if (!a.has(k)) n++;
    return n;
  }
  return sameJson(a, b) ? 0 : 1;
}

// ---------------------------------------------------------------------------------------------
// Labels and search

/** "RequireSneaking" → "Require sneaking", "maxHPRegen" → "Max HP regen", "tier_2" → "Tier 2". */
export function humanize(key: string): string {
  const words = key.replace(/[_\-.]+/g, " ").match(/[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+|\d+|[^\sA-Za-z\d]+/g);
  if (!words) return key;
  return words
    .map((w, i) => {
      if (w.length > 1 && w === w.toUpperCase()) return w; // an acronym
      const lower = w.toLowerCase();
      return i === 0 ? lower.charAt(0).toUpperCase() + lower.slice(1) : lower;
    })
    .join(" ");
}

const keyMatches = (key: string, q: string) => key.toLowerCase().includes(q) || humanize(key).toLowerCase().includes(q);

export function matchesSearch(key: string, value: Json, q: string): boolean {
  if (!q) return true;
  const s = q.toLowerCase();
  if (keyMatches(key, s)) return true;
  if (!isContainer(value)) return (value instanceof JsonNumber ? value.raw : String(value)).toLowerCase().includes(s);
  return entriesOf(value).some(([k, v]) => matchesSearch(String(k), v, q));
}

/** Splits `text` around case-insensitive matches of `q`, for highlighting. */
export function splitMatches(text: string, q: string): { text: string; hit: boolean }[] {
  if (!q) return [{ text, hit: false }];
  const out: { text: string; hit: boolean }[] = [];
  const lower = text.toLowerCase();
  const s = q.toLowerCase();
  let at = 0;
  for (let i = lower.indexOf(s); i >= 0; i = lower.indexOf(s, at)) {
    if (i > at) out.push({ text: text.slice(at, i), hit: false });
    out.push({ text: text.slice(i, i + s.length), hit: true });
    at = i + s.length;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}
