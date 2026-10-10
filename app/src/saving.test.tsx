import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { RepoSaving } from "./bindings";
import { SAVING_REREAD_MS, usePlaneSaving, useRepoSaving, useRepoSavingKept } from "./saving";
import { forgetShown, windowShown } from "./test-shown";

/**
 * The repos' save standing the title bar counts in, read for the project in front (charter-app
 * #299) — and what it draws when the window switches between projects (FR-27).
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const ONE = "/home/dev/one";
const TWO = "/home/dev/two";

function repo(name: string, stage: string): RepoSaving {
  return { name, stage } as RepoSaving;
}

/** A core whose `workspace_saving` answers each project only when the test says so. */
function core() {
  const waiting: { plane: string; answer: (rows: RepoSaving[]) => void }[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "plugin:event|listen") return 1;
    if (cmd !== "workspace_saving") return null;
    const plane = (args as { plane: string }).plane;
    return new Promise<RepoSaving[]>((answer) => waiting.push({ plane, answer }));
  });
  /** Answers the oldest question still out about `plane`. */
  const answer = async (plane: string, rows: RepoSaving[]) => {
    const at = waiting.findIndex((one) => one.plane === plane);
    if (at < 0) throw new Error(`nothing asked about ${plane}`);
    const [one] = waiting.splice(at, 1);
    await act(async () => one.answer(rows));
  };
  return { waiting, answer };
}

describe("the repos of the project in front, across a switch", () => {
  it("draws a project's last rows the moment it is back in front, and asks again behind them", async () => {
    const { waiting, answer } = core();
    // The project behind holds its own rows, as its view does while the window holds it.
    renderHook(() => useRepoSavingKept(ONE, "alpha"));
    const { result, rerender } = renderHook(({ plane }) => useRepoSaving(plane, "alpha"), {
      initialProps: { plane: ONE },
    });
    await waitFor(() => expect(waiting.some((one) => one.plane === ONE)).toBe(true));
    await answer(ONE, [repo("svc", "changed")]);
    expect(result.current).toEqual([repo("svc", "changed")]);

    rerender({ plane: TWO });
    expect(result.current).toBeUndefined();
    await waitFor(() => expect(waiting.some((one) => one.plane === TWO)).toBe(true));
    await answer(TWO, [repo("web", "saved")]);
    expect(result.current).toEqual([repo("web", "saved")]);

    rerender({ plane: ONE });
    expect(result.current).toEqual([repo("svc", "changed")]);
    await waitFor(() => expect(waiting.some((one) => one.plane === ONE)).toBe(true));
  });

  it("draws what the core says once it says something new", async () => {
    const { waiting, answer } = core();
    renderHook(() => useRepoSavingKept(ONE, "alpha"));
    const { result, rerender } = renderHook(({ plane }) => useRepoSaving(plane, "alpha"), {
      initialProps: { plane: ONE },
    });
    await waitFor(() => expect(waiting.length).toBeGreaterThan(0));
    await answer(ONE, [repo("svc", "changed")]);
    rerender({ plane: TWO });
    rerender({ plane: ONE });
    await waitFor(() => expect(waiting.some((one) => one.plane === ONE)).toBe(true));

    await answer(ONE, [repo("svc", "saved")]);

    expect(result.current).toEqual([repo("svc", "saved")]);
  });

  it("forgets a project's rows once nothing holds them", async () => {
    const { waiting, answer } = core();
    const { result, rerender } = renderHook(({ plane }) => useRepoSaving(plane, "alpha"), {
      initialProps: { plane: ONE },
    });
    await waitFor(() => expect(waiting.some((one) => one.plane === ONE)).toBe(true));
    await answer(ONE, [repo("svc", "changed")]);

    rerender({ plane: TWO });
    await waitFor(() => expect(waiting.some((one) => one.plane === TWO)).toBe(true));
    await answer(TWO, [repo("web", "saved")]);
    rerender({ plane: ONE });

    expect(result.current).toBeUndefined();
  });

  it("draws nothing again for an answer that says what is already drawn", async () => {
    const { waiting, answer } = core();
    let renders = 0;
    const { result } = renderHook(() => {
      renders += 1;
      return useRepoSaving(ONE, "alpha");
    });
    await waitFor(() => expect(waiting.length).toBe(1));
    await answer(ONE, [repo("svc", "changed")]);
    expect(result.current).toEqual([repo("svc", "changed")]);

    // Coming back to the window asks again (`useRereads`).
    await act(async () => void window.dispatchEvent(new Event("focus")));
    await waitFor(() => expect(waiting.length).toBe(1));
    const asked = renders;
    await answer(ONE, [repo("svc", "changed")]);

    expect(renders).toBe(asked);
  });
});

describe("the standing's beat, and the window out of sight (#1392)", () => {
  afterEach(() => {
    forgetShown();
    vi.useRealTimers();
  });

  /** A core that answers every read at once, counting each command it is asked. */
  function counting() {
    const asked = new Map<string, number>();
    mockIPC((cmd) => {
      if (cmd === "plugin:event|listen") return 1;
      asked.set(cmd, (asked.get(cmd) ?? 0) + 1);
      if (cmd === "workspace_saving") return [];
      if (cmd === "plane_saving") return { stage: "saved" };
      return null;
    });
    return (cmd: string) => asked.get(cmd) ?? 0;
  }

  it("asks again on every beat while shown, never while hidden, and once on show", async () => {
    // Only the beat's clock and the clock the return fold reads: the window's promises settle as
    // they do.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
    const asked = counting();
    renderHook(() => {
      usePlaneSaving(ONE);
      useRepoSaving(ONE, "alpha");
    });
    await waitFor(() => expect(asked("workspace_saving")).toBe(1));
    expect(asked("plane_saving")).toBe(1);

    await act(async () => void vi.advanceTimersByTime(SAVING_REREAD_MS));
    expect([asked("plane_saving"), asked("workspace_saving")]).toEqual([2, 2]);

    act(() => windowShown(false));
    await act(async () => void vi.advanceTimersByTime(SAVING_REREAD_MS * 30));
    expect([asked("plane_saving"), asked("workspace_saving")]).toEqual([2, 2]);

    await act(async () => windowShown(true));
    expect([asked("plane_saving"), asked("workspace_saving")]).toEqual([3, 3]);
  });

  it("reads once for a return that both shows and focuses the window (D-1392-6)", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
    const asked = counting();
    renderHook(() => {
      usePlaneSaving(ONE);
      useRepoSaving(ONE, "alpha");
    });
    await waitFor(() => expect(asked("workspace_saving")).toBe(1));
    act(() => windowShown(false));
    vi.advanceTimersByTime(60_000);

    // The engine shows the window, then gives it focus, each its own task: one return, so one
    // `git status`.
    await act(async () => windowShown(true));
    await act(async () => void window.dispatchEvent(new Event("focus")));
    expect([asked("plane_saving"), asked("workspace_saving")]).toEqual([2, 2]);

    // The other order folds the same way.
    act(() => windowShown(false));
    vi.advanceTimersByTime(60_000);
    await act(async () => void window.dispatchEvent(new Event("focus")));
    await act(async () => windowShown(true));
    expect([asked("plane_saving"), asked("workspace_saving")]).toEqual([3, 3]);
  });

  it("still reads for a focus well after the show, and for every finished save", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
    const asked = counting();
    renderHook(() => usePlaneSaving(ONE));
    await waitFor(() => expect(asked("plane_saving")).toBe(1));
    act(() => windowShown(false));
    await act(async () => windowShown(true));
    expect(asked("plane_saving")).toBe(2);

    // A save that finished is news, whenever it lands.
    await act(async () => void window.dispatchEvent(new Event("charter-plane-saved")));
    expect(asked("plane_saving")).toBe(3);

    // Focus a few seconds later is a return of its own (the window was only behind another).
    vi.advanceTimersByTime(5_000);
    await act(async () => void window.dispatchEvent(new Event("focus")));
    expect(asked("plane_saving")).toBe(4);
  });
});
