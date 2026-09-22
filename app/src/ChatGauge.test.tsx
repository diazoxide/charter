import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, renderHook, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { AFTER_A_MOVE_MS, ChatGauge, useChatUsage, WHILE_RUNNING_MS } from "./ChatGauge";
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
      <ChatGauge usage={{ context: null, cache: { value: 40, tone: "bad" }, rebuilds: null }} />,
    );

    const gauge = screen.getByTestId("chat-gauge");
    expect(gauge.textContent).not.toContain("ctx");
    expect(gauge.textContent).toContain("cache 40%");
    expect(gauge.textContent).not.toContain("↻");
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
