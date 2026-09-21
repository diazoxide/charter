import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Explorer, type Spot } from "./Explorer";
import { nothingKnown } from "./chatState";
import type { OpenChat, Panels as PanelsModel, Piece } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

afterEach(cleanup);

const ALPHA = "/home/dev/plane/workspaces/alpha";
const CUT = `${ALPHA}/.worktrees/svc`;

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc", "tool"],
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: ["steward"],
  persona: "steward",
};

function piece(name: string, on: Partial<Piece> = {}): Piece {
  return { piece: name, path: `${CUT}/${name}`, branch: name, wired: true, stale: false, ...on };
}

function chat(session: number, name: string, cwd: string, on: Partial<OpenChat> = {}): OpenChat {
  return {
    session,
    name,
    cwd,
    harness: "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: null,
    unreported: null,
    pinned: false,
    ...on,
  };
}

function state(on: Partial<WorkspaceState> = {}): WorkspaceState {
  return {
    panels: PANELS,
    repos: { workspace: "alpha", repos: [], cache_refused: null },
    pieces: { svc: [piece("one"), piece("two")], tool: [] },
    piecesRefused: {},
    reading: false,
    ...on,
  };
}

function draw(on: {
  state?: WorkspaceState;
  chats?: OpenChat[];
  spot?: Spot;
  onPick?: (spot: Spot | undefined) => void;
  onShowChat?: (session: number) => void;
  workspace?: string;
}) {
  render(
    <Explorer
      workspace={"workspace" in on ? on.workspace : "alpha"}
      state={on.state ?? state()}
      chats={on.chats ?? []}
      states={nothingKnown}
      spot={on.spot}
      onPick={on.onPick ?? (() => {})}
      onShowChat={on.onShowChat ?? (() => {})}
    />,
  );
}

/** The spot marked as the one the next chat would start in. */
const picked = () =>
  screen
    .getAllByRole("button")
    .filter((one) => one.getAttribute("aria-current") === "true")
    .map((one) => one.textContent);

describe("the explorer", () => {
  it("lists the focused workspace's clones and the worktrees cut off each", () => {
    draw({});

    const svc = screen.getByTestId("clone-svc");
    expect(within(svc).getByRole("button", { name: /one/ })).toBeInTheDocument();
    expect(within(svc).getByRole("button", { name: /two/ })).toBeInTheDocument();
    expect(screen.getByTestId("clone-tool")).toHaveTextContent("No worktrees cut here");
  });

  it("does not list every workspace, because the strip above already answers that", () => {
    // charter ADR 0038: the sidebar used to draw every workspace with its vision text, under
    // the strip that had just been made the axis. That duplication is what this region
    // replaced, and a test is the only thing that keeps it replaced.
    draw({});

    expect(screen.queryByText("beta")).not.toBeInTheDocument();
    expect(screen.getByTestId("explorer")).not.toHaveTextContent("vision");
  });

  it("starts on the workspace's own directory, so a plain New tab opens where it always did", () => {
    draw({});

    expect(picked()).toEqual([expect.stringContaining("alpha")]);
  });

  it("hands back the path the core spelled when a worktree is picked", async () => {
    // The window never joins a path together. `worktree_list` answers with one, and that is
    // what the next chat is started in.
    const onPick = vi.fn();
    draw({ onPick });

    await userEvent.click(screen.getByRole("button", { name: /^one/ }));

    expect(onPick).toHaveBeenCalledWith({ repo: "svc", piece: "one", path: `${CUT}/one` });
  });

  it("marks the picked worktree, and only it", () => {
    draw({ spot: { repo: "svc", piece: "two", path: `${CUT}/two` } });

    expect(picked()).toEqual([expect.stringContaining("two")]);
  });

  it("goes back to the workspace when its own row is picked", async () => {
    const onPick = vi.fn();
    draw({ spot: { repo: "svc", piece: "two", path: `${CUT}/two` }, onPick });

    await userEvent.click(screen.getByRole("button", { name: /the workspace itself/ }));

    expect(onPick).toHaveBeenCalledWith(undefined);
  });

  it("says a worktree has no charter layer before a chat is started in it", () => {
    // `unwired` is what the operator has to see BEFORE they click: a chat started in such a
    // tree runs with none of the plane's ask/deny rules and no persona agents.
    draw({ state: state({ pieces: { svc: [piece("one", { wired: false })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("unwired");
  });

  it("says a registration whose directory is gone is stale", () => {
    draw({ state: state({ pieces: { svc: [piece("one", { stale: true })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("stale");
  });

  it("names the branch each worktree is on", () => {
    draw({ state: state({ pieces: { svc: [piece("one", { branch: "fix/login" })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("fix/login");
  });

  it("says why a clone's worktrees could not be listed rather than showing none", () => {
    draw({
      state: state({
        pieces: { tool: [] },
        piecesRefused: { svc: "charter will not run git through a symlink" },
      }),
    });

    const svc = screen.getByTestId("clone-svc");
    expect(within(svc).getByRole("alert")).toHaveTextContent("symlink");
    expect(svc).not.toHaveTextContent("No worktrees cut here");
  });

  it("says the listing is still coming rather than saying there is nothing", () => {
    draw({ state: state({ pieces: {} }) });

    expect(screen.getByTestId("clone-svc")).toHaveTextContent("Asking git…");
  });

  // ---------------------------------------------------------------------------------------
  // What is already running where
  // ---------------------------------------------------------------------------------------

  it("shows the chats working in a worktree under that worktree", () => {
    draw({ chats: [chat(1, "ide.1", `${CUT}/one/deep`), chat(2, "ide.2", ALPHA)] });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("ide.1");
    expect(screen.getByTestId("piece-svc-one")).not.toHaveTextContent("ide.2");
  });

  it("shows a chat that is in no worktree under the workspace itself", () => {
    draw({ chats: [chat(2, "ide.2", `${ALPHA}/svc`)] });

    // Not inside any `piece-…` row: it works in the clone, not in a piece of it.
    expect(screen.getByTestId("explorer")).toHaveTextContent("ide.2");
    expect(screen.getByTestId("piece-svc-one")).not.toHaveTextContent("ide.2");
  });

  it("does not put a chat in a worktree whose name merely starts the same way", () => {
    // By path components and never by string prefix: `…/one-more` starts with `…/one`.
    draw({ chats: [chat(3, "ide.3", `${CUT}/one-more`)] });

    expect(screen.getByTestId("piece-svc-one")).not.toHaveTextContent("ide.3");
  });

  it("brings a chat forward when its row is pressed", async () => {
    const onShowChat = vi.fn();
    draw({ chats: [chat(1, "ide.1", `${CUT}/one`)], onShowChat });

    await userEvent.click(screen.getByRole("button", { name: /ide\.1/ }));

    expect(onShowChat).toHaveBeenCalledWith(1);
  });

  it("says on a chat what its harness cannot report, rather than leaving it unexplained", () => {
    draw({
      chats: [
        chat(1, "ide.1", `${CUT}/one`, {
          unreported: "codex does not report an approval prompt",
        }),
      ],
    });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("does not report");
  });

  // ---------------------------------------------------------------------------------------
  // Nothing to explore
  // ---------------------------------------------------------------------------------------

  it("says so when the strip is on the chats that are in no workspace", () => {
    draw({ workspace: undefined });

    expect(screen.getByTestId("explorer")).toHaveTextContent("nothing to explore");
  });

  it("says a workspace holds no repos rather than drawing an empty region", () => {
    draw({ state: state({ panels: { ...PANELS, repos: [] }, pieces: {} }) });

    expect(screen.getByTestId("explorer")).toHaveTextContent("No repos in this workspace");
  });

  it("names a repo the manifest claims that nobody has cloned", () => {
    draw({ state: state({ panels: { ...PANELS, repos: ["svc"], absent: ["later"] } }) });

    expect(screen.getByTestId("absent")).toHaveTextContent("later");
    // And it is not a heading with worktrees under it: there is nothing to cut one from.
    expect(screen.queryByTestId("clone-later")).not.toBeInTheDocument();
  });

  it("shows what charter would not read instead of a workspace with fewer repos in it", () => {
    draw({
      state: state({
        panels: {
          ...PANELS,
          repos: ["svc"],
          refused: [["alias", "'alias' is reached through a symlink"]],
        },
      }),
    });

    expect(screen.getByRole("alert")).toHaveTextContent("alias");
  });
});
