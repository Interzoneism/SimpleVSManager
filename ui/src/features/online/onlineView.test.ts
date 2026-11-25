import { describe, expect, it } from "vitest";
import type { OnlineEntry } from "../../bindings";
import { filterOnline, splitOwn } from "./onlineView";

const entry = (id: string, over: Partial<OnlineEntry["entry"]> = {}, extra: Partial<OnlineEntry> = {}): OnlineEntry => ({
  entry: {
    registryId: id,
    name: id,
    description: null,
    icon: null,
    version: null,
    uploader: "Bob",
    gameVersion: null,
    mods: [],
    revision: 1,
    createdMs: null,
    updatedMs: null,
    complete: false,
    ...over,
  },
  subscribers: 0,
  subscribed: false,
  mine: false,
  localId: null,
  ...extra,
});

describe("filterOnline", () => {
  it("searches names, authors and mod ids, and keeps yours and followed first", () => {
    const all = [
      entry("b", { updatedMs: 3, mods: [{ modId: "carryon", version: null }] }),
      entry("a", { updatedMs: 2, uploader: "Alice" }),
      entry("c", { updatedMs: 1 }, { subscribed: true }),
      entry("d", { updatedMs: 0 }, { mine: true }),
    ];
    expect(filterOnline(all, "", "newest").map((e) => e.entry.registryId)).toEqual(["d", "c", "b", "a"]);
    expect(filterOnline(all, "CARRY", "newest").map((e) => e.entry.registryId)).toEqual(["b"]);
    expect(filterOnline(all, "alice", "name").map((e) => e.entry.registryId)).toEqual(["a"]);
  });

  it("sorts by followers", () => {
    const all = [entry("x", {}, { subscribers: 1 }), entry("y", {}, { subscribers: 5 })];
    expect(filterOnline(all, "", "subscribers").map((e) => e.entry.registryId)).toEqual(["y", "x"]);
  });
});

describe("splitOwn", () => {
  it("puts your own published modlists in their own group, keeping the order", () => {
    const list = [entry("a"), entry("b", {}, { mine: true }), entry("c"), entry("d", {}, { mine: true })];
    const { mine, others } = splitOwn(list);
    expect(mine.map((o) => o.entry.registryId)).toEqual(["b", "d"]);
    expect(others.map((o) => o.entry.registryId)).toEqual(["a", "c"]);
  });
});
