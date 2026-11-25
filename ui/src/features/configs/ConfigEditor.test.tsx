import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Activity, type ReactNode } from "react";
import { commands } from "../../bindings";
import { ConfigEditor } from "./ConfigEditor";

vi.mock("../../bindings", () => ({
  commands: { listModConfigs: vi.fn(), readModConfig: vi.fn() },
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
