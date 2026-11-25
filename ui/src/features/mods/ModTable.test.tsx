import { fireEvent, render, screen } from "@testing-library/react";
import { ContextMenuProvider } from "../../components/ContextMenu";
import { ModTable } from "./ModTable";

function renderTable() {
  return render(
    <ContextMenuProvider>
      <ModTable
        os="windows" mods={[]} db={new Map()} progress={new Map()} onUpdate={() => {}}
        selected={new Set()} onSelectionChange={() => {}} sortKey="name" sortDirection="asc" onSort={() => {}}
        onToggleActive={() => {}} onRowMenu={() => {}} busy={false} empty={<p>No mods</p>} rowSize="medium"
      />
    </ContextMenuProvider>,
  );
}

// In-memory storage, so the remembered layout can be checked.
const store = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v),
  removeItem: (k: string) => void store.delete(k),
  clear: () => store.clear(),
});
beforeEach(() => store.clear());

test("right-click the header to hide and show columns, remembered", () => {
  const { unmount } = renderTable();
  expect(screen.getByRole("columnheader", { name: /Tags/ })).toBeInTheDocument();
  fireEvent.contextMenu(screen.getByRole("columnheader", { name: /Tags/ }));
  const tags = screen.getByRole("menuitemcheckbox", { name: /Tags/ });
  expect(tags).toHaveAttribute("aria-checked", "true");
  expect(screen.queryByRole("menuitemcheckbox", { name: /Name/ })).not.toBeInTheDocument();
  fireEvent.click(tags);
  // The menu stays open so several columns can be toggled.
  expect(screen.getByRole("menuitemcheckbox", { name: /Tags/ })).toHaveAttribute("aria-checked", "false");
  expect(screen.queryByRole("columnheader", { name: /Tags/ })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("menuitemcheckbox", { name: /Side/ }));
  expect(screen.queryByRole("columnheader", { name: /Side/ })).not.toBeInTheDocument();
  unmount();

  renderTable();
  expect(screen.queryByRole("columnheader", { name: /Tags/ })).not.toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: /Version/ })).toBeInTheDocument();
  fireEvent.contextMenu(screen.getByRole("columnheader", { name: /Version/ }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Reset columns" }));
  expect(screen.getByRole("columnheader", { name: /Tags/ })).toBeInTheDocument();
});

test("dragging a column edge resizes it and double-click resets it", () => {
  const { container } = renderTable();
  const table = container.querySelector<HTMLElement>(".mod-table")!;
  const handle = screen.getByRole("separator", { name: "Resize Authors column" });
  fireEvent.pointerDown(handle, { clientX: 100, pointerId: 1 });
  fireEvent.pointerMove(handle, { clientX: 160, pointerId: 1 });
  fireEvent.pointerUp(handle, { clientX: 160, pointerId: 1 });
  expect(table.style.getPropertyValue("--table-columns")).toContain(" 190px ");
  expect(JSON.parse(JSON.parse(localStorage.getItem("svsm-layout:installed-columns")!)).widths).toEqual({ authors: 190 });
  fireEvent.doubleClick(handle);
  expect(table.style.getPropertyValue("--table-columns")).toContain(" 130px ");
});
