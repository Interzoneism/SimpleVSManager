import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import type { CloudStatus, ModlistSummary, OnlineEntry, OnlineListing } from "../../bindings";
import { commands } from "../../bindings";
import { Modlists } from "./Modlists";
import { Toaster } from "../../components/Toast";
import { ContextMenuProvider } from "../../components/ContextMenu";

vi.mock("../../bindings", () => ({
  commands: { listModlists: vi.fn(), cloudStatus: vi.fn(), onlineModlists: vi.fn(), publishSavedModlist: vi.fn() },
  events: { modsChanged: { listen: async () => () => {} } },
}));

const local: ModlistSummary = {
  id: "Modlists/Winter.json",
  fileName: "Winter.json",
  kind: "modlist",
  name: "Winter save",
  description: null,
  icon: null,
  iconColor: null,
  version: null,
  gameVersion: null,
  uploader: null,
  modifiedMs: null,
  mods: [],
  configCount: 0,
  hasVersions: true,
  error: null,
  group: null,
  active: false,
  downloadsNeeded: 0,
  subscription: null,
  published: null,
};

function online(name: string, mine: boolean): OnlineEntry {
  return {
    entry: {
      registryId: name,
      name,
      description: null,
      icon: null,
      iconColor: null,
      version: null,
      uploader: "Alice",
      gameVersion: null,
      mods: [],
      revision: 1,
      createdMs: null,
      updatedMs: null,
      complete: true,
    },
    subscribers: 0,
    subscribed: !mine,
    mine,
    localId: mine ? null : local.id,
  };
}

function renderModlists(offline = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <ContextMenuProvider>
        <Modlists os="windows" offline={offline} droppedFile={null} onDroppedFileHandled={() => {}} />
      </ContextMenuProvider>
      <Toaster />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("ResizeObserver", class {
    observe() {}
    disconnect() {}
  });
  vi.mocked(commands.listModlists).mockResolvedValue({
    status: "ok",
    data: [
      local,
      { ...local, id: "Modlists/Zed.json", name: "Zed published", published: { registryId: "My published list", revision: 2, changed: false } },
      { ...local, id: "Modlists/Cozy.svsmgroup", name: "Cozy group", kind: "group", group: { members: [], conflicts: [] } },
    ],
  });
  const status: CloudStatus = {
    offline: false, hasIdentity: false, playerName: "Tester", playerProblem: null, legacyIdentity: false, signInProblem: null,
  };
  const listing: OnlineListing = {
    entries: [online("My published list", true), online("Community winter", false)],
    hiddenModlists: 0,
    hiddenAuthors: 0,
    error: null,
  };
  vi.mocked(commands.cloudStatus).mockResolvedValue({ status: "ok", data: status });
  vi.mocked(commands.onlineModlists).mockResolvedValue({ status: "ok", data: listing });
});

afterEach(() => vi.unstubAllGlobals());

test("searches saved and online cards together and opens your copy in the shared details pane", async () => {
  renderModlists();
  const saved = await screen.findByRole("list", { name: "Saved modlists" });
  expect(screen.queryByRole("list", { name: "Your published modlists" })).toBeNull();
  expect(within(saved).getByRole("button", { name: /Cozy group/ })).toBeVisible();
  // Your published modlist is a saved modlist, listed first and marked; it isn't listed again online.
  const cards = within(saved).getAllByRole("button", { pressed: false });
  expect(cards[0]).toHaveAccessibleName(/Zed published/);
  expect(cards[0]).toHaveClass("published");
  expect(within(cards[0]).getByLabelText("Published")).toBeInTheDocument();
  const community = await screen.findByRole("list", { name: "From the community" });
  expect(await within(community).findByRole("button", { name: /Community winter/ })).toBeVisible();
  expect(within(community).queryByRole("button", { name: /My published list/ })).toBeNull();

  const search = screen.getByRole("searchbox", { name: "Search modlists" });
  fireEvent.change(search, { target: { value: "winter" } });
  expect(screen.queryByRole("button", { name: /Cozy group|Zed published/ })).toBeNull();
  const onlineCard = within(community).getByRole("button", { name: /Community winter/ });
  fireEvent.click(onlineCard);
  const details = within(screen.getByRole("complementary", { name: "Modlist details" }));
  expect(details.getByRole("heading", { name: "Community winter" })).toBeVisible();

  fireEvent.click(details.getByRole("button", { name: "Show my copy" }));
  expect(details.getByRole("heading", { name: /Winter save/ })).toBeVisible();
  expect(within(saved).getByRole("button", { name: /Winter save/ })).toHaveAttribute("aria-pressed", "true");
  expect(onlineCard).toHaveAttribute("aria-pressed", "false");
});

test("keeps saved modlists available in offline mode", async () => {
  renderModlists(true);
  expect(await screen.findByRole("list", { name: "Saved modlists" })).toBeVisible();
  expect(screen.getByText("Offline mode is on.")).toBeVisible();
  expect(commands.cloudStatus).not.toHaveBeenCalled();
  expect(commands.onlineModlists).not.toHaveBeenCalled();
});

test("collapsed online sections don't load the online listing, and the sort sits under From the community", async () => {
  const stored = new Map<string, string>([
    ["svsm-layout:modlists-community-collapsed", "true"],
  ]);
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => stored.get(k) ?? null,
    setItem: (k: string, v: string) => void stored.set(k, v),
    removeItem: (k: string) => void stored.delete(k),
  });
  renderModlists();
  expect(await screen.findByRole("list", { name: "Saved modlists" })).toBeVisible();
  expect(screen.queryByRole("combobox", { name: "Sort online modlists" })).toBeNull();
  expect(commands.onlineModlists).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("button", { name: /From the community/ }));
  expect(await screen.findByRole("list", { name: "From the community" })).toBeVisible();
  expect(screen.getByRole("combobox", { name: "Sort online modlists" })).toBeVisible();
  expect(screen.getByRole("button", { name: /Refresh online modlists/ })).toBeVisible();
  expect(commands.onlineModlists).toHaveBeenCalled();
});

test("publishes a saved modlist from its details, and offers Unpublish for a published one", async () => {
  vi.mocked(commands.publishSavedModlist).mockResolvedValue({
    status: "ok",
    data: { ...local, published: { registryId: "new", revision: 1, changed: false } },
  });
  renderModlists();
  const saved = await screen.findByRole("list", { name: "Saved modlists" });
  fireEvent.click(within(saved).getByRole("button", { name: /Winter save/ }));
  const details = within(screen.getByRole("complementary", { name: "Modlist details" }));
  const publish = await details.findByRole("button", { name: "Publish" });
  await vi.waitFor(() => expect(publish).toBeEnabled());
  fireEvent.click(publish);
  await vi.waitFor(() => expect(commands.publishSavedModlist).toHaveBeenCalledWith("Modlists/Winter.json"));
  expect(await screen.findByText(/Published "Winter save"/)).toBeVisible();

  fireEvent.click(within(saved).getByRole("button", { name: /Zed published/ }));
  expect(details.getByText("Published", { selector: ".badge" })).toBeVisible();
  expect(details.getByRole("button", { name: "Unpublish…" })).toBeVisible();
  expect(details.queryByRole("button", { name: "Publish" })).toBeNull();
});

test("details offer one Export menu, no Update button, and hide server commands unless turned on in Settings", async () => {
  const withVersions: ModlistSummary = { ...local, mods: [{ modId: "carryon", version: "1.0.0", isActive: true }] };
  vi.mocked(commands.listModlists).mockResolvedValue({ status: "ok", data: [withVersions] });
  const stored = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => stored.get(k) ?? null,
    setItem: (k: string, v: string) => void stored.set(k, v),
    removeItem: (k: string) => void stored.delete(k),
  });
  const view = renderModlists();
  const saved = await screen.findByRole("list", { name: "Saved modlists" });
  fireEvent.click(within(saved).getByRole("button", { name: /Winter save/ }));
  const details = within(screen.getByRole("complementary", { name: "Modlist details" }));
  expect(details.queryByRole("button", { name: "Update" })).toBeNull();
  expect(details.queryByRole("button", { name: /Export (PDF|JSON)/ })).toBeNull();
  expect(details.queryByRole("button", { name: "Copy server commands" })).toBeNull();

  fireEvent.click(details.getByRole("button", { name: "Export…" }));
  const menu = within(screen.getByRole("menu", { name: "Export as" }));
  expect(menu.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["PDF file…", "JSON file…"]);

  view.unmount();
  stored.set("svsm-layout:modlist-show-server-commands", "true");
  renderModlists();
  const again = await screen.findByRole("list", { name: "Saved modlists" });
  fireEvent.click(within(again).getByRole("button", { name: /Winter save/ }));
  expect(within(screen.getByRole("complementary", { name: "Modlist details" })).getByRole("button", { name: "Copy server commands" })).toBeVisible();
});
