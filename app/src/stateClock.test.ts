import { describe, expect, it } from "vitest";
import { moved, nothingKnown, type ChatStates, type State } from "./chatState";
import { chatsTree, type ListedChat } from "./chatsTree";
import { stateClock } from "./stateClock";

const listed = (session: number): ListedChat => ({
  session,
  name: `chat ${session}`,
  persona: "steward",
  workspace: "alpha",
  shell: false,
  parent: null,
  mode: null,
  from: null,
  tab: true,
  branch: null,
  report: null,
  outcome: null,
  asking: null,
  harness: "claude",
});

/** `states` after chat `session` moved to `state`, as the core's `sequence`-th word. */
const after = (states: ChatStates, session: number, state: State, sequence: number): ChatStates =>
  moved(states, {
    plane: "/plane",
    session,
    state,
    needs_you: false,
    queue: [],
    moved_at: sequence,
    sequence,
    reports: [],
    refusals: [],
    children: [],
    needs: null,
    stopped: null,
  });

describe("how long a chat has been in its state, as the window saw it (V100-19)", () => {
  const rows = chatsTree([listed(1), listed(2)]);

  it("has no time for a chat that was already in its state when the window first read it", () => {
    const clock = stateClock();

    clock.read(after(nothingKnown, 1, "running", 1), rows, 1000);

    expect(clock.since(1)).toBeNull();
    expect(clock.since(2)).toBeNull();
  });

  it("guesses nothing from the first thing it hears of a chat: that state may be hours old", () => {
    const clock = stateClock();
    clock.read(nothingKnown, rows, 1000);

    clock.read(after(nothingKnown, 1, "waiting", 1), rows, 2000);

    expect(clock.since(1)).toBeNull();
  });

  it("times a state from the moment the window saw a chat it had heard from come into it", () => {
    const clock = stateClock();
    let states = after(nothingKnown, 1, "running", 1);
    clock.read(states, rows, 1000);
    states = after(states, 1, "waiting", 2);

    clock.read(states, rows, 5000);
    clock.read(states, rows, 9000);

    expect(clock.since(1)).toBe(5000);
    expect(clock.since(2)).toBeNull();
  });

  it("times a chat that arrived while the window was open from its arrival", () => {
    const clock = stateClock();
    clock.read(nothingKnown, rows, 1000);

    clock.read(nothingKnown, chatsTree([listed(1), listed(2), listed(3)]), 7000);

    expect(clock.since(3)).toBe(7000);
  });

  it("tells its readers when a time changes, and only then", () => {
    const clock = stateClock();
    let told = 0;
    clock.subscribe(() => (told += 1));
    let states = after(nothingKnown, 1, "running", 1);
    clock.read(states, rows, 1000);
    clock.read(states, rows, 2000);
    expect(told).toBe(0);

    states = after(states, 1, "waiting", 2);
    clock.read(states, rows, 3000);

    expect(told).toBe(1);
  });

  it("forgets a chat that is no longer listed", () => {
    const clock = stateClock();
    let states = after(nothingKnown, 1, "running", 1);
    clock.read(states, rows, 1000);
    states = after(states, 1, "waiting", 2);
    clock.read(states, rows, 2000);

    clock.read(states, chatsTree([listed(2)]), 3000);

    expect(clock.since(1)).toBeNull();
  });
});
