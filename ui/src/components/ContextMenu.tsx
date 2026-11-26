import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { toast } from "./Toast";

/** `checked` makes a checkbox item; `keepOpen` leaves the menu open after it (for toggling several). */
type Action = { label: string; run: () => void | Promise<void>; disabled?: boolean; danger?: boolean; checked?: boolean; keepOpen?: boolean };
type Groups = (Action | false | undefined)[][];
type OpenMenu = (event: ReactMouseEvent<HTMLElement>, label: string, groups: Groups) => void;
type Menu = {
  x: number;
  y: number;
  label: string;
  groups: Action[][];
  trigger: HTMLElement | null;
};

const ContextMenu = createContext<OpenMenu>(() => {});
export const useContextMenu = () => useContext(ContextMenu);
export const copyText = (text: string) => navigator.clipboard.writeText(text);

const textControl = 'textarea, input:not([type]), input:is([type="text"], [type="search"], [type="url"], [type="tel"], [type="password"])';
const focusable = "button, input, a[href], [tabindex]";

export function ContextMenuProvider({ children }: { children: ReactNode }) {
  const [menu, setMenu] = useState<Menu | null>(null);
  const popup = useRef<HTMLDivElement>(null);

  const close = useCallback((restore: boolean) => {
    if (restore && menu?.trigger?.isConnected) menu.trigger.focus({ preventScroll: true });
    setMenu(null);
  }, [menu]);

  const open = useCallback<OpenMenu>((event, label, groups) => {
    event.preventDefault();
    event.stopPropagation();
    const trigger = event.currentTarget.matches(focusable)
      ? event.currentTarget
      : event.currentTarget.querySelector<HTMLElement>("button:enabled") ?? event.currentTarget.querySelector<HTMLElement>(focusable);
    setMenu({
      x: event.clientX, y: event.clientY, label, trigger,
      groups: groups.map((group) => group.filter((item): item is Action => !!item)).filter((group) => group.length > 0),
    });
  }, []);

  useEffect(() => {
    const openTextMenu = (event: MouseEvent) => {
      if (!(event.target instanceof HTMLElement) || popup.current?.contains(event.target)) return;
      const control = event.target.closest<HTMLInputElement | HTMLTextAreaElement>(textControl);
      if (control && !control.disabled) {
        const selected = control.selectionStart !== control.selectionEnd && control.type !== "password";
        const editable = !control.readOnly;
        setMenu({
          x: event.clientX, y: event.clientY, label: "Text editing", trigger: control,
          groups: [[
            { label: "Cut", disabled: !editable || !selected, run: () => { document.execCommand("cut"); } },
            { label: "Copy", disabled: !selected, run: () => { document.execCommand("copy"); } },
            {
              label: "Paste", disabled: !editable,
              run: async () => {
                const text = await navigator.clipboard.readText();
                if (document.activeElement === control) document.execCommand("insertText", false, text);
              },
            },
          ], [{ label: "Select all", disabled: !control.value, run: () => { control.select(); } }]],
        });
        return;
      }
      const selection = window.getSelection()?.toString();
      if (selection) {
        setMenu({
          x: event.clientX, y: event.clientY, label: "Selected text", trigger: null,
          groups: [[{ label: "Copy", run: () => copyText(selection) }]],
        });
      } else {
        setMenu(null);
      }
    };
    const keyboardMenu = (event: KeyboardEvent) => {
      if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
      const target = document.activeElement;
      if (!(target instanceof HTMLElement) || popup.current?.contains(target)) return;
      event.preventDefault();
      const rect = target.getBoundingClientRect();
      target.dispatchEvent(new MouseEvent("contextmenu", {
        bubbles: true, cancelable: true, button: 2,
        clientX: Math.round(rect.left + 8), clientY: Math.round(rect.bottom),
      }));
    };
    document.addEventListener("contextmenu", openTextMenu);
    document.addEventListener("keydown", keyboardMenu, true);
    return () => {
      document.removeEventListener("contextmenu", openTextMenu);
      document.removeEventListener("keydown", keyboardMenu, true);
    };
  }, []);

  useLayoutEffect(() => {
    const node = popup.current;
    if (!menu || !node) return;
    const { offsetWidth: width, offsetHeight: height } = node;
    const edge = 8;
    const fit = (start: number, size: number, limit: number) =>
      Math.max(edge, Math.min(start + size <= limit - edge ? start : start - size, limit - size - edge));
    node.style.left = `${fit(menu.x, width, window.innerWidth)}px`;
    node.style.top = `${fit(menu.y, height, window.innerHeight)}px`;
    (node.querySelector<HTMLElement>("button:enabled") ?? node).focus({ preventScroll: true });
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const dismissOutside = (event: Event) => {
      if (event.target instanceof Node && popup.current?.contains(event.target)) return;
      close(false);
    };
    const dismiss = () => close(false);
    document.addEventListener("pointerdown", dismissOutside, true);
    window.addEventListener("scroll", dismissOutside, true);
    window.addEventListener("resize", dismiss);
    window.addEventListener("blur", dismiss);
    return () => {
      document.removeEventListener("pointerdown", dismissOutside, true);
      window.removeEventListener("scroll", dismissOutside, true);
      window.removeEventListener("resize", dismiss);
      window.removeEventListener("blur", dismiss);
    };
  }, [menu, close]);

  const activate = async (action: Action) => {
    if (action.keepOpen) {
      // Flip the shown state right away; the real state comes from `run`.
      setMenu((m) => m && {
        ...m,
        groups: m.groups.map((g) => g.map((a) => (a === action && a.checked !== undefined ? { ...a, checked: !a.checked } : a))),
      });
    } else {
      close(true);
    }
    try {
      await action.run();
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : "Could not complete this action.");
    }
  };

  return (
    <ContextMenu.Provider value={open}>
      {children}
      {menu && createPortal(
        <div className="context-menu" role="menu" aria-label={menu.label} ref={popup} tabIndex={-1}
          onContextMenu={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === "Tab") { close(true); return; }
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              close(true);
              return;
            }
            if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
            event.preventDefault();
            const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:enabled")];
            if (items.length === 0) return;
            const focused = items.indexOf(document.activeElement as HTMLButtonElement);
            const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
              : (focused + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
            items[next].focus();
          }}
        >
          {menu.groups.map((group, index) => (
            <div role="group" className="context-menu-group" key={index}>
              {group.map((action) => (
                <button key={action.label} type="button" role={action.checked === undefined ? "menuitem" : "menuitemcheckbox"} tabIndex={-1}
                  aria-checked={action.checked}
                  className={[action.danger && "danger", action.checked !== undefined && "menu-check"].filter(Boolean).join(" ") || undefined}
                  disabled={action.disabled} onClick={() => void activate(action)}>
                  {action.checked !== undefined && <span className="menu-check-mark" aria-hidden>{action.checked ? "✓" : ""}</span>}
                  {action.label}
                </button>
              ))}
            </div>
          ))}
        </div>, document.body)}
    </ContextMenu.Provider>
  );
}
