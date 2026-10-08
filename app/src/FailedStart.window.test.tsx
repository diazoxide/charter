import { readFileSync } from "node:fs";
import { join } from "node:path";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { FinishedTask, NotStarted, OpenChat } from "./bindings";
import { forgetThisLaunch } from "./regions";

/**
 * **A task that did not start, against the whole window** (#1497, ruling V100-51): it is a
 * failed row under the chat that asked, with the reason in full, and never a banner across the
 * window. The banner is kept for a chat nobody asked for, and there it says the whole reason,
 * one line a chat, with the rest behind "+N more".
 *
 * The core here is a pretend one, as `FinishedTasks.window.test.tsx` has: `finished_tasks`
 * lists the rows the real one reads from its dispatch records, and
 * `chats_that_would_not_start` the chats a launch could not put back.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const STEWARD: OpenChat = {
  session: 4,
  name: "4",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "steward",
  unreported: null,
  card: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

/** The sentence the operator met, as the core says it. */
const WHY =
  "this project runs every chat sandboxed, and this profile's program does not answer as " +
  "Claude Code, whose sandbox it was given, so it was not started sandboxed. Check the " +
  "profile's command in /Users/dev/a/very/long/path/with-no-break-in-it/harness-profiles.toml";

/** A task of the steward chat's that did not start, as its dispatch record says. */
const UNSTARTED: FinishedTask = {
  id: "01K6UNSTARTED",
  asker: 4,
  name: "check prod",
  persona: "devops",
  outcome: "failed",
  folds: false,
  report: `it did not start: ${WHY}`,
  changed: null,
  ended: "2026-10-08T12:04:30+00:00",
  place: "alpha",
  branch: null,
  reopens: false,
  did_not_start: true,
};

const DONE: FinishedTask = {
  ...UNSTARTED,
  id: "01K6DONE",
  name: "live check talk",
  outcome: "done",
  folds: true,
  report: "All good.",
  reopens: true,
  did_not_start: false,
};

/** The core: the steward chat open, `rows` finished under it, `waiting` not put back. */
function core(rows: FinishedTask[], waiting: NotStarted[] = []) {
  let listed = [...rows];
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [STEWARD];
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: ["devops", "steward"],
          persona: "steward",
          unfiled: [],
          workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [STEWARD] }],
        };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return waiting;
      if (cmd === "running_sessions") return [];
      if (cmd === "stopping_chats") return [];
      if (cmd === "finished_tasks") return listed;
      if (cmd === "clear_finished_tasks") {
        const ids = a.ids as string[];
        const before = listed.length;
        listed = listed.filter((row) => !ids.includes(row.id));
        return before - listed.length;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
}

const theirs = () => screen.findByRole("group", { name: "Finished tasks of steward 4" });
/** The finished row named `name`: the button its report opens on. */
const theRow = (group: HTMLElement, name: string) => {
  const found = within(group)
    .getAllByRole("button")
    .find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no finished row is named ${name}`);
  return found;
};
/** The Notices that say a chat did not start, by the chat's id. */
const banners = () =>
  [...document.querySelectorAll<HTMLElement>('[data-cause^="chat-did-not-start:"]')].map(
    (one) => one.dataset.cause,
  );

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("a task that did not start", () => {
  it("is a failed row under the chat that asked, with the reason in full and no banner", async () => {
    core([UNSTARTED, DONE]);
    render(<App />);
    const group = await theirs();

    // Its own row, in the one word for it, and never behind the Finished count.
    const row = theRow(group, "check prod");
    expect(within(row).getByRole("img", { name: "failed" })).toBeInTheDocument();
    expect(within(group).getByRole("button", { name: "Finished (1)" })).toBeInTheDocument();

    // The reason, whole, without a press: every word of the core's sentence.
    const why = within(group).getByRole("region", { name: "Report of check prod" });
    expect(why).toHaveTextContent(`it did not start: ${WHY}`);
    expect(row).toHaveAttribute("aria-expanded", "true");

    // Nothing to resume, and the button says why and not that a harness was at fault.
    const reopen = within(group).getByRole("button", { name: "Reopen check prod" });
    expect(reopen).toBeDisabled();
    expect(reopen).toHaveAttribute(
      "title",
      "It did not start, so there is no conversation to resume.",
    );

    // And no line across the window says it.
    expect(banners()).toEqual([]);
    expect(document.querySelector(".notice-trouble")).toBeNull();
  });

  it("is cleared as any finished row is, and its reason can be folded away", async () => {
    core([UNSTARTED]);
    render(<App />);
    const group = await theirs();
    const row = theRow(group, "check prod");

    await userEvent.click(row);
    expect(within(group).queryByRole("region", { name: "Report of check prod" })).toBeNull();

    await userEvent.click(within(group).getByRole("button", { name: "Clear check prod" }));
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Finished tasks of steward 4" })).toBeNull(),
    );
  });
});

describe("a chat nobody asked for that did not start", () => {
  const waiting = (id: string, name: string): NotStarted => ({
    id,
    name,
    why: WHY,
    approval: null,
  });

  it("still says so under the strip, with the whole reason", async () => {
    core([], [waiting("01K6ROOT", "5")]);
    render(<App />);

    const said = (await screen.findByText(/did not start/)).closest("[data-cause]");
    expect(said).toHaveAttribute("data-cause", "chat-did-not-start:01K6ROOT");
    // Not cut: the sentence is all there, to its last word.
    expect(said).toHaveTextContent(`5 did not start (${WHY}).`);
    expect(within(said as HTMLElement).getByRole("button", { name: "Retry now" })).toBeVisible();
  });

  it("is one line a chat, and the ones that do not fit are behind a count that opens them", async () => {
    core([], [waiting("a", "5"), waiting("b", "6"), waiting("c", "7"), waiting("d", "8")]);
    render(<App />);

    await screen.findAllByText(/did not start/);
    await waitFor(() => expect(banners()).toHaveLength(2));
    const more = screen.getByRole("button", { name: /\+2 more/ });
    await userEvent.click(more);
    // Each failure is a line of its own, with its own reason.
    await waitFor(() => expect(banners()).toHaveLength(4));
    for (const one of document.querySelectorAll('[data-cause^="chat-did-not-start:"]'))
      expect(one).toHaveTextContent(WHY);
  });

  it("is drawn by a rule that wraps a long reason inside the window", () => {
    // jsdom lays nothing out, so the stylesheet is read as text, as `Notice.guard.test.ts`
    // reads it; `e2e/specs/notices.e2e.ts` measures the same line in the real window.
    const css = readFileSync(join(__dirname, "App.css"), "utf8");
    const rules = [...css.matchAll(/(^|\n)(\.notice-band[^{\n]*)\{([^}]*)\}/g)].map((rule) => ({
      selector: rule[2].trim(),
      body: rule[3],
    }));
    const band = rules.find((rule) => rule.selector === ".notice-band");
    expect(band?.body).toMatch(/overflow-wrap:\s*anywhere/);
    // And nothing under the strip is held to one line or cut with an ellipsis.
    for (const rule of rules) {
      expect(rule.body, rule.selector).not.toMatch(/white-space:\s*nowrap/);
      expect(rule.body, rule.selector).not.toMatch(/text-overflow/);
      expect(rule.body, rule.selector).not.toMatch(/line-clamp/);
    }
  });
});
