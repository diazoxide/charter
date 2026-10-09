import { afterEach, describe, expect, it } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { HarnessGlance, PlaneId } from "./bindings";
import { useHarnessCards } from "./harnessCards";

const PLANE = "/plane" as unknown as PlaneId;

const CODEX: HarnessGlance = {
  name: "codex",
  title: "Codex",
  label: "What Codex can do here",
  lines: [],
  cannot_type: null,
};

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The core, answering `harness_cards` with `cards`, and counting each ask. */
function core(cards: HarnessGlance[] | string) {
  const asked: string[] = [];
  mockIPC((cmd) => {
    asked.push(cmd);
    if (cmd !== "harness_cards") throw new Error(`unexpected ${cmd}`);
    if (typeof cards === "string") throw cards;
    return cards;
  });
  return asked;
}

describe("the harness cards the palette lists (#1134)", () => {
  it("reads the project's cards while it is in front, and again when its settings change", async () => {
    const asked = core([CODEX]);
    const { result, rerender } = renderHook(
      ({ changes }) => useHarnessCards(PLANE, true, changes),
      { initialProps: { changes: 0 } },
    );

    await waitFor(() => expect(result.current).toEqual([CODEX]));
    rerender({ changes: 1 });
    await waitFor(() => expect(asked).toHaveLength(2));
    // Only the declarations: never the picker's `start_options`.
    expect(new Set(asked)).toEqual(new Set(["harness_cards"]));
  });

  it("reads nothing for a project behind, and lists none it could not read", async () => {
    const asked = core([CODEX]);
    renderHook(() => useHarnessCards(PLANE, false, 0));
    expect(asked).toEqual([]);
    cleanup();

    core("the project's harnesses could not be read");
    const { result } = renderHook(() => useHarnessCards(PLANE, true, 0));
    await new Promise((settled) => setTimeout(settled, 0));
    expect(result.current).toEqual([]);
  });
});
