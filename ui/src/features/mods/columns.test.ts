import { DEFAULT_LAYOUT, gridFor, parseLayout, setWidth, toggleHidden, visibleColumns } from "./columns";

test("default layout shows every column, name takes the spare room", () => {
  expect(visibleColumns(DEFAULT_LAYOUT).map((c) => c.id)).toEqual(["active", "name", "version", "latest", "authors", "tags", "side", "modified", "status"]);
  const g = gridFor(DEFAULT_LAYOUT);
  expect(g.template.startsWith("38px minmax(220px, 1fr) 80px")).toBe(true);
  expect(g.minWidth).toBe(38 + 220 + 80 + 92 + 130 + 120 + 70 + 100 + 120);
});

test("hide, resize and round-trip", () => {
  let l = toggleHidden(DEFAULT_LAYOUT, "tags");
  l = setWidth(l, "authors", 10); // clamped to the minimum
  l = setWidth(l, "name", 300.4);
  const back = parseLayout(JSON.stringify(l));
  expect(back).toEqual({ hidden: ["tags"], widths: { authors: 50, name: 300 } });
  expect(visibleColumns(back).some((c) => c.id === "tags")).toBe(false);
  expect(gridFor(back).template).toContain("minmax(300px, 1fr)");
  expect(toggleHidden(back, "tags").hidden).toEqual([]);
  expect(setWidth(back, "name", null).widths).toEqual({ authors: 50 });
});

test("bad saved layouts fall back safely", () => {
  expect(parseLayout("not json")).toEqual(DEFAULT_LAYOUT);
  // Unknown ids, unhideable columns and non-numbers are dropped.
  expect(parseLayout(JSON.stringify({ hidden: ["name", "active", "nope", "side"], widths: { active: 90, side: "x", status: 1e9 } })))
    .toEqual({ hidden: ["side"], widths: { status: 800 } });
});
