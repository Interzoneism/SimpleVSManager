import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import type { ApplyReport, ModlistPreview } from "../../bindings";
import { commands } from "../../bindings";
import { LoadModlistDialog } from "./LoadModlistDialog";
import { quickSwitchNotice } from "./quickSwitch";

vi.mock("../../bindings", () => ({
  commands: { previewModlist: vi.fn(), applyModlist: vi.fn(), cancelUpdates: vi.fn() },
  events: {
    modlistProgress: { listen: async () => () => {} },
    updateProgress: { listen: async () => () => {} },
  },
}));

// A list with one mod that isn't in the library and one loose file that can't change version.
const preview: ModlistPreview = {
  name: "Winter",
  kind: "modlist",
  modCount: 3,
  hasVersions: true,
  hasStatus: true,
  plan: {
    install: [{ modId: "gone", version: "1.0.0" }],
    changeVersion: [],
    remove: [{ key: "a", modId: "a", name: "A" }],
    states: [],
    disabledEntries: null,
    skipped: ["Loose (wants 2.0, can't change a loose file)"],
    toggles: 0,
  },
  fromLibrary: 0,
  toDownload: 1,
  configFiles: [],
  conflicts: [],
};

const report: ApplyReport = {
  installed: 0,
  changed: 0,
  removed: 1,
  toLibrary: 1,
  toggled: 0,
  configsWritten: 0,
  failures: ["gone 1.0.0: not found on the Mod DB"],
  skipped: preview.plan.skipped,
  cancelled: false,
  backupId: "b",
  list: null,
};

test("Quick switch applies a list even when some mods can't be loaded, without the review", async () => {
  vi.mocked(commands.previewModlist).mockResolvedValue({ status: "ok", data: preview });
  vi.mocked(commands.applyModlist).mockResolvedValue({ status: "ok", data: report });
  const onDone = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <LoadModlistDialog
        os="windows"
        source={{ id: "Modlists/Winter.json", file: null, online: null }}
        kind="modlist"
        title="Winter"
        quickSwitch={{ start: true, preference: "keep-configs", onRemember: vi.fn(), onDone }}
        onShowBackup={vi.fn()}
        onClose={vi.fn()}
      />
    </QueryClientProvider>,
  );
  await waitFor(() => expect(onDone).toHaveBeenCalledWith(report));
  expect(commands.applyModlist).toHaveBeenCalledWith(expect.anything(), "replace", false);
  expect(screen.queryByRole("button", { name: "Switch" })).toBeNull();
});

test("the Quick switch notice names what was left out", () => {
  const notice = quickSwitchNotice("Winter", report, "windows");
  expect(notice.problems).toBe(true);
  expect(notice.text).toContain("Couldn't load: gone 1.0.0");
  expect(notice.text).toContain("Left as they are: Loose");
  expect(quickSwitchNotice("Winter", { ...report, failures: [], skipped: [] }, "windows")).toEqual({ text: 'Switched to "Winter".', problems: false });
  expect(quickSwitchNotice("Winter", null, "windows").problems).toBe(false);
});
