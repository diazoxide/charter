import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Sidebar } from "./Sidebar";
import { nothingKnown } from "./chatState";
import type { OpenChat, Sidebar as SidebarModel } from "./bindings";

afterEach(cleanup);

/** One chat as the core reports it. The sidebar shows what it is called, not its id. */
function chat(session: number, name: string, cwd: string, on: Partial<OpenChat> = {}): OpenChat {
  return {
    session,
    name,
    cwd,
    harness: null,
    in_front: false,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    ...on,
  };
}

const model: SidebarModel = {
  root: "/home/dev/plane",
  personas: ["devops", "steward"],
  persona: "steward",
  unfiled: [],
  workspaces: [
    {
      name: "alpha",
      path: "/home/dev/plane/workspaces/alpha",
      vision: "Ship the widget",
      todos: ["Review the rollout plan", "Write the migration"],
      chats: [
        chat(1, "ide.1", "/home/dev/plane/workspaces/alpha"),
        chat(2, "ide.2", "/home/dev/plane/workspaces/alpha/svc"),
      ],
    },
    {
      name: "beta",
      path: "/home/dev/plane/workspaces/beta",
      vision: "Retire the old importer",
      todos: [],
      chats: [],
    },
  ],
};

const names = () => screen.getAllByRole("tab").map((el) => el.textContent);

describe("Sidebar", () => {
  it("lists every workspace on the plane", () => {
    render(<Sidebar states={nothingKnown} sidebar={model} focused="alpha" onFocus={() => {}} />);

    expect(names()).toEqual(["alpha", "beta"]);
  });

  it("shows each workspace's chats under it", () => {
    render(<Sidebar states={nothingKnown} sidebar={model} focused="alpha" onFocus={() => {}} />);

    const alpha = screen.getByTestId("workspace-alpha");
    expect(alpha).toHaveTextContent("ide.1");
    expect(alpha).toHaveTextContent("ide.2");
    expect(screen.getByTestId("workspace-beta")).toHaveTextContent("No chats");
  });

  it("shows the focused workspace's todos and the persona a chat would adopt", () => {
    render(<Sidebar states={nothingKnown} sidebar={model} focused="alpha" onFocus={() => {}} />);

    const focused = screen.getByTestId("focused");
    expect(focused).toHaveTextContent("Review the rollout plan");
    expect(focused).toHaveTextContent("Write the migration");
    expect(focused).toHaveTextContent("steward");
  });

  it("shows only the focused workspace's todos, not another's", () => {
    render(<Sidebar states={nothingKnown} sidebar={model} focused="beta" onFocus={() => {}} />);

    const focused = screen.getByTestId("focused");
    expect(focused).not.toHaveTextContent("Review the rollout plan");
    expect(focused).toHaveTextContent("Nothing to do");
  });

  it("says a workspace has no vision rather than leaving the line blank", () => {
    const unset: SidebarModel = {
      ...model,
      workspaces: [
        {
          name: "gamma",
          path: "/home/dev/plane/workspaces/gamma",
          vision: "",
          todos: [],
          chats: [],
        },
      ],
    };

    render(<Sidebar states={nothingKnown} sidebar={unset} focused="gamma" onFocus={() => {}} />);

    expect(screen.getByTestId("workspace-gamma")).toHaveTextContent("No vision yet");
  });

  it("focuses the workspace that was clicked", async () => {
    const focused: string[] = [];
    render(
      <Sidebar
        states={nothingKnown}
        sidebar={model}
        focused="alpha"
        onFocus={(name) => focused.push(name)}
      />,
    );

    await userEvent.click(screen.getByRole("tab", { name: "beta" }));

    expect(focused).toEqual(["beta"]);
  });

  it("says what each chat is doing, beside the chat", () => {
    // The sidebar is where a person looks to see which of fifty chats is which, so it is
    // where the state has to be. It comes from the harness's own hooks and from nothing
    // else (spec decision 3).
    render(
      <Sidebar
        states={{ bySession: { 1: "running", 2: "waiting" }, needsYou: [2] }}
        sidebar={model}
        focused="alpha"
        onFocus={() => {}}
      />,
    );

    const alpha = within(screen.getByTestId("workspace-alpha"));
    expect(alpha.getByRole("img", { name: "running" })).toBeInTheDocument();
    expect(alpha.getByRole("img", { name: "waiting on you" })).toBeInTheDocument();
  });

  it("says a chat whose harness reports nothing is unknown, rather than saying nothing", () => {
    // A harness with no state hook shows `unknown`, LABELLED as unknown — the spec's own
    // words. A blank space would read as "nothing is happening", which charter cannot know.
    render(<Sidebar states={nothingKnown} sidebar={model} focused="alpha" onFocus={() => {}} />);

    expect(
      within(screen.getByTestId("workspace-alpha")).getAllByRole("img", { name: "unknown" }),
    ).toHaveLength(2);
  });

  it("says on the chat what its harness cannot report, rather than leaving it unexplained", () => {
    // #27. A Codex chat reads `unknown` until its first prompt and never says when it waits
    // on an approval. The core says so in a sentence, and the sentence is on the chat — the
    // honest half named where it applies, not left for the operator to guess from a mark.
    const said =
      "Codex says nothing until your first prompt, and nothing at all until you trust " +
      "charter's hooks when Codex asks; it never says when it stops mid-turn for your approval.";
    const codex = {
      ...model,
      workspaces: [
        {
          ...model.workspaces[0],
          chats: [
            chat(1, "ide.1", "/home/dev/plane/workspaces/alpha", {
              harness: "codex",
              unreported: said,
            }),
            chat(2, "ide.2", "/home/dev/plane/workspaces/alpha", { harness: "claude" }),
          ],
        },
      ],
    };

    render(<Sidebar states={nothingKnown} sidebar={codex} focused="alpha" onFocus={() => {}} />);

    const row = screen.getByText("ide.1").parentElement as HTMLElement;
    expect(within(row).getByText(said)).toBeInTheDocument();
    expect(within(row).getByRole("img", { name: "unknown" })).toBeInTheDocument();
    // And only there: a harness that reports everything carries no such sentence.
    const other = screen.getByText("ide.2").parentElement as HTMLElement;
    expect(within(other).queryByText(said)).toBeNull();
  });

  it("shows a chat working outside every workspace rather than dropping it", () => {
    const stray: SidebarModel = {
      ...model,
      unfiled: [chat(9, "stray.1", "/tmp")],
    };

    render(<Sidebar states={nothingKnown} sidebar={stray} focused="alpha" onFocus={() => {}} />);

    expect(screen.getByTestId("unfiled")).toHaveTextContent("stray.1");
  });

  it("names the profile a chat started on, and the persona it adopted", () => {
    // A profile is what the operator picked and what a relaunch looks up again; the kind is
    // what the plane calls the harness. One without the other hides either which account a
    // chat is on or which harness it runs.
    const on = {
      ...model,
      workspaces: [
        {
          ...model.workspaces[0],
          chats: [
            chat(1, "ide.1", "/home/dev/plane/workspaces/alpha", {
              harness: "claude",
              profile: "claude-work",
              persona: "steward",
            }),
          ],
        },
      ],
    };

    render(<Sidebar states={nothingKnown} sidebar={on} focused="alpha" onFocus={() => {}} />);

    const row = screen.getByText("ide.1").parentElement as HTMLElement;
    expect(within(row).getByText(/claude-work/)).toBeInTheDocument();
    expect(within(row).getByText(/\(claude\)/)).toBeInTheDocument();
    expect(within(row).getByText(/steward/)).toBeInTheDocument();
  });
});
