import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ModlistSummary } from "../../bindings";
import { ModlistPicker } from "./ModlistPicker";

vi.mock("../../bindings", () => ({ commands: { myCloudSlots: vi.fn() } }));

const list = (name: string, over: Partial<ModlistSummary> = {}): ModlistSummary => ({
  id: `Modlists/${name}.json`,
  kind: "modlist",
  fileName: `${name}.json`,
  name,
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
  active: false,
  downloadsNeeded: 0,
  group: null,
  subscription: null,
  published: null,
  ...over,
} as ModlistSummary);

function renderPicker(lists: ModlistSummary[], onPick = vi.fn()) {
  const client = new QueryClient();
  render(
    <QueryClientProvider client={client}>
      <ModlistPicker lists={lists} onPick={onPick} onClose={() => {}} />
    </QueryClientProvider>,
  );
  return onPick;
}

const titles = () =>
  within(screen.getByRole("list", { name: "Modlists" })).getAllByRole("button").map((b) => b.getAttribute("title"));

beforeEach(() => localStorage.clear());

it("picks a card and shows the last picks first next time", () => {
  const lists = [list("Beta"), list("Alpha"), list("Zed", { kind: "group" })];
  const onPick = renderPicker(lists);
  expect(titles()).toEqual(["Alpha", "Beta", "Zed"]);
  fireEvent.click(screen.getByTitle("Zed"));
  expect(onPick).toHaveBeenCalledWith(lists[2]);
  document.body.innerHTML = "";
  renderPicker(lists);
  expect(titles()).toEqual(["Zed", "Alpha", "Beta"]);
});
