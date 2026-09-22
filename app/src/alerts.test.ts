import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { AlertRow, PlaneAlerts } from "./bindings";
import { REREAD_EVERY_MS, countOf, useAlerts } from "./alerts";

/**
 * **What the status line may count**, and how the reading is kept fresh.
 *
 * The rule is `footer.rs`'s zone 1: a count charter cannot stand behind is dropped, never
 * drawn as zero. Each case below is one way a total could be smaller than the truth.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
});

const A = "/home/dev/a";
const B = "/home/dev/b";

function row(subject: string): AlertRow {
  return { severity: "warn", subject, detail: "d", remedy: "r" };
}

function plane(at: string, alerts: AlertRow[], stopped: string | null = null): PlaneAlerts {
  return { plane: at, alerts, stopped };
}

describe("the count", () => {
  it("adds every project's alerts, because the drawer lists every project", () => {
    const reading = {
      at: "read" as const,
      planes: [plane(A, [row("reinit")]), plane(B, [row("front door"), row("plane root")])],
    };
    expect(countOf(reading, [A, B])).toBe(3);
  });

  it("is zero when every project was read to the end and none has an alert", () => {
    expect(countOf({ at: "read", planes: [plane(A, []), plane(B, [])] }, [A, B])).toBe(0);
  });

  it("is dropped before anything has answered", () => {
    expect(countOf({ at: "reading" }, [A])).toBeUndefined();
  });

  it("is dropped when the ask failed", () => {
    expect(countOf({ at: "failed", why: "no" }, [A])).toBeUndefined();
  });

  it("is dropped when charter stopped looking in any one project", () => {
    // The alerts it found stand, but their number is not that project's number.
    const reading = {
      at: "read" as const,
      planes: [plane(A, [row("reinit")]), plane(B, [], "charter.toml is not valid TOML")],
    };
    expect(countOf(reading, [A, B])).toBeUndefined();
  });

  it("is dropped when a project this window holds is not in the reading yet", () => {
    // Opened after the reading was taken: its alerts are not in the total, so the total is
    // not the window's.
    expect(countOf({ at: "read", planes: [plane(A, [])] }, [A, B])).toBeUndefined();
  });
});

describe("the reading", () => {
  it("asks the core once, for every project and no project in particular", async () => {
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      return [plane(A, [row("reinit")])];
    });
    const { result } = renderHook(() => useAlerts([A]));

    await waitFor(() => expect(result.current.reading.at).toBe("read"));
    const mine = asked.filter((one) => one.cmd === "alerts_everywhere");
    expect(mine).toHaveLength(1);
    // Cross-project by construction: nothing names a plane.
    expect(mine[0].args ?? {}).toEqual({});
  });

  it("asks again when told to, and when the projects change", async () => {
    let asked = 0;
    mockIPC((cmd) => {
      if (cmd === "alerts_everywhere") asked += 1;
      return [];
    });
    const { result, rerender } = renderHook(({ planes }) => useAlerts(planes), {
      initialProps: { planes: [A] },
    });
    await waitFor(() => expect(asked).toBe(1));

    act(() => result.current.reread());
    await waitFor(() => expect(asked).toBe(2));

    rerender({ planes: [A, B] });
    await waitFor(() => expect(asked).toBe(3));
  });

  it("asks again when the window comes back into focus, and on its own once a minute", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    let asked = 0;
    mockIPC((cmd) => {
      if (cmd === "alerts_everywhere") asked += 1;
      return [];
    });
    renderHook(() => useAlerts([A]));
    await waitFor(() => expect(asked).toBe(1));

    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(asked).toBe(2));

    act(() => {
      vi.advanceTimersByTime(REREAD_EVERY_MS);
    });
    await waitFor(() => expect(asked).toBe(3));
  });

  it("says the core's words when the ask fails, and never makes up an empty reading", async () => {
    mockIPC(() => {
      throw "the registry is gone";
    });
    const { result } = renderHook(() => useAlerts([A]));

    await waitFor(() => expect(result.current.reading.at).toBe("failed"));
    expect(result.current.reading).toEqual({ at: "failed", why: "the registry is gone" });
  });

  it("does not take an answer that is not a list for a reading with nothing in it", async () => {
    mockIPC(() => null);
    const { result } = renderHook(() => useAlerts([A]));

    await waitFor(() => expect(result.current.reading.at).toBe("failed"));
  });
});
