import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { BrowseEntry, BrowsePage, ModlistSummary } from "../../bindings";
import { commands } from "../../bindings";
import { forgetAll } from "../../lib/pageMemory";
import { builder, emptyState, getBuilder, isDirty, resetBuilderForTests, signature, type BuilderEntry } from "./builderStore";
import { ModBrowser } from "./ModBrowser";
import { BuilderToggle } from "./ModlistBuilder";

// Node's own (disabled) localStorage hides jsdom's: use simple in-memory ones.
vi.hoisted(() => {
  class MemoryStorage {
    private items = new Map<string, string>();
    get length() { return this.items.size; }
    key(i: number) { return [...this.items.keys()][i] ?? null; }
    getItem(k: string) { return this.items.get(k) ?? null; }
    setItem(k: string, v: string) { this.items.set(k, String(v)); }
    removeItem(k: string) { this.items.delete(k); }
    clear() { this.items.clear(); }
  }
  (globalThis as { ResizeObserver?: unknown }).ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} };
  for (const name of ["localStorage", "sessionStorage"]) {
    Object.defineProperty(globalThis, name, { value: new MemoryStorage(), configurable: true, writable: true });
  }
});

vi.mock("../../bindings", () => ({
  commands: {
    getEnvironment: vi.fn(),
    browseMods: vi.fn(),
    browseFacets: vi.fn(),
    browseThumbnails: vi.fn(),
    listModlists: vi.fn(),
    builderVersions: vi.fn(),
    modlistModCards: vi.fn(),
    saveBuiltModlist: vi.fn(),
  },
  events: { modsChanged: { listen: async () => () => {} } },
}));

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

function browseEntry(modId: string, name: string, downloads: number): BrowseEntry {
  return {
    modId, modIds: [modId], numericId: 1, assetId: 1, name, summary: null, author: "Tyron", urlAlias: null, side: "both",
    kind: "mod", logoUrl: null, tags: [], downloads, follows: 0, comments: 0, trendingPoints: 0, lastReleasedMs: null,
    installed: false, favourite: false, suggestion: null,
  };
}

const saved: ModlistSummary = {
  id: "Modlists/Winter.json", fileName: "Winter.json", kind: "modlist", name: "Winter", description: null, icon: null,
  iconColor: null, version: null, gameVersion: null, uploader: null, modifiedMs: null,
  mods: [{ modId: "carryon", version: "1.8.0", isActive: true }], configCount: 0, hasVersions: true, error: null,
  group: null, active: false, downloadsNeeded: 0, subscription: null,
    published: null,
};

const page: BrowsePage = {
  total: 2,
  fetchedMs: Date.now(),
  suggestBasis: null,
  items: [browseEntry("carryon", "Carry On", 5000), browseEntry("primitivesurvival", "Primitive Survival", 9000)],
};

const mocked = commands as unknown as Record<string, ReturnType<typeof vi.fn>>;

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  forgetAll();
  resetBuilderForTests();
  mocked.getEnvironment.mockReturnValue(ok({ settings: { offlineMode: false } }));
  mocked.browseMods.mockReturnValue(ok(page));
  mocked.browseFacets.mockReturnValue(ok({ tags: [], gameVersions: [] }));
  mocked.browseThumbnails.mockReturnValue(ok({}));
  mocked.builderVersions.mockImplementation((ids: string[]) => ok(ids.map((modId) => ({ modId, version: "2.0.0", compatible: true }))));
  mocked.modlistModCards.mockImplementation((ids: string[]) =>
    ok(ids.map((modId) => ({ modId, name: `Card ${modId}`, author: null, side: null, logoUrl: null, pageUrl: null, downloads: 1,
      installedVersion: modId === "carryon" ? "1.8.0" : null, libraryVersions: [], notOnModDb: false }))));
  const lists = [saved];
  mocked.listModlists.mockImplementation(() => ok([...lists]));
  mocked.saveBuiltModlist.mockImplementation((r: { id: string | null; name: string; mods: { modId: string; version: string | null }[] }) => {
    const out = { ...saved, id: r.id ?? `Modlists/${r.name}.json`, name: r.id ? saved.name : r.name, mods: r.mods.map((m) => ({ ...m, isActive: true })) };
    if (!lists.some((l) => l.id === out.id)) lists.push(out);
    return ok(out);
  });
  // jsdom has no layout: put the builder pane on the right half of the window.
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const r = this.classList.contains("builder-pane") ? { left: 600, right: 1000, top: 0, bottom: 800 } : { left: 0, right: 0, top: 0, bottom: 0 };
    return { ...r, x: r.left, y: r.top, width: r.right - r.left, height: r.bottom - r.top, toJSON: () => r } as DOMRect;
  });
});

afterEach(() => vi.restoreAllMocks());

function renderBrowser() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <BuilderToggle />
      <ModBrowser enabled />
    </QueryClientProvider>,
  );
}

/** Drags from an element to a point with pointer events, like a mouse would. */
function drag(from: Element, to: { x: number; y: number }) {
  fireEvent.pointerDown(from, { button: 0, clientX: 100, clientY: 100, isPrimary: true });
  fireEvent.pointerMove(window, { clientX: 120, clientY: 110 });
  fireEvent.pointerMove(window, { clientX: to.x, clientY: to.y });
  fireEvent.pointerUp(window, { clientX: to.x, clientY: to.y });
  fireEvent.click(from);
}

const INSIDE = { x: 800, y: 300 };
const OUTSIDE = { x: 200, y: 300 };

test("drag cards into the builder, then save a new modlist with the newest compatible versions", async () => {
  renderBrowser();
  fireEvent.click(await screen.findByRole("button", { name: /Modlist builder/ }));
  const pane = screen.getByRole("complementary", { name: "Modlist builder" });
  expect(within(pane).getByText("Drag mods here")).toBeInTheDocument();

  const card = screen.getByRole("listitem", { name: /Carry On/ });
  drag(card, INSIDE);
  // A drop that ends a drag doesn't open the details window.
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  const table = within(pane).getByRole("grid", { name: "Mods in the modlist" });
  await waitFor(() => expect(within(table).getByText("2.0.0")).toBeInTheDocument());
  expect(mocked.builderVersions).toHaveBeenCalledWith(["carryon"]);
  expect(screen.getByRole("listitem", { name: /Carry On/ })).toHaveTextContent("In list");

  // Dropping outside the pane adds nothing; dropping the same mod twice adds it once.
  drag(screen.getByRole("listitem", { name: /Primitive Survival/ }), OUTSIDE);
  drag(card, INSIDE);
  expect(getBuilder().entries.map((e) => e.modId)).toEqual(["carryon"]);

  const save = within(pane).getByRole("button", { name: "Save modlist" });
  expect(save).toBeDisabled(); // needs a name
  fireEvent.change(within(pane).getByRole("textbox", { name: "Name for the new modlist" }), { target: { value: "Summer" } });
  fireEvent.click(save);
  await waitFor(() => expect(mocked.saveBuiltModlist).toHaveBeenCalledWith({ id: null, name: "Summer", mods: [{ modId: "carryon", version: "2.0.0" }] }));
  await waitFor(() => expect(getBuilder().sourceId).toBe("Modlists/Summer.json"));
  expect(isDirty(getBuilder())).toBe(false);
});

test("drag a row out of the list to remove it", async () => {
  builder.setOpen(true);
  builder.add([{ modId: "carryon", version: "1.0.0", compatible: true, name: "Carry On", author: null, side: null, logoUrl: null, downloads: 3 }]);
  renderBrowser();
  const pane = await screen.findByRole("complementary", { name: "Modlist builder" });
  const row = within(pane).getByText("Carry On").closest('[role="row"]')!;
  drag(row, INSIDE); // moved within the list: stays
  expect(getBuilder().entries).toHaveLength(1);
  drag(row, OUTSIDE);
  expect(getBuilder().entries).toHaveLength(0);
});

test("the list, search and scroll position survive leaving the page", async () => {
  const first = renderBrowser();
  fireEvent.click(await screen.findByRole("button", { name: /Modlist builder/ }));
  drag(screen.getByRole("listitem", { name: /Carry On/ }), INSIDE);
  fireEvent.change(screen.getByRole("searchbox", { name: "Search the Mod DB" }), { target: { value: "carry" } });
  const scroller = document.querySelector(".browse-scroll") as HTMLElement;
  scroller.scrollTop = 340;
  fireEvent.scroll(scroller);
  first.unmount();

  // Back on the page: jsdom has no layout, so give the scroll box room to scroll.
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(5000);
  renderBrowser();
  const pane = await screen.findByRole("complementary", { name: "Modlist builder" });
  expect(within(pane).getByText("Carry On")).toBeInTheDocument();
  expect(screen.getByRole("searchbox", { name: "Search the Mod DB" })).toHaveValue("carry");
  await waitFor(() => expect((document.querySelector(".browse-scroll") as HTMLElement).scrollTop).toBe(340));
});

test("editing a saved modlist keeps its pinned versions and asks before discarding changes", async () => {
  builder.setOpen(true);
  renderBrowser();
  const pane = await screen.findByRole("complementary", { name: "Modlist builder" });
  const picker = within(pane).getByRole("combobox", { name: "Modlist to edit" });
  await waitFor(() => expect(within(picker).getByRole("option", { name: "Winter" })).toBeInTheDocument());
  fireEvent.change(picker, { target: { value: saved.id } });
  await waitFor(() => expect(within(pane).getByText("Card carryon")).toBeInTheDocument());
  expect(within(pane).getByText("1.8.0")).toBeInTheDocument();
  await waitFor(() => expect(within(pane).getByText("Installed")).toBeInTheDocument()); // the Library column
  expect(mocked.builderVersions).not.toHaveBeenCalled();
  expect(within(pane).getByRole("button", { name: "Save changes" })).toBeDisabled(); // nothing changed yet

  drag(screen.getByRole("listitem", { name: /Primitive Survival/ }), INSIDE);
  await waitFor(() => expect(within(pane).getByRole("button", { name: "Save changes" })).toBeEnabled());

  // Cancel asks first; "Keep editing" keeps the list.
  fireEvent.click(within(pane).getByRole("button", { name: "Cancel" }));
  fireEvent.click(within(pane).getByRole("button", { name: "Keep editing" }));
  expect(getBuilder().entries).toHaveLength(2);

  fireEvent.click(within(pane).getByRole("button", { name: "Save changes" }));
  await waitFor(() =>
    expect(mocked.saveBuiltModlist).toHaveBeenCalledWith({
      id: saved.id, name: "Winter",
      mods: [{ modId: "carryon", version: "1.8.0" }, { modId: "primitivesurvival", version: "2.0.0" }],
    }));

  // Cancel with nothing unsaved goes straight back to a new, empty list.
  await waitFor(() => expect(isDirty(getBuilder())).toBe(false));
  fireEvent.click(within(pane).getByRole("button", { name: "Cancel" }));
  expect(getBuilder().entries).toHaveLength(0);
  expect(getBuilder().sourceId).toBeNull();
});

test("builder state survives a restart", async () => {
  const entry = (modId: string, name: string, downloads: number, version: string): BuilderEntry =>
    ({ modId, version, compatible: true, name, author: null, side: null, logoUrl: null, downloads });
  const entries = [entry("b", "Beta", 5, "1.10.0"), entry("a", "alpha", 9, "1.9.0")];
  const state = { ...emptyState(), open: true, name: "Draft", entries };
  expect(signature(state)).not.toBe(state.baseline);
  localStorage.setItem("svsm-builder", JSON.stringify(state));
  vi.resetModules();
  const fresh = await import("./builderStore");
  expect(fresh.getBuilder()).toMatchObject({ open: true, name: "Draft", entries });
  localStorage.setItem("svsm-builder", "{not json");
  vi.resetModules();
  expect((await import("./builderStore")).getBuilder().entries).toEqual([]);
  act(() => resetBuilderForTests());
});

test("Suggested for you is based on the builder's mods when it has some, else the installed mods", async () => {
  const lastSeeds = () => (mocked.browseMods.mock.calls.at(-1)![0] as { seedModIds: string[] }).seedModIds;
  mocked.browseMods.mockImplementation((q: { sort: string }) =>
    ok(q.sort !== "suggested" ? page : {
      ...page,
      items: [{ ...browseEntry("primitivesurvival", "Primitive Survival", 9000), suggestion: { score: 1, together: 7 } }],
      suggestBasis: { fromBuilder: false, seedCount: 12, modlists: 40, reporters: 9, error: null },
    }));
  renderBrowser();
  fireEvent.change(await screen.findByRole("combobox", { name: "Sort by" }), { target: { value: "suggested" } });
  expect(await screen.findByText(/Suggested from your 12 installed mods, using 40 community modlists and 9 people's mod reports/)).toBeInTheDocument();
  expect(lastSeeds()).toEqual([]);
  expect(screen.getByRole("listitem", { name: /Primitive Survival/ })).toHaveTextContent("7");

  // An open builder with mods: those are the seeds.
  act(() => {
    builder.setOpen(true);
    builder.add([{ modId: "CarryOn", version: "1.0.0", compatible: true, name: "Carry On", author: null, side: null, logoUrl: null, downloads: null }]);
  });
  await waitFor(() => expect(lastSeeds()).toEqual(["carryon"]));
  // Closing it goes back to the installed mods.
  act(() => builder.setOpen(false));
  await waitFor(() => expect(lastSeeds()).toEqual([]));
});
