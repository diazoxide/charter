import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * The opener, and the question in front of it.
 *
 * Two things are being pinned here and they are different in kind. The first is that a window
 * with no project is **usable** — it says which of the two no-project states it is in, and it
 * offers a way out of both. The second is the gate: a project the operator has not approved is
 * described and not opened, the approval carries back what was on screen, and cancelling
 * leaves the window exactly as it was.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

const SIDEBAR = {
  root: "/home/dev/plane",
  workspaces: [],
  personas: [],
  persona: null,
  unfiled: [],
};

/** What a project with something to declare contributes. */
const CONTRIBUTES = {
  plugins: [["superpowers@market", "true"]],
  env: [["ANTHROPIC_BASE_URL", "https://example.invalid"]],
  starts: [['{"program":"/bin/sh","args":[],"cwd":"/home/dev/plane"}', ""]],
  profiles: [],
};

/**
 * A core whose answers a test decides, with every call kept.
 *
 * `answers` is consulted first; anything it does not answer falls through to the shape the
 * window needs to come up at all.
 */
function core(answers: (cmd: string, args: Record<string, unknown>) => unknown) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    const answer = answers(cmd, given);
    if (answer !== undefined) return answer;
    if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
    if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    return null;
  });
  return { asked };
}

/** Types a path into the opener and presses Open. */
async function openByPath(path: string) {
  const person = userEvent.setup();
  await person.type(await screen.findByLabelText("Or type a path"), path);
  await person.click(screen.getByRole("button", { name: "Open" }));
  return person;
}

describe("the opener", () => {
  it("opens a project the operator names, and the window then shows it", async () => {
    const { asked } = core((cmd) => {
      if (cmd === "open_plane") return { plane: "/home/dev/plane", ask: null };
      return undefined;
    });

    render(<App />);
    await openByPath("/home/dev/plane");

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
    expect(asked.find((one) => one.cmd === "open_plane")?.args).toEqual({
      path: "/home/dev/plane",
    });
  });

  it("says what a project contributes instead of opening it, and opens nothing until asked", async () => {
    // charter ADR 0035. `.charter/app/reopen.json` is an execution input and a project is a
    // DIRECTORY, so "open this folder" must not be able to mean "run what is written in it".
    const { asked } = core((cmd) => {
      if (cmd === "open_plane")
        return {
          plane: null,
          ask: {
            path: "/home/dev/stranger",
            contributes: CONTRIBUTES,
            changes: [],
            first: true,
          },
        };
      return undefined;
    });

    render(<App />);
    await openByPath("/home/dev/stranger");

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Open this project?");
    expect(dialog).toHaveTextContent("superpowers@market");
    expect(dialog).toHaveTextContent("ANTHROPIC_BASE_URL=https://example.invalid");
    expect(dialog).toHaveTextContent("/bin/sh");
    // The path charter RESOLVED, because a picker pointed at a subfolder opens the project
    // above it and approving a directory you did not choose is the failure this prevents.
    expect(dialog).toHaveTextContent("/home/dev/stranger");
    expect(asked.some((one) => one.cmd === "approve_plane")).toBe(false);
  });

  it("carries the contribution that was shown back with the approval", async () => {
    // The approval is an answer to the question that was ASKED. Between the dialog reading
    // the project and the button being pressed, anything on the machine can rewrite its
    // settings or its record; the core checks this value against the disk again, and it can
    // only do that if the window hands back what it drew.
    const { asked } = core((cmd) => {
      if (cmd === "open_plane")
        return {
          plane: null,
          ask: { path: "/home/dev/stranger", contributes: CONTRIBUTES, changes: [], first: true },
        };
      if (cmd === "approve_plane") return "/home/dev/stranger";
      return undefined;
    });

    render(<App />);
    await openByPath("/home/dev/stranger");
    await userEvent.setup().click(await screen.findByRole("button", { name: "Open project" }));

    expect(asked.find((one) => one.cmd === "approve_plane")?.args).toEqual({
      path: "/home/dev/stranger",
      contributes: CONTRIBUTES,
    });
    expect(await screen.findByText("/home/dev/stranger")).toBeInTheDocument();
  });

  it("opens nothing when the operator cancels, and leaves the opener where it was", async () => {
    const { asked } = core((cmd) => {
      if (cmd === "open_plane")
        return {
          plane: null,
          ask: { path: "/home/dev/stranger", contributes: CONTRIBUTES, changes: [], first: true },
        };
      return undefined;
    });

    render(<App />);
    await openByPath("/home/dev/stranger");
    await userEvent.setup().click(await screen.findByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(asked.some((one) => one.cmd === "approve_plane")).toBe(false);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "You have not opened a project yet",
    );
  });

  it("reads differently when a project it already approved has started doing more", async () => {
    // An approval is consent to a CONTRIBUTION, not to a path, so a project that gains a
    // plugin after it was approved has been handed a grant nobody looked at. The words for
    // what changed are charter's own; the window does not describe them a second time.
    core((cmd) => {
      if (cmd === "open_plane")
        return {
          plane: null,
          ask: {
            path: "/home/dev/plane",
            contributes: CONTRIBUTES,
            changes: ["the plugin superpowers@market is new"],
            first: false,
          },
        };
      return undefined;
    });

    render(<App />);
    await openByPath("/home/dev/plane");

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("This project has changed since you approved it");
    expect(dialog).toHaveTextContent("the plugin superpowers@market is new");
    expect(screen.getByRole("button", { name: "Open it anyway" })).toBeInTheDocument();
  });

  it("offers the projects this machine remembers, and opens one on a click", async () => {
    const { asked } = core((cmd) => {
      if (cmd === "recent_planes")
        return {
          planes: [
            { path: "/home/dev/one", name: "one", approved: true },
            { path: "/home/dev/two", name: "two", approved: false },
          ],
          dropped: [],
          forgetful: null,
        };
      if (cmd === "open_plane") return { plane: "/home/dev/one", ask: null };
      return undefined;
    });

    render(<App />);
    await userEvent.setup().click(await screen.findByRole("button", { name: /one/ }));

    expect(asked.find((one) => one.cmd === "open_plane")?.args).toEqual({ path: "/home/dev/one" });
    // The row for a project nobody has approved says so, and the approved one does not.
    expect(screen.queryAllByText("charter will ask about this one")).toHaveLength(0);
  });

  it("drops a project that has moved with a line saying so, and never an error", async () => {
    // ADR 0034: the record is a convenience and the project is the truth. An opener that
    // shows one fewer row and says why is usable; a dialog at launch is not.
    core((cmd) => {
      if (cmd === "recent_planes")
        return {
          planes: [],
          dropped: ["/home/dev/gone is no longer there"],
          forgetful: null,
        };
      return undefined;
    });

    render(<App />);

    expect(await screen.findByText("/home/dev/gone is no longer there")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("is a working screen on a machine that keeps no store at all", async () => {
    // Windows: `0600` has no expression there, so charter's guard refuses rather than
    // degrades (ADR 0031) and there is no record of anything. The app still opens projects.
    core((cmd) => {
      if (cmd === "recent_planes")
        return {
          planes: [],
          dropped: [],
          forgetful: "charter keeps no machine store on this platform",
        };
      return undefined;
    });

    render(<App />);

    expect(await screen.findByText(/cannot remember projects on this machine/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Project…" })).toBeInTheDocument();
  });

  it("says why an open did not happen, in the core's own words, and stays open", async () => {
    core((cmd) => {
      if (cmd === "open_plane")
        throw new Error("/home/dev/notes is not a plane: charter found no charter.toml");
      return undefined;
    });

    render(<App />);
    await openByPath("/home/dev/notes");

    expect(await screen.findByRole("alert")).toHaveTextContent("is not a plane");
    expect(screen.getByRole("button", { name: "Open Project…" })).toBeInTheDocument();
  });

  it("comes back to the opener when the project is closed, and can open it again", async () => {
    // The way back. Closing is the window letting go of a project — its chats end and its
    // record is written into it — and nothing of it on disk goes, so the same one opens
    // again with everything still in it. Opening it again has to draw its chats again: a
    // window showing a project with no tabs and no way to get them is the state this whole
    // screen exists to remove.
    const { asked } = core((cmd) => {
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "open_plane") return { plane: "/home/dev/plane", ask: null };
      if (cmd === "opened_chats")
        return [
          {
            session: 1,
            name: "one",
            cwd: null,
            harness: null,
            in_front: true,
            resumed: null,
            fresh: null,
            profile: null,
            persona: null,
            unreported: null,
          },
        ];
      if (cmd === "close_plane") return null;
      return undefined;
    });

    render(<App />);
    const person = userEvent.setup();
    await screen.findByText("/home/dev/plane");
    // Through the catalogue, which is where every button in this window comes from.
    await person.click(await screen.findByRole("button", { name: "Close this project" }));

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "You have not opened a project yet",
    );
    expect(asked.some((one) => one.cmd === "close_plane")).toBe(true);

    await openByPath("/home/dev/plane");

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
    // Its chats are drawn again, which is what a ref still naming the project it just let go
    // of would have silently prevented.
    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "opened_chats").length).toBeGreaterThanOrEqual(2),
    );
  });

  it("draws nothing of the last project's chats before the next one has answered", async () => {
    // The window between the switch and the new project's first answer. It is an `await`
    // wide, and what fills it is whatever the last project said — so the fold that arrives
    // afterwards cannot be the only guard, and this is the one that holds in the meantime.
    core((cmd, args) => {
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/one", from: "/home/dev/one", why: null };
      if (cmd === "open_plane") return { plane: "/home/dev/two", ask: null };
      if (cmd === "chat_states")
        return args.plane === "/home/dev/one"
          ? [{ plane: args.plane, session: 1, state: "waiting", needs_you: true, queue: [1] }]
          : // The second project never answers, so nothing can arrive to correct the screen.
            new Promise(() => undefined);
      // BOTH projects have a chat 1 — which is the whole point: a session number means
      // nothing without its project, and a screen with no second chat 1 on it could not
      // show the last project's state landing on this one's.
      if (cmd === "opened_chats")
        return [
          {
            session: 1,
            name: args.plane === "/home/dev/one" ? "one" : "two",
            cwd: null,
            harness: null,
            in_front: true,
            resumed: null,
            fresh: null,
            profile: null,
            persona: null,
            unreported: null,
          },
        ];
      if (cmd === "close_plane") return null;
      return undefined;
    });

    render(<App />);
    const person = userEvent.setup();
    await vi.waitFor(() =>
      expect(screen.getByRole("img", { name: "waiting on you" })).toBeInTheDocument(),
    );
    await person.click(await screen.findByRole("button", { name: "Close this project" }));
    await openByPath("/home/dev/two");

    await vi.waitFor(() => expect(screen.getByText("/home/dev/two")).toBeInTheDocument());
    expect(screen.queryByRole("img", { name: "waiting on you" })).not.toBeInTheDocument();
  });

  it("draws nothing of the last project's chats once the next one has answered", async () => {
    // Every project numbers its chats from one, so "session 1 is waiting" is a sentence about
    // a pair and not about a number. The window used to keep what it had been told under the
    // next project's first answer — `underneath` never writes over what it finds, which is
    // right for a snapshot racing an event inside one project and wrong across two.
    const one = {
      session: 1,
      name: "one",
      cwd: null,
      harness: null,
      in_front: true,
      resumed: null,
      fresh: null,
      profile: null,
      persona: null,
      unreported: null,
    };
    let showing = "/home/dev/one";
    core((cmd) => {
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/one", from: "/home/dev/one", why: null };
      if (cmd === "open_plane") return { plane: "/home/dev/two", ask: null };
      if (cmd === "opened_chats") return [one];
      // The first project's chat 1 is waiting for the operator; the second project's is not.
      if (cmd === "chat_states")
        return showing === "/home/dev/one"
          ? [{ plane: showing, session: 1, state: "waiting", needs_you: true, queue: [1] }]
          : [];
      if (cmd === "close_plane") {
        showing = "/home/dev/two";
        return null;
      }
      return undefined;
    });

    render(<App />);
    const person = userEvent.setup();
    await vi.waitFor(() =>
      expect(screen.getByRole("img", { name: "waiting on you" })).toBeInTheDocument(),
    );
    await person.click(await screen.findByRole("button", { name: "Close this project" }));
    await openByPath("/home/dev/two");

    await vi.waitFor(() =>
      expect(screen.queryByRole("img", { name: "waiting on you" })).not.toBeInTheDocument(),
    );
  });

  it("tells the core which project this window has in front", async () => {
    // The half charter-app#111 named as missing: every project numbers its chats from one,
    // so a window showing B would otherwise suppress a notification for A's chat 3 on the
    // strength of A's own answer.
    const { asked } = core((cmd) => {
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      return undefined;
    });

    render(<App />);

    // The LAST thing it said, not the first: a window says "no project" before the core has
    // answered which one the launch opened, and the answer that matters is the current one.
    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "window_shows_plane").pop()?.args).toEqual({
        plane: "/home/dev/plane",
      }),
    );
  });
});
