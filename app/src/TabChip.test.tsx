import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { ChatsHere, fixedChats, nothingKnown } from "./chatState";
import type { ChatRow } from "./chatsTree";
import { TabTasks } from "./TabChip";
import { REST_MS, sinceClock, type Needing } from "./tabTasks";

/**
 * **A tab's chip, drawn alone** (#1487): what it does that the window has no way to bring
 * about in a test, a tab being carried, a touch, and the footer later tickets fill.
 * `TabChip.window.test.tsx` holds it in the window.
 */

function row(session: number, level: number, name: string): ChatRow {
  return {
    session,
    name,
    persona: null,
    workspace: "alpha",
    shell: false,
    parent: level === 1 ? null : 1,
    mode: level === 1 ? null : "task",
    from: null,
    tab: level === 1,
    branch: null,
    report: level === 1 ? null : "owed",
    outcome: null,
    asking: null,
    harness: "Claude Code",
    level,
    posinset: 1,
    setsize: 1,
    orphaned: false,
  };
}

const ROWS = [row(1, 1, "steward 1"), row(2, 2, "talk")];

function chip(
  over: {
    rows?: ChatRow[];
    needs?: Needing[];
    dragging?: () => boolean;
    limits?: ReactNode;
    totals?: ReactNode;
  } = {},
) {
  const onShow = vi.fn();
  render(
    <ChatsHere.Provider
      value={fixedChats({ ...nothingKnown, bySession: { 1: "running", 2: "running" } })}
    >
      <TabTasks
        name="steward 1"
        rows={over.rows ?? ROWS}
        ended={[]}
        current={1}
        needs={over.needs ?? []}
        asked={0}
        keySaid="Ctrl+Shift+J"
        clock={sinceClock()}
        dragging={over.dragging ?? (() => false)}
        onShow={onShow}
        limits={over.limits}
        totals={over.totals}
      />
    </ChatsHere.Provider>,
  );
  return onShow;
}

const counts = () => screen.getByRole("button", { name: /^Tasks of steward 1/ });
const menu = () => screen.queryByRole("menu");

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("a tab's chip", () => {
  it("is nothing at all for a session with no tasks", () => {
    chip({ rows: [row(1, 1, "steward 1")] });

    expect(document.body.querySelector(".tab-tasks")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("says its counts in words, and its key where a pointer rests", () => {
    chip();

    expect(counts().getAttribute("aria-label")).toBe("Tasks of steward 1: 1 working");
    expect(counts().getAttribute("title")).toBe("Tasks of steward 1: 1 working. Ctrl+Shift+J");
    // The drawn count is decoration to a screen reader: the name has said it.
    expect(counts().querySelector("[data-count]")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("opens no menu while a tab is being carried, by a rest or by the press that ends it", () => {
    vi.useFakeTimers();
    chip({ dragging: () => true });

    fireEvent.pointerEnter(counts(), { pointerType: "mouse" });
    act(() => void vi.advanceTimersByTime(REST_MS * 4));
    expect(menu()).toBeNull();

    fireEvent.click(counts());
    expect(menu()).toBeNull();
  });

  it("does not open when a drag begins while the pointer is resting on it", () => {
    vi.useFakeTimers();
    let carried = false;
    chip({ dragging: () => carried });

    fireEvent.pointerEnter(counts(), { pointerType: "mouse" });
    act(() => void vi.advanceTimersByTime(REST_MS - 10));
    carried = true;
    act(() => void vi.advanceTimersByTime(REST_MS));

    expect(menu()).toBeNull();
  });

  it("opens under a finger only when pressed: a touch does not rest", () => {
    vi.useFakeTimers();
    chip();

    fireEvent.pointerEnter(counts(), { pointerType: "touch" });
    act(() => void vi.advanceTimersByTime(REST_MS * 4));
    expect(menu()).toBeNull();

    fireEvent.click(counts());
    expect(menu()).not.toBeNull();
  });

  it("goes to the chat that waits from its hand, and opens no menu for that", () => {
    const onShow = chip({ needs: [{ session: 2, name: "talk" }] });

    fireEvent.click(screen.getByRole("button", { name: "talk needs you. Go to talk" }));

    expect(onShow).toHaveBeenCalledWith(2);
    expect(menu()).toBeNull();
  });

  it("draws no footer in its menu until something has a line for it", () => {
    chip();
    fireEvent.click(counts());

    expect((menu() as HTMLElement).querySelector(".tasks-menu-foot")).toBeNull();
  });

  it("draws the limits and the totals it is handed at the foot of its menu, under the rows", () => {
    chip({ limits: "4 of 6 running", totals: "5 tasks · 310k tokens · 6m" });
    fireEvent.click(counts());

    const foot = (menu() as HTMLElement).querySelector(".tasks-menu-foot") as HTMLElement;
    expect(foot.querySelector(".tasks-menu-limits")?.textContent).toBe("4 of 6 running");
    expect(foot.querySelector(".tasks-menu-totals")?.textContent).toBe(
      "5 tasks · 310k tokens · 6m",
    );
    // After every row, and not a row: nothing in it is picked.
    expect((menu() as HTMLElement).lastElementChild).toBe(foot);
    expect(within(foot).queryByRole("menuitem")).toBeNull();
  });
});
