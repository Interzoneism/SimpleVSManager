import { describe, expect, it } from "vitest";
import { coerce, getAt, matchesSearch, setAt, stringify, type Json } from "./jsonTree";

describe("json tree helpers", () => {
  const doc: Json = { b: 1, a: { list: [true, "x"] }, c: null };

  it("sets nested values without changing key order", () => {
    const next = setAt(doc, ["a", "list", 1], "y");
    expect(getAt(next, ["a", "list", 1])).toBe("y");
    expect(Object.keys(next as object)).toEqual(["b", "a", "c"]);
    expect(getAt(doc, ["a", "list", 1])).toBe("x");
    expect(stringify(next)).toContain('"y"');
  });

  it("coerces typed text to the old type", () => {
    expect(coerce(1, "2.5")).toEqual({ ok: true, value: 2.5 });
    expect(coerce(1, "abc").ok).toBe(false);
    expect(coerce(false, "true")).toEqual({ ok: true, value: true });
    expect(coerce(null, "")).toEqual({ ok: true, value: null });
    expect(coerce("s", "1")).toEqual({ ok: true, value: "1" });
  });

  it("searches keys and values", () => {
    expect(matchesSearch("root", doc, "LIST")).toBe(true);
    expect(matchesSearch("root", doc, "zzz")).toBe(false);
  });
});
