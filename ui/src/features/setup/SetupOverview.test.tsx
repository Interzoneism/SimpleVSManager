import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import type { DetectedEnvironment } from "../../bindings";
import { EnvironmentDetails } from "./SetupOverview";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), ask: vi.fn() }));

function renderWithQuery(ui: React.ReactElement) {
  return render(<QueryClientProvider client={new QueryClient()}>{ui}</QueryClientProvider>);
}

const env: DetectedEnvironment = {
  os: "linux",
  settings: {
    schemaVersion: 1,
    gameDirectory: null,
    dataDirectory: "/chosen",
    closeAfterLaunch: true,
    offlineMode: false,
    targetGameVersion: null,
    requireExactVersionMatch: false,
    modCacheLimitMb: 2048,
    skippedModVersions: {},
    updateExclusions: [],
    favouriteMods: [],
    legacyPreferencesImported: true,
    hiddenOnlineModlists: [],
    hiddenOnlineAuthors: [],
    modUsageTracking: true,
    modUsageScanned: false,
    modUsage: { counted: [], longSessions: 0, mods: {} },
    autoDataBackups: false,
    maxDataBackups: 0,
    dataBackupLocation: null,
    theme: "darkVs",
    customPalette: {},
    profiles: [],
    activeProfile: "Default",
    verboseLogging: false,
    legacyM5Imported: true,
    customShortcutPath: null,
    legacyShortcutImported: true,
  },
  managerDir: { path: "/home/me/.local/share/simple-vs-manager", source: "default", exists: false },
  dataDir: { path: "/chosen", source: "userSetting", exists: true },
  gameDir: null,
  gameExecutable: null,
  gameVersion: "1.21.5",
  legacyProfile: null,
  warnings: ["The data folder from your earlier Simple VS Manager settings no longer exists: /gone"],
};

test("shows detected folders, version, warnings and settings", () => {
  renderWithQuery(<EnvironmentDetails env={env} />);
  expect(screen.getByText("1.21.5")).toBeInTheDocument();
  expect(screen.getByText("/chosen")).toBeInTheDocument();
  expect(screen.getByText("Chosen by you")).toBeInTheDocument();
  expect(screen.getByText(/Vintage Story wasn't found/)).toBeInTheDocument();
  expect(screen.getByRole("list", { name: "Warnings" })).toHaveTextContent("no longer exists");
  // Only the data folder was set by hand, so only it offers "Auto-detect".
  expect(screen.getAllByRole("button", { name: "Auto-detect" })).toHaveLength(1);
  expect(screen.getAllByRole("button", { name: "Change…" })).toHaveLength(2);
  // The manager folder can be moved; it's in the default place, so there's nothing to move back.
  expect(screen.getByRole("button", { name: "Change folder…" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Move back to default" })).not.toBeInTheDocument();
  // No custom shortcut yet.
  expect(screen.getByText("Off")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Choose…" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: /Close the manager/ })).toBeChecked();
});
