import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ModlistModCard } from "../../bindings";
import { commands } from "../../bindings";
import { ContextMenuProvider } from "../../components/ContextMenu";
import { ModCards } from "./ModCards";
import { libraryStatus, sortRows } from "./modlistTable";

vi.mock("../../bindings", () => ({
  commands: { modlistModCards: vi.fn(), openUrl: vi.fn() },
  events: { modsChanged: { listen: async () => () => {} } },
}));

const card = (modId: string, name: string, installedVersion: string | null, libraryVersions: string[] = []): ModlistModCard => ({
  modId, name, author: "Tyron", side: "both", logoUrl: null, pageUrl: `https://mods.vintagestory.at/${modId}`, downloads: 1,
  installedVersion, libraryVersions, notOnModDb: false,
});

const cards = [card("carryon", "Carry On", "1.8.0"), card("butchering", "Butchering", null, ["1.2.0"]), card("ancienttools", "Ancient Tools", "0.9.0")];

beforeEach(() => {
  vi.mocked(commands.modlistModCards).mockResolvedValue({ status: "ok", data: cards });
});

test("library status and sorting", () => {
  expect(libraryStatus("1.8.0", cards[0])?.label).toBe("Installed");
  expect(libraryStatus("1.2.0", cards[1])?.label).toBe("In library");
  expect(libraryStatus("1.0.0", cards[2])).toMatchObject({ label: "Other version", detail: "0.9.0 is installed; loading the list switches it" });
  expect(libraryStatus("1.0.0", card("x", "X", null))?.label).toBe("Not downloaded");
  expect(libraryStatus("1.0.0", undefined)).toBeNull();

  const rows = [{ modId: "b", version: "1.10.0", name: "Beta" }, { modId: "a", version: "1.9.0", name: "alpha" }];
  expect(sortRows(rows, null, "asc").map((r) => r.modId)).toEqual(["b", "a"]);
  expect(sortRows(rows, "name", "asc").map((r) => r.modId)).toEqual(["a", "b"]);
  expect(sortRows(rows, "name", "desc").map((r) => r.modId)).toEqual(["b", "a"]);
  expect(sortRows(rows, "version", "asc").map((r) => r.modId)).toEqual(["a", "b"]);
});

test("the modlist details show the mods as a sortable table with a right-click menu", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ContextMenuProvider>
        <ModCards mods={[
          { modId: "carryon", version: "1.8.0", isActive: true },
          { modId: "butchering", version: "1.2.0", isActive: false },
          { modId: "ancienttools", version: "1.0.0", isActive: true },
        ]} />
      </ContextMenuProvider>
    </QueryClientProvider>,
  );
  const table = screen.getByRole("grid", { name: "Mods in this modlist" });
  await waitFor(() => expect(within(table).getByText("Carry On")).toBeInTheDocument());
  expect(within(table).getByText("In library")).toBeInTheDocument();
  expect(within(table).getByText("Other version")).toBeInTheDocument();
  // Switched-off mods are greyed out.
  expect(within(table).getByText("Butchering").closest('[role="row"]')).toHaveClass("status-disabled");

  const names = () => within(table).getAllByRole("row").slice(1).map((r) => r.querySelector(".mod-title")?.textContent);
  expect(names()).toEqual(["Carry On", "Butchering", "Ancient Tools"]);
  fireEvent.click(within(table).getByRole("button", { name: "Name" }));
  expect(names()).toEqual(["Ancient Tools", "Butchering", "Carry On"]);
  fireEvent.click(within(table).getByRole("button", { name: /Name/ }));
  expect(names()).toEqual(["Carry On", "Butchering", "Ancient Tools"]);

  // Row sizes, like on Installed mods.
  fireEvent.click(screen.getByRole("button", { name: "L" }));
  expect(table).toHaveClass("rows-large");

  fireEvent.contextMenu(within(table).getByText("Carry On"));
  expect(await screen.findByRole("menuitem", { name: "Open Mod DB page" })).toBeInTheDocument();
  expect(screen.getByRole("menuitem", { name: "Copy mod ID" })).toBeInTheDocument();
});
