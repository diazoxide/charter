import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { ChatBlocked, GrantLevel } from "./bindings";
import type { TaskBlockGroup } from "./taskAsks";
import { TaskBlocksNotice } from "./TaskBlocksNotice";

/**
 * **The question for several tasks says what the single-chat Notice says** (#1709): a held
 * connection waits on it, what policy ruled out is said, and its scopes come in the same order.
 */

const PLANE = "/home/dev/plane";

const clock = { now: 1_000_000 };
beforeEach(() => {
  clock.now = 1_000_000;
  vi.spyOn(Date, "now").mockImplementation(() => clock.now);
});
afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

function blocked(session: number, over: Partial<ChatBlocked> = {}): ChatBlocked {
  return {
    plane: PLANE,
    session,
    operation: "connect",
    kind: "host",
    ours: false,
    harness: "claude",
    said: "a connection to a host",
    offer: "host",
    target: "registry.npmjs.org",
    route: null,
    levels: ["chat", "you", "project"],
    held: false,
    ruled: null,
    ...over,
  };
}

function group(
  offer: "host" | "write",
  levels: GrantLevel[],
  over: (session: number) => Partial<ChatBlocked> = () => ({}),
): TaskBlockGroup {
  const target = offer === "host" ? "registry.npmjs.org" : "/work/cache";
  return {
    key: `${offer}:${target}`,
    session: 1,
    offer,
    target,
    said: offer === "host" ? "a connection to a host" : "a write to a folder",
    levels,
    members: [4, 5].map((session) => ({
      session,
      whose: session === 4 ? "talk" : "sweep",
      block: blocked(session, { offer, target, levels, ...over(session) }),
    })),
  };
}

function shown(of: TaskBlockGroup) {
  render(
    <TaskBlocksNotice plane={PLANE} group={of} onAnswered={vi.fn()} onKeepBlocked={vi.fn()} />,
  );
  return screen.getByRole("status", { name: "Sandbox block for 2 tasks" });
}

const buttons = (within_: HTMLElement) =>
  within(within_)
    .getAllByRole("button")
    .map((one) => one.textContent);

describe("the question for several tasks (#1709)", () => {
  it("offers a host for this machine first, then only these tasks or everyone, as one chat's does", async () => {
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      return { answered: [4, 5], said: "Allowed." };
    });
    const question = shown(group("host", ["chat", "you", "project"]));
    expect(buttons(question)).toEqual([
      "Allow for me on this machine",
      "Other scopes…",
      "Keep blocked",
    ]);
    await userEvent.click(within(question).getByRole("button", { name: "Other scopes…" }));
    // Under the menu: only these tasks, then everyone in the project.
    expect(
      screen
        .getAllByRole("button")
        .map((one) => one.textContent)
        .filter((label) => label !== null && !buttons(question).includes(label)),
    ).toEqual(["Allow only for these 2 tasks", "Allow for everyone in this project"]);
    clock.now += 5_000;
    await userEvent.click(screen.getByRole("button", { name: "Allow only for these 2 tasks" }));
    await waitFor(() =>
      expect(
        asked
          .filter((one) => one.cmd === "allow_sandbox_block_for_tasks")
          .map((one) => (one.args as { level: GrantLevel }).level),
      ).toEqual(["chat"]),
    );
  });

  it("offers a folder for these tasks first, then this machine, and never the project", async () => {
    const question = shown(group("write", ["chat", "you"]));
    expect(buttons(question)).toEqual([
      "Allow only for these 2 tasks",
      "Always allow…",
      "Keep blocked",
    ]);
    await userEvent.click(within(question).getByRole("button", { name: "Always allow…" }));
    expect(screen.getByRole("button", { name: "Allow for me on this machine" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Allow for everyone in this project" })).toBeNull();
  });

  it("offers only the scopes policy leaves open, and says what it ruled out", () => {
    const ruled = "Your administrator's policy does not let a host be allowed for everyone.";
    const question = shown(group("host", ["chat"], () => ({ ruled })));
    expect(buttons(question)).toEqual(["Allow only for these 2 tasks", "Keep blocked"]);
    expect(question.textContent).toContain(ruled);
    // Said once, though each task's block says it.
    expect(question.textContent?.split(ruled).length).toBe(2);
  });

  it("says each command waits while the person answers, where every connection is held", () => {
    const question = shown(group("host", ["chat", "you"], () => ({ held: true })));
    expect(question.textContent).toContain(
      "Each one's command is waiting on a connection to a host: purlis holds the connection",
    );
    expect(question.textContent).not.toContain("The sandbox blocked");
  });

  it("names the tasks that wait, where only some connections are held", () => {
    const question = shown(group("host", ["chat", "you"], (session) => ({ held: session === 5 })));
    expect(question.textContent).toContain("sweep is waiting");
    expect(question.textContent).toContain("The sandbox blocked it in the others.");
  });

  it("says the sandbox blocked it, where no connection is held", () => {
    const question = shown(group("host", ["chat", "you"]));
    expect(question.textContent).toContain("The sandbox blocked a connection to a host in each.");
    expect(question.textContent).not.toContain("waiting");
  });
});
