import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { ASKS_NOTIFIED, useInboxOpenTold, useNotificationLanding } from "./askNotices";

/**
 * The window's half of the asks' system notifications (#1694, I-7): the core is told whether a
 * project's Inbox is open, so it sends nothing about what the person is reading; and a
 * notification the person clicks brings the window to the front, which lands on the Inbox at
 * that chat's group.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

const PLANE = "/home/dev/plane";

/** Every command the window sent, with what it sent. */
function sent() {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args: args as Record<string, unknown> });
      return null;
    },
    { shouldMockEvents: true },
  );
  return calls;
}

const told = (calls: ReturnType<typeof sent>) =>
  calls.filter((one) => one.cmd === "inbox_shown").map((one) => one.args);

describe("whether the Inbox is open", () => {
  it("is told when it opens and when it closes", async () => {
    const calls = sent();
    const { rerender } = renderHook(({ open }) => useInboxOpenTold(PLANE, open), {
      initialProps: { open: true },
    });
    await waitFor(() => expect(told(calls)).toEqual([{ plane: PLANE, open: true }]));
    rerender({ open: false });
    await waitFor(() => expect(told(calls).at(-1)).toEqual({ plane: PLANE, open: false }));
  });

  it("is told closed when the project's view goes", async () => {
    const calls = sent();
    const { unmount } = renderHook(() => useInboxOpenTold(PLANE, true));
    await waitFor(() => expect(told(calls)).toHaveLength(1));
    unmount();
    await waitFor(() => expect(told(calls).at(-1)).toEqual({ plane: PLANE, open: false }));
  });
});

describe("a notification clicked", () => {
  /** The window behind another app, as it is when a notification is clicked. */
  const behind = () => vi.spyOn(document, "hasFocus").mockReturnValue(false);

  it("lands on the Inbox at that chat's group when the window comes to the front", async () => {
    sent();
    behind();
    const land = vi.fn();
    renderHook(() => useNotificationLanding(land));
    await act(async () => {
      await emit(ASKS_NOTIFIED, { plane: PLANE, session: 3 });
    });
    expect(land).not.toHaveBeenCalled();
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(land).toHaveBeenCalledWith({ plane: PLANE, session: 3 });
    // Once: coming back again later is not the click.
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(land).toHaveBeenCalledTimes(1);
  });

  it("lands on the latest of several", async () => {
    sent();
    behind();
    const land = vi.fn();
    renderHook(() => useNotificationLanding(land));
    await act(async () => {
      await emit(ASKS_NOTIFIED, { plane: PLANE, session: 3 });
      await emit(ASKS_NOTIFIED, { plane: PLANE, session: 5 });
    });
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(land).toHaveBeenCalledWith({ plane: PLANE, session: 5 });
    expect(land).toHaveBeenCalledTimes(1);
  });

  it("moves nothing when the window was already in front, where no click brings it", async () => {
    sent();
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const land = vi.fn();
    renderHook(() => useNotificationLanding(land));
    await act(async () => {
      await emit(ASKS_NOTIFIED, { plane: PLANE, session: 3 });
    });
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(land).not.toHaveBeenCalled();
  });

  it("ignores a notice that names no chat", async () => {
    sent();
    behind();
    const land = vi.fn();
    renderHook(() => useNotificationLanding(land));
    await act(async () => {
      await emit(ASKS_NOTIFIED, { plane: PLANE, session: "3" });
    });
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(land).not.toHaveBeenCalled();
  });
});
