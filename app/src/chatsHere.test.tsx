import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import {
  ChatsHere,
  fixedChats,
  forgetReadsOutsideChatsHere,
  markOf,
  nothingKnown,
  readsOutsideChatsHere,
  useChatsHere,
  useChatsSelect,
} from "./chatState";

/**
 * **A component that reads the chats with no project above it** (#1037). It draws every chat as
 * "unknown", which is right for a component drawn on its own in a test and wrong in the window,
 * where it would hide what each chat is doing without a word. So the read stays quiet and is
 * counted, and `ChatsHere.window.test.tsx` holds the window to none.
 */
describe("the chats a component reads with no project above it", () => {
  it("are nothing known, and the read is counted", () => {
    forgetReadsOutsideChatsHere();
    const { result } = renderHook(() => useChatsSelect(useChatsHere(), (states) => states));

    expect(result.current).toBe(nothingKnown);
    expect(markOf(result.current, 1, false)).toBe("unknown");
    expect(readsOutsideChatsHere()).toBeGreaterThan(0);
  });

  it("are not counted below a provider, even one holding nothing", () => {
    forgetReadsOutsideChatsHere();
    renderHook(() => useChatsHere(), {
      wrapper: ({ children }) => (
        <ChatsHere.Provider value={fixedChats()}>{children}</ChatsHere.Provider>
      ),
    });

    expect(readsOutsideChatsHere()).toBe(0);
  });
});
