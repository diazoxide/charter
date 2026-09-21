import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { ALL_SHOWN, remembered, useRegions } from "./regions";

const KEY = "charter.regions.shown";

beforeEach(() => globalThis.localStorage.clear());
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("which regions are drawn", () => {
  it("draws all three when nothing has been remembered", () => {
    expect(remembered()).toEqual(ALL_SHOWN);
  });

  it("puts a region away and brings it back", () => {
    const { result } = renderHook(() => useRegions());

    act(() => result.current.toggle("explorer"));
    expect(result.current.shown).toEqual({ ...ALL_SHOWN, explorer: false });

    act(() => result.current.toggle("explorer"));
    expect(result.current.shown).toEqual(ALL_SHOWN);
  });

  it("remembers the answer, so a project switch does not bring a region back", () => {
    // The regions are the WINDOW's layout, and a `PlaneView` that is not in front draws
    // nothing at all — so without this, looking at another project and back would undo it.
    const { result } = renderHook(() => useRegions());

    act(() => result.current.toggle("bottom"));

    expect(remembered()).toEqual({ ...ALL_SHOWN, bottom: false });
  });

  it("draws a region the stored answer does not mention", () => {
    // An answer written by an older build, or by hand. Losing a region with no way to notice
    // it went is worse than ignoring half a preference.
    globalThis.localStorage.setItem(KEY, JSON.stringify({ explorer: false }));

    expect(remembered()).toEqual({ ...ALL_SHOWN, explorer: false });
  });

  it("draws all three when what was stored is not an answer at all", () => {
    globalThis.localStorage.setItem(KEY, "{{{");

    expect(remembered()).toEqual(ALL_SHOWN);
  });

  it("draws all three when the webview will not let charter read its storage", () => {
    vi.spyOn(globalThis.Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage is blocked");
    });

    expect(remembered()).toEqual(ALL_SHOWN);
  });

  it("still puts a region away when the webview will not let charter store it", () => {
    // The preference is lost at the next launch. The window must not fail to lay out over it.
    vi.spyOn(globalThis.Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("storage is blocked");
    });
    const { result } = renderHook(() => useRegions());

    act(() => result.current.toggle("aside"));

    expect(result.current.shown.aside).toBe(false);
  });
});
