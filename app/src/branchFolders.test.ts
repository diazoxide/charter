import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PlaneId } from "./bindings";
import { useBranchFolders, WATCH_WAIT_MS, type BranchFolderRef } from "./branchFolders";

/**
 * **A folder's first read waits on its watch, but not forever** (#1427's train 16 review). The
 * read waits for the `files_watch` naming the folder, so a change made before the watch held is
 * in it; a watch the core never answers, or watches that keep being sent while folders are
 * toggled, would otherwise hold the read back for good. The explorer's own tests
 * (`Explorer.files.test.tsx`) cover the ordering on the real tree.
 */

const PLANE = "/plane" as unknown as PlaneId;

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
});

const folder = (name: string): BranchFolderRef => ({ repo: "svc", piece: "one", folder: name });

/** The core: `files_watch` answered after `watchTakes` ms, or never when it is `undefined`;
 *  `branch_tree` answered at once and recorded. */
function core(watchTakes: number | undefined) {
  const read: string[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "branch_tree") {
      read.push((args as { folder: string }).folder);
      return { entries: [], more: 0 };
    }
    if (cmd === "files_watch")
      return new Promise<null>((answer) => {
        if (watchTakes !== undefined) setTimeout(() => answer(null), watchTakes);
      });
    return null;
  });
  return read;
}

describe("a folder's first read", () => {
  it("waits for the watch that names it while that is answered in time", async () => {
    const read = core(300);
    renderHook(() => useBranchFolders(PLANE, "alpha", [folder("")]));

    await vi.advanceTimersByTimeAsync(200);
    expect(read).toEqual([]);
    await vi.advanceTimersByTimeAsync(200);
    expect(read).toEqual([""]);
  });

  it("goes ahead after the bound when the watch is never answered", async () => {
    const read = core(undefined);
    renderHook(() => useBranchFolders(PLANE, "alpha", [folder("")]));

    await vi.advanceTimersByTimeAsync(WATCH_WAIT_MS - 100);
    expect(read).toEqual([]);
    await vi.advanceTimersByTimeAsync(200);
    expect(read).toEqual([""]);
  });

  it("goes ahead after the bound while newer watches keep being sent", async () => {
    // Each watch takes a second and a new one is sent every half second, as folders toggled
    // quickly would: the newest is never answered before the next goes out.
    const read = core(1_000);
    const view = renderHook(({ open }) => useBranchFolders(PLANE, "alpha", open), {
      initialProps: { open: [folder("")] },
    });
    const open = [folder("")];
    for (let at = 1; at * 500 < WATCH_WAIT_MS + 500; at++) {
      await vi.advanceTimersByTimeAsync(500);
      // Another folder opened and the last one closed again: the first stays open throughout.
      open.splice(1, 1, folder(`f${at}`));
      view.rerender({ open: [...open] });
      if (at * 500 < WATCH_WAIT_MS - 100) expect(read).not.toContain("");
    }

    expect(read).toContain("");
  });
});
