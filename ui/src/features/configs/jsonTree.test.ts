import { describe, expect, it } from "vitest";
import { coerce, countChanges, getAt, humanize, JsonNumber, matchesSearch, newItemLike, num, obj, parse, setAt, splitMatches, stepNumber, stringify, type Json } from "./jsonTree";

describe("json tree helpers", () => {
  const doc: Json = obj({ b: num(1), a: obj({ list: [true, "x"] }), c: null });

  it("sets nested values without changing key order", () => {
    const next = setAt(doc, ["a", "list", 1], "y");
    expect(getAt(next, ["a", "list", 1])).toBe("y");
    expect(stringify(next).indexOf('"b"')).toBeLessThan(stringify(next).indexOf('"a"'));
    expect(getAt(doc, ["a", "list", 1])).toBe("x");
    expect(stringify(next)).toContain('"y"');
  });

  it("keeps key order, number text and formatting through a round trip", () => {
    const text = '{"z": 1.0, "10": 2, "big": 12345678901234567890, "e": -1.5e3, "s": "a\\"b", "n": null, "o": {}, "l": []}';
    const pretty = stringify(parse(text));
    expect(Object.keys(JSON.parse(pretty))).toEqual(["10", "z", "big", "e", "s", "n", "o", "l"]); // JS reorders; the file doesn't
    expect(pretty.indexOf('"z"')).toBeLessThan(pretty.indexOf('"10"'));
    expect(pretty).toContain('"z": 1.0');
    expect(pretty).toContain("12345678901234567890");
    expect(pretty).toContain("-1.5e3");
    const plain = { a: [1, { b: "x" }], c: {}, d: [] };
    expect(stringify(parse(JSON.stringify(plain)))).toBe(JSON.stringify(plain, null, 2));
  });

  it("rejects bad JSON", () => {
    expect(() => parse("{ a: 1 }")).toThrow(SyntaxError);
    expect(() => parse("[1,]")).toThrow(SyntaxError);
    expect(() => parse("1 2")).toThrow(SyntaxError);
  });

  it("coerces typed text to the old type", () => {
    expect(coerce(num(1), "2.5")).toEqual({ ok: true, value: num("2.5") });
    expect(coerce(num(1), "abc").ok).toBe(false);
    expect(coerce(false, "true")).toEqual({ ok: true, value: true });
    expect(coerce(null, "")).toEqual({ ok: true, value: null });
    expect(coerce("s", "1")).toEqual({ ok: true, value: "1" });
  });

  it("steps numbers keeping their decimals", () => {
    expect(stepNumber(num("0"), 1).raw).toBe("1");
    expect(stepNumber(num("0.25"), -1).raw).toBe("0.15");
    expect(stepNumber(num("0.05"), 1).raw).toBe("0.06");
    expect(stepNumber(num("0.0"), 1).raw).toBe("0.1");
    expect(stepNumber(num("1.0"), 10).raw).toBe("11.0");
    expect(stepNumber(num("-3"), -1).raw).toBe("-4");
  });

  it("makes readable labels from keys", () => {
    expect(humanize("RequireSneaking")).toBe("Require sneaking");
    expect(humanize("BedsThatDontRequireRooms")).toBe("Beds that dont require rooms");
    expect(humanize("enableDebugMessages")).toBe("Enable debug messages");
    expect(humanize("maxHPRegen")).toBe("Max HP regen");
    expect(humanize("tier_2_chance")).toBe("Tier 2 chance");
    expect(humanize("Enabled")).toBe("Enabled");
  });

  it("counts changes and makes new list items", () => {
    const next = setAt(setAt(doc, ["b"], num(2)), ["a", "list"], [true, "x", "new"]);
    expect(countChanges(next, doc)).toBe(2);
    expect(newItemLike([obj({ id: "x", n: num(3), on: true })])).toEqual(obj({ id: "", n: num(0), on: false }));
    expect(newItemLike([])).toBe("");
    expect(new JsonNumber("3").value).toBe(3);
  });

  it("searches keys, labels and values, and splits matches", () => {
    expect(matchesSearch("root", doc, "LIST")).toBe(true);
    expect(matchesSearch("root", doc, "zzz")).toBe(false);
    expect(matchesSearch("RequireSneaking", false, "require sn")).toBe(true);
    expect(splitMatches("Require sneaking", "sne")).toEqual([{ text: "Require ", hit: false }, { text: "sne", hit: true }, { text: "aking", hit: false }]);
  });
});
