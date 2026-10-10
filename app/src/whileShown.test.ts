import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { forgetShown, windowShown as shown } from "./test-shown";
import { everyWhileShown, useWhileShown } from "./whileShown";

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  forgetShown();
  vi.useRealTimers();
});

describe("one scheduler for the window's timed reads", () => {
  it("reads every interval while the window is shown", () => {
    const read = vi.fn();
    const stop = everyWhileShown(1000, read);
    vi.advanceTimersByTime(3000);
    expect(read).toHaveBeenCalledTimes(3);
    stop();
  });

  it("reads nothing while the window is hidden", () => {
    const read = vi.fn();
    const stop = everyWhileShown(1000, read);
    shown(false);
    vi.advanceTimersByTime(60_000);
    expect(read).not.toHaveBeenCalled();
    stop();
  });

  it("catches up with one read the moment the window is shown again, then keeps its beat", () => {
    const read = vi.fn();
    const stop = everyWhileShown(1000, read);
    shown(false);
    vi.advanceTimersByTime(10_000);
    shown(true);
    expect(read).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(999);
    expect(read).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(read).toHaveBeenCalledTimes(2);
    stop();
  });

  it("does not start a beat for a read added while the window is hidden, and reads it on show", () => {
    shown(false);
    const read = vi.fn();
    const stop = everyWhileShown(1000, read);
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(5000);
    expect(read).not.toHaveBeenCalled();
    shown(true);
    expect(read).toHaveBeenCalledTimes(1);
    stop();
  });

  it("shares one timer between two reads of one interval, and runs one per interval", () => {
    const one = vi.fn();
    const two = vi.fn();
    const other = vi.fn();
    const stops = [
      everyWhileShown(1000, one),
      everyWhileShown(1000, two),
      everyWhileShown(5000, other),
    ];
    expect(vi.getTimerCount()).toBe(2);
    vi.advanceTimersByTime(5000);
    expect([one.mock.calls.length, two.mock.calls.length, other.mock.calls.length]).toEqual([
      5, 5, 1,
    ]);
    for (const stop of stops) stop();
  });

  it("clears the timer with the last read that stops, and keeps it until then", () => {
    const one = vi.fn();
    const stopOne = everyWhileShown(1000, one);
    const stopTwo = everyWhileShown(1000, vi.fn());
    stopTwo();
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(1000);
    expect(one).toHaveBeenCalledTimes(1);
    stopOne();
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(10_000);
    shown(false);
    shown(true);
    expect(one).toHaveBeenCalledTimes(1);
  });

  it("stops one read once even when it is stopped twice", () => {
    const kept = vi.fn();
    const stopKept = everyWhileShown(1000, kept);
    const stop = everyWhileShown(1000, vi.fn());
    stop();
    stop();
    vi.advanceTimersByTime(1000);
    expect(kept).toHaveBeenCalledTimes(1);
    stopKept();
  });

  it("gives the same read twice its own two subscriptions", () => {
    const read = vi.fn();
    const first = everyWhileShown(1000, read);
    const second = everyWhileShown(1000, read);
    first();
    vi.advanceTimersByTime(1000);
    expect(read).toHaveBeenCalledTimes(1);
    second();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("lets every other read run when one throws", () => {
    const thrown = vi.fn(() => {
      throw new Error("a read that failed");
    });
    const read = vi.fn();
    const stops = [everyWhileShown(1000, thrown), everyWhileShown(1000, read)];
    // The failure is reported, not swallowed: it is thrown again outside the beat.
    const reported = vi.spyOn(globalThis, "queueMicrotask").mockImplementation(() => undefined);
    vi.advanceTimersByTime(1000);
    expect(read).toHaveBeenCalledTimes(1);
    expect(reported).toHaveBeenCalledTimes(1);
    reported.mockRestore();
    for (const stop of stops) stop();
  });
});

describe("useWhileShown", () => {
  it("reads the latest callback each beat, and only while on", () => {
    const first = vi.fn();
    const later = vi.fn();
    const { rerender, unmount } = renderHook(
      ({ read, on }: { read: () => void; on: boolean }) => useWhileShown(1000, read, on),
      { initialProps: { read: first, on: true } },
    );
    vi.advanceTimersByTime(1000);
    rerender({ read: later, on: true });
    vi.advanceTimersByTime(1000);
    expect([first.mock.calls.length, later.mock.calls.length]).toEqual([1, 1]);
    rerender({ read: later, on: false });
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(5000);
    expect(later).toHaveBeenCalledTimes(1);
    unmount();
  });

  it("catches up on show and stops when unmounted", () => {
    const read = vi.fn();
    const { unmount } = renderHook(() => useWhileShown(1000, read));
    shown(false);
    vi.advanceTimersByTime(5000);
    shown(true);
    expect(read).toHaveBeenCalledTimes(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
