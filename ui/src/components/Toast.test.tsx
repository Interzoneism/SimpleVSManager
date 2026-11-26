import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { toast, Toaster } from "./Toast";

afterEach(() => vi.useRealTimers());

it("shows toasts in the corner stack and closes them", () => {
  vi.useFakeTimers();
  render(<Toaster />);
  act(() => void toast.success("Saved the list."));
  act(() => void toast.error("Couldn't save."));
  const region = screen.getByRole("region", { name: "Notifications" });
  expect(region).toContainElement(screen.getByText("Saved the list."));
  expect(screen.getByRole("alert")).toHaveTextContent("Couldn't save.");

  fireEvent.click(screen.getAllByRole("button", { name: "Close notification" })[0]);
  act(() => void vi.runAllTimers());
  expect(screen.queryByText("Saved the list.")).not.toBeInTheDocument();
  expect(screen.getByText("Couldn't save.")).toBeInTheDocument();
});

it("times out on its own, but not while hovered, and not when sticky", () => {
  render(<Toaster />);
  act(() => void toast.info("Short one"));
  act(() => void toast.warning("Sticky one", { duration: null }));
  const card = screen.getByText("Short one").closest(".toast")!;
  const timer = card.querySelector(".toast-timer") as HTMLElement;
  fireEvent.mouseEnter(card);
  expect(timer.style.animationPlayState).toBe("paused");
  fireEvent.mouseLeave(card);
  expect(timer.style.animationPlayState).toBe("running");
  expect(screen.getByText("Sticky one").closest(".toast")!.querySelector(".toast-timer")).toBeNull();

  vi.useFakeTimers();
  fireEvent.animationEnd(timer);
  act(() => void vi.runAllTimers());
  expect(screen.queryByText("Short one")).not.toBeInTheDocument();
  expect(screen.getByText("Sticky one")).toBeInTheDocument();
});

it("shows the same message once, runs its action and reports dismissal", () => {
  const onDismiss = vi.fn();
  const onClick = vi.fn();
  render(<Toaster />);
  act(() => void toast.error("Offline"));
  act(() => void toast.error("Offline"));
  expect(screen.getAllByText("Offline")).toHaveLength(1);

  act(() => void toast.info("Update ready", { id: "update", action: { label: "Open", onClick }, onDismiss }));
  fireEvent.click(screen.getByRole("button", { name: "Open" }));
  expect(onClick).toHaveBeenCalledOnce();
  expect(onDismiss).toHaveBeenCalledOnce();
});
