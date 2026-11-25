import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import type { InstalledMod, ModList } from "../bindings";

let resolveToggle: (r: unknown) => void = () => {};
vi.mock("../bindings", () => ({
  commands: { setModsActive: () => new Promise((r) => (resolveToggle = r)) },
  events: {},
}));

const { queryKeys, useSetModsActive } = await import("./index");

const mod = (key: string, isActive: boolean) => ({ key, isActive, error: null }) as unknown as InstalledMod;
const list = (a: boolean): ModList => ({ mods: [mod("a", a), mod("b", true)], searchPaths: [], dataDir: "/d", clientSettingsFound: true });

function setup() {
  const client = new QueryClient();
  client.setQueryData(queryKeys.mods, list(true));
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const { result } = renderHook(() => useSetModsActive(), { wrapper });
  const active = (key: string) => client.getQueryData<ModList>(queryKeys.mods)!.mods.find((m) => m.key === key)!.isActive;
  return { result, active, client };
}

test("toggling shows the new state at once and keeps other rows as they were", async () => {
  const { result, active, client } = setup();
  const before = client.getQueryData<ModList>(queryKeys.mods)!.mods[1];
  act(() => result.current.mutate({ keys: ["a"], active: false }));
  await waitFor(() => expect(active("a")).toBe(false));
  expect(client.getQueryData<ModList>(queryKeys.mods)!.mods[1]).toBe(before);
  act(() => resolveToggle({ status: "ok", data: list(false) }));
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(active("a")).toBe(false);
});

test("a failed toggle puts the old state back", async () => {
  const { result, active } = setup();
  act(() => result.current.mutate({ keys: ["a"], active: false }));
  await waitFor(() => expect(active("a")).toBe(false));
  act(() => resolveToggle({ status: "error", error: { message: "locked" } }));
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(active("a")).toBe(true);
});
