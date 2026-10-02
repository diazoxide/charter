import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { RepoSaving } from "./bindings";
import { useRepoSaving } from "./saving";

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
});
