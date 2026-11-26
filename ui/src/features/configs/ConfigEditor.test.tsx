import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Activity, type ReactNode } from "react";
import { commands } from "../../bindings";
import { ConfigEditor } from "./ConfigEditor";

vi.mock("../../bindings", () => ({
  commands: { listModConfigs: vi.fn(), readModConfig: vi.fn(), saveModConfig: vi.fn() },
}));

const client = () => new QueryClient({ defaultOptions: { queries: { retry: false } } });

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("ResizeObserver", class {
    observe() {}
    disconnect() {}
  });
  vi.mocked(commands.listModConfigs).mockResolvedValue({
    status: "ok",
    data: [
      { relativePath: "alpha.json", modId: "alpha", modName: "Alpha", sizeBytes: 10 },
      { relativePath: "beta.json", modId: "beta", modName: "Beta", sizeBytes: 10 },
    ],
  });
  vi.mocked(commands.readModConfig).mockImplementation(async (relativePath) => ({
    status: "ok",
    data: { document: { relativePath, format: "json", text: '{ "a": 1 }', tree: null, parseError: null }, previous: null },
  }));
});

function setup() {
  const qc = client();
  const page = (focusMod: { modId: string } | null, visible = true): ReactNode => (
    <QueryClientProvider client={qc}>
      <Activity mode={visible ? "visible" : "hidden"}>
        <ConfigEditor enabled focusMod={focusMod} />
      </Activity>
    </QueryClientProvider>
  );
  return { page, ...render(page(null)) };
}

const selected = () => screen.getAllByRole("option").find((o) => o.getAttribute("aria-selected") === "true")?.textContent;

it("opens the requested mod's config each time Edit config asks, also for the same mod again", async () => {
  const { page, rerender } = setup();
  await screen.findByText("alpha.json");
  rerender(page({ modId: "beta" }));
  await waitFor(() => expect(selected()).toContain("beta.json"));

  // The user moves on, then asks for the same mod again: a new request object.
  fireEvent.change(screen.getByLabelText("Filter config files"), { target: { value: "" } });
  fireEvent.click(screen.getByText("alpha.json"));
  expect(selected()).toContain("alpha.json");
  rerender(page({ modId: "beta" }));
  await waitFor(() => expect(selected()).toContain("beta.json"));
});

it("keeps the user's own filter when the page is hidden and shown again", async () => {
  const { page, rerender } = setup();
  const request = { modId: "beta" };
  rerender(page(request));
  await waitFor(() => expect(selected()).toContain("beta.json"));
  fireEvent.change(screen.getByLabelText("Filter config files"), { target: { value: "alp" } });

  rerender(page(request, false));
  rerender(page(request, true));
  expect(screen.getByLabelText<HTMLInputElement>("Filter config files").value).toBe("alp");
});

it("keeps unsaved edits when the page is hidden and shown again", async () => {
  const { page, rerender } = setup();
  fireEvent.click(await screen.findByText("alpha.json"));
  const box = await screen.findByLabelText<HTMLTextAreaElement>("File contents");
  fireEvent.change(box, { target: { value: '{ "a": 2 }' } });

  rerender(page(null, false));
  rerender(page(null, true));
  await waitFor(() => expect(commands.readModConfig).toHaveBeenCalledTimes(2));
  expect(screen.getByLabelText<HTMLTextAreaElement>("File contents").value).toBe('{ "a": 2 }');
});

const BED_SPAWN = '{"BlacklistedBeds":[],"RequireSneaking":false,"Rooms":{"Enabled":false,"BedsThatDontRequireRooms":[]},"10":true,"Cooldown":{"Enabled":false,"CooldownDays":1.0}}';

async function openFields() {
  vi.mocked(commands.readModConfig).mockImplementation(async (relativePath) => ({
    status: "ok",
    data: { document: { relativePath, format: "json", text: BED_SPAWN, tree: BED_SPAWN, parseError: null }, previous: null },
  }));
  vi.mocked(commands.saveModConfig).mockImplementation(async (relativePath, text) => ({
    status: "ok",
    data: { document: { relativePath, format: "json", text, tree: text, parseError: null }, previous: null },
  }));
  setup();
  fireEvent.click(await screen.findByText("alpha.json"));
  return screen.findByRole("switch", { name: "Require sneaking" });
}

it("shows settings with readable labels, switches and steppers, and marks changes", async () => {
  const sneaking = await openFields();
  expect(screen.getByText("RequireSneaking")).toBeInTheDocument(); // the raw key as a hint
  expect(screen.getByRole("button", { name: /Beds that dont require rooms/ })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

  fireEvent.click(sneaking);
  expect(sneaking).toHaveAttribute("aria-checked", "true");
  fireEvent.click(screen.getAllByRole("button", { name: "Increase" })[0]);
  expect(screen.getByLabelText<HTMLInputElement>("Cooldown days").value).toBe("2.0");
  expect(screen.getByText("2 unsaved changes")).toBeInTheDocument();

  // Put one back from its row.
  fireEvent.click(screen.getByRole("button", { name: "Put back Require sneaking to false" }));
  expect(screen.getByText("1 unsaved change")).toBeInTheDocument();
});

it("adds and removes list items and saves in the file's own key order", async () => {
  await openFields();
  fireEvent.click(screen.getAllByRole("button", { name: "Add item" })[0]);
  fireEvent.change(screen.getByLabelText("Item 1"), { target: { value: "bed-wood" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(commands.saveModConfig).toHaveBeenCalled());
  const saved = vi.mocked(commands.saveModConfig).mock.calls[0][1];
  expect(saved.indexOf('"RequireSneaking"')).toBeLessThan(saved.indexOf('"10"'));
  expect(saved).toContain('"CooldownDays": 1.0');
  expect(saved).toContain('"BlacklistedBeds": [\n    "bed-wood"\n  ]');

  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeDisabled());
  fireEvent.click(screen.getByRole("button", { name: "Remove item 1" }));
  expect(screen.queryByLabelText("Item 1")).toBeNull();
});

it("filters settings and highlights what matched", async () => {
  await openFields();
  fireEvent.change(screen.getByLabelText("Find a setting"), { target: { value: "cooldown" } });
  expect(screen.queryByRole("switch", { name: "Require sneaking" })).toBeNull();
  expect(screen.getByLabelText("Cooldown days")).toBeInTheDocument();
  expect(document.querySelectorAll("mark.cfg-hit").length).toBeGreaterThan(0);
});
