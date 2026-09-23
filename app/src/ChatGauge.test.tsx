/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, renderHook, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { AFTER_A_MOVE_MS, ChatGauge, Trend, useChatUsage, WHILE_RUNNING_MS } from "./ChatGauge";
import type { ChatUsage } from "./bindings";

/**
 * **A chat's gauge**: what it draws from the core's answer, and when it asks again.
 *
 * The numbers and their tones are the core's (`charter_core::usage::gauge`, pinned against the
 * Python charter's own helpers); what is here is the window's half.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
});

const PLANE = "/home/dev/plane";

const USAGE: ChatUsage = {
  context: { value: 83, tone: "bad" },
  cache: { value: 98, tone: "ok" },
  rebuilds: { count: 2, cost: "696k", tone: "bad" },
  turns: [],
  cold: null,
};

describe("the gauge", () => {
  it("draws ctx, cache and the rebuilds, each in its tone", () => {
    render(<ChatGauge usage={USAGE} />);

    const gauge = screen.getByTestId("chat-gauge");
    expect(gauge.textContent?.replace(/\s+/g, " ")).toBe("ctx 83%cache 98%↻2 696k");
    expect(screen.getByText("83%")).toHaveClass("gauge-bad");
    expect(screen.getByText("98%")).toHaveClass("gauge-ok");
    expect(gauge.getAttribute("title")).toContain("context window 83% full");
  });

  it("draws nothing at all when charter knows nothing, never a zero", () => {
    render(<ChatGauge usage={undefined} />);

    expect(screen.queryByTestId("chat-gauge")).toBeNull();
  });

  it("draws only the parts that are known", () => {
    // Early in a session there is usage and no percentage yet: `cache` without `ctx`.
    render(
      <ChatGauge
        usage={{
          context: null,
          cache: { value: 40, tone: "bad" },
          rebuilds: null,
          turns: [],
          cold: null,
        }}
      />,
    );

    const gauge = screen.getByTestId("chat-gauge");
    expect(gauge.textContent).not.toContain("ctx");
    expect(gauge.textContent).toContain("cache 40%");
    expect(gauge.textContent).not.toContain("↻");
  });
});

describe("the trend", () => {
  const turn = (value: number, tone: "ok" | "warn" | "bad") => ({
    cache: { value, tone },
    context: { value: 20, tone: "ok" as const },
    written: "1k",
  });

  it("draws a bar per turn, as tall as its cache share and in its tone", () => {
    render(<Trend turns={[turn(100, "ok"), turn(10, "bad"), turn(60, "warn")]} />);

    const bars = [...screen.getByTestId("gauge-trend").querySelectorAll("rect")];
    expect(bars.map((bar) => bar.getAttribute("height"))).toEqual(["10", "1", "6"]);
    expect(bars.map((bar) => bar.getAttribute("class"))).toEqual([
      "gauge-ok",
      "gauge-bad",
      "gauge-warn",
    ]);
    expect(bars[1].textContent).toBe("turn 2, cache 10%, ctx 20%, wrote 1k");
  });

  it("draws a turn whose share is unknown as a muted stub, not as a zero", () => {
    render(<Trend turns={[turn(90, "ok"), { cache: null, context: null, written: null }]} />);

    const bars = [...screen.getByTestId("gauge-trend").querySelectorAll("rect")];
    expect(bars[1].getAttribute("class")).toBe("gauge-unknown");
    expect(bars[1].textContent).toBe("turn 2, cache unknown");
  });

  it("is drawn beside the gauge only once there are two turns to compare", () => {
    const { rerender } = render(<ChatGauge usage={{ ...USAGE, turns: [turn(90, "ok")] }} />);
    expect(screen.queryByTestId("gauge-trend")).toBeNull();

    rerender(<ChatGauge usage={{ ...USAGE, turns: [turn(90, "ok"), turn(95, "ok")] }} />);
    expect(screen.getByTestId("gauge-trend")).toBeInTheDocument();
  });

  it("says a cold streak in words once the core says there is one", () => {
    const { rerender } = render(<ChatGauge usage={USAGE} />);
    expect(screen.getByTestId("chat-gauge").textContent).not.toContain("cold");

    rerender(<ChatGauge usage={{ ...USAGE, cold: 3 }} />);
    expect(screen.getByText("cold 3")).toHaveClass("gauge-warn");
  });
});

describe("useChatUsage", () => {
  let asked: number;
  let answer: unknown;

  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    asked = 0;
    answer = USAGE;
    mockIPC((cmd) => {
      if (cmd !== "chat_usage") return null;
      asked += 1;
      return answer;
    });
  });

  /** Lets every pending ask land. */
  const settle = () =>
    act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

  it("reads when the pane opens, again a moment later, and then not while the chat is idle", async () => {
    const { result } = renderHook(() => useChatUsage(PLANE, 3, 1, false));
    await settle();
    expect(result.current).toEqual(USAGE);
    expect(asked).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(AFTER_A_MOVE_MS);
    });
    expect(asked).toBe(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(WHILE_RUNNING_MS * 5);
    });
    expect(asked).toBe(2);
  });

  it("reads again every few seconds while the chat is mid-turn", async () => {
    renderHook(() => useChatUsage(PLANE, 3, 1, true));
    await settle();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(WHILE_RUNNING_MS * 3);
    });

    // One at open, one a moment after, and one per tick.
    expect(asked).toBe(1 + 1 + 3);
  });

  it("reads again when the chat moves", async () => {
    const { rerender } = renderHook(({ moved }) => useChatUsage(PLANE, 3, moved, false), {
      initialProps: { moved: 1 },
    });
    await settle();
    expect(asked).toBe(1);

    rerender({ moved: 2 });
    await settle();

    expect(asked).toBe(2);
  });

  it("draws nothing for an answer that is not a gauge", async () => {
    answer = [];
    const { result } = renderHook(() => useChatUsage(PLANE, 3, 1, false));
    await settle();

    expect(asked).toBe(1);
    expect(result.current).toBeUndefined();
  });

  it("never shows one chat's numbers on the pane of another", async () => {
    const { result, rerender } = renderHook(
      ({ session }) => useChatUsage(PLANE, session, 1, false),
      { initialProps: { session: 3 } },
    );
    await settle();
    expect(result.current).toEqual(USAGE);

    answer = null;
    rerender({ session: 4 });

    // Before chat 4's answer lands, chat 3's numbers are already gone.
    expect(result.current).toBeUndefined();
  });
});

/**
 * **Where the gauge sits in a pane, held over the stylesheet** (charter-app#193).
 *
 * The operator: *"context status indicator in pane right corner can be moved to left corner. to
 * not make split and close buttons uggly"*. jsdom lays nothing out, so what can be held here is
 * the rule as written — and the three properties the move had to keep are all rules:
 *
 * - **neither thing positions itself**; a corner does, so a third thing in a pane's corner
 *   collides in review and not at runtime;
 * - **the gauge is always drawn**, so the controls' hover rule may not reach it;
 * - **it never sits on the terminal's first line**, which at top-left it would at every size,
 *   because a terminal's text starts at column 0 — so the pane gives up a row, and only when
 *   there is a gauge to put in it.
 */
describe("the gauge's corner", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  const rule = (selector: string) =>
    new RegExp(`(^|[},])\\s*${selector}\\s*\\{([^}]*)\\}`).exec(css)?.[2] ?? "";

  it("is the top-left corner, and the controls' is the top-right", () => {
    expect(rule("\\.pane-corner\\.at-start")).toMatch(/left:/);
    expect(rule("\\.pane-corner\\.at-end")).toMatch(/right:/);
  });

  it("leaves the positioning to the corner, for the gauge and the controls alike", () => {
    for (const own of [rule("\\.chat-gauge"), rule("\\.pane-doing")]) {
      expect(own).not.toBe("");
      expect(own).not.toMatch(/position:|top:|left:|right:/);
    }
  });

  it("never takes the controls' hover-only rule", () => {
    const hidden = [...css.matchAll(/([^{}]*)\{[^{}]*visibility:\s*hidden[^{}]*\}/g)]
      .map((hit) => hit[1])
      .join(",");
    expect(hidden).toContain(".pane-doing");
    expect(hidden).not.toContain("gauge");
  });

  it("floats over the terminal and never changes the pane's size", () => {
    // The operator: "context usage component position is not absolute and its changing harness
    // container sizes". A pane with a gauge must be exactly as tall as one without.
    expect(css).not.toMatch(/\.pane-frame\.gauged/);
    expect(css).not.toMatch(/--gauge-room/);
    expect(rule("\\.pane-corner")).toMatch(/position:\s*absolute/);
  });
});
