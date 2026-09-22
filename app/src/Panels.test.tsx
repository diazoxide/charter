/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Panels } from "./Panels";
import type { Panels as PanelsModel } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

afterEach(cleanup);

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc", "tool"],
  absent: [],
  refused: [],
  todos: [
    { slug: "20260302-091400-review", title: "Review the rollout plan", stamp: "2026-03-02" },
  ],
  todos_refused: null,
  personas: ["devops", "steward"],
  persona: "steward",
};

function state(on: Partial<WorkspaceState> = {}): WorkspaceState {
  return {
    panels: PANELS,
    repos: { workspace: "alpha", repos: [], cache_refused: null },
    pieces: {},
    piecesRefused: {},
    reading: false,
    ...on,
  };
}

function draw(
  on: {
    workspace?: string;
    state?: WorkspaceState;
    queue?: number[];
    quiet?: string[];
    showChat?: (session: number) => void;
  } = {},
) {
  render(
    <Panels
      workspace={"workspace" in on ? on.workspace : "alpha"}
      state={on.state ?? state()}
      queue={on.queue ?? []}
      quiet={on.quiet ?? []}
      nameOf={(session) => `ide.${session}`}
      showChat={on.showChat ?? (() => {})}
    />,
  );
}

describe("the right-hand region", () => {
  it("holds the needs-you queue, which used to share a line with six other things", () => {
    // charter ADR 0038 moved it here off `<header className="bar">`. The queue itself was
    // already built; where it is was the decision.
    draw({ queue: [3, 7] });

    const queue = within(screen.getByTestId("panels")).getByLabelText("Needs you");
    expect(queue).toHaveTextContent("2 need you");
    expect(within(queue).getByRole("button", { name: "ide.3" })).toBeInTheDocument();
  });

  it("brings a chat forward from the queue", async () => {
    const showChat = vi.fn();
    draw({ queue: [7], showChat });

    await userEvent.click(screen.getByRole("button", { name: "ide.7" }));

    expect(showChat).toHaveBeenCalledWith(7);
  });

  it("draws the queue even when no workspace is focused, because it is not the workspace's", () => {
    // A chat asking for you in a workspace nobody is looking at is exactly the one that must
    // not be hidden — the same hole the project tabs close one scope up.
    draw({ workspace: undefined, queue: [3] });

    expect(screen.getByLabelText("Needs you")).toHaveTextContent("1 need you");
  });

  it("is named for what it is, and not a second answer to which workspace this is", () => {
    // Two `nav[aria-label="Workspaces"]` broke a spec once. The strip is the axis and keeps
    // that name; this region is what is asking for you.
    draw();

    expect(screen.getByTestId("panels")).toHaveAttribute("aria-label", "Attention · alpha");
  });

  // ---------------------------------------------------------------------------------------
  // Alerts, which have a region and nothing to draw
  // ---------------------------------------------------------------------------------------

  it("says the alert row is not ported rather than showing an empty alert area", () => {
    // charter ADR 0038 assigns alerts here and there is no source: `_alerts` lives in
    // `charter/statusline.py` and nothing ports it. An empty area under the heading would
    // claim charter had looked — the same lie `footer.rs` refuses to tell.
    draw();

    const alerts = screen.getByTestId("panel-alerts");
    expect(alerts).toHaveTextContent("Not drawn by this build");
    expect(alerts).not.toHaveTextContent("No alerts");
  });

  it("invents no alert state even when everything else has been read", () => {
    draw({ state: state() });

    expect(screen.getByTestId("panel-alerts")).toHaveTextContent(
      /cannot tell you whether anything is alerting/,
    );
  });

  // ---------------------------------------------------------------------------------------
  // Todos and personas, which stayed
  // ---------------------------------------------------------------------------------------

  it("shows the focused workspace's open todos", () => {
    draw();

    expect(screen.getByTestId("panel-todos")).toHaveTextContent("Review the rollout plan");
  });

  it("says which persona a chat started here would adopt", () => {
    draw();

    const personas = screen.getByTestId("panel-personas");
    expect(personas).toHaveTextContent("steward · default");
    expect(within(personas).getByText("devops")).not.toHaveTextContent("default");
  });

  it("says why the todos could not be read rather than showing none", () => {
    draw({
      state: state({
        panels: { ...PANELS, todos: [], todos_refused: "todos/ resolves outside the plane" },
      }),
    });

    expect(within(screen.getByTestId("panel-todos")).getByRole("alert")).toHaveTextContent(
      "outside",
    );
  });

  it("says the plane is still being read rather than saying there is nothing to do", () => {
    draw({ state: state({ panels: undefined }) });

    expect(screen.getByTestId("panel-todos")).toHaveTextContent("Reading the plane…");
  });

  it("says when the core refused the workspace outright", () => {
    draw({
      workspace: "ghost",
      state: state({ panels: undefined, trouble: "no workspace 'ghost'" }),
    });

    expect(screen.getByRole("alert")).toHaveTextContent("ghost");
  });

  it("draws no workspace answers when none is focused", () => {
    draw({ workspace: undefined });

    expect(screen.getByText("No workspace focused.")).toBeInTheDocument();
    expect(screen.queryByTestId("panel-todos")).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------------------
  // What moved out (charter ADR 0038)
  // ---------------------------------------------------------------------------------------

  it("no longer draws repos or CI, which are state and went to the bottom bar", () => {
    draw();

    expect(screen.queryByTestId("panel-repos")).not.toBeInTheDocument();
    expect(screen.queryByTestId("panel-ci")).not.toBeInTheDocument();
    expect(screen.queryByTestId("repo-svc")).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------------------
  // The personas, and the count that is the point of the window (M6.6)
  // ---------------------------------------------------------------------------------------

  it("marks every persona as one, and the plane's default with a star beside its word", () => {
    draw();

    const personas = within(screen.getByTestId("panel-personas")).getAllByRole("listitem");
    for (const row of personas) expect(row.querySelector("svg.lucide-user-round")).not.toBeNull();
    const [devops, steward] = personas;
    expect(steward).toHaveClass("is-default");
    expect(steward.querySelector(".default svg.lucide-star")).not.toBeNull();
    expect(devops).not.toHaveClass("is-default");
    expect(devops.querySelector("svg.lucide-star")).toBeNull();
  });

  it("draws the count of chats waiting as a number of its own, inside the sentence", () => {
    draw({ queue: [3, 7, 9] });

    const queue = screen.getByLabelText("Needs you");
    // The number is emphasised by being its own element, and the sentence is still one
    // phrase — "3 need you" — for anyone who is read to rather than shown.
    expect(queue.querySelector(".needs-you-number")?.textContent).toBe("3");
    expect(queue.querySelector(".needs-you-count")?.textContent).toBe("3 need you");
    // Every row names its chat and nothing else; the terminal mark is unread.
    expect(
      within(queue).getByRole("button", { name: "ide.7" }).querySelector("svg"),
    ).not.toBeNull();
  });
});

/**
 * **The count's colour is only ever on a pair the contrast suite measures.**
 *
 * `needs-you.base` is `#b85050` in charter-dark, which is 3.64:1 on `surface.base` — under AA
 * for words. It was split from `danger.base` precisely because white on the old shared value
 * failed AA on this very badge. So the words beside the count are `text.primary`, and the
 * number is `needs-you.text` FILLED with `needs-you.base`, which is the pair
 * `contrast.test.ts` holds at 4.5:1. jsdom computes no colour, so this reads the rules.
 */
describe("the needs-you count's colours", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  const rule = (selector: string) =>
    new RegExp(`(?:^|\\})\\s*${selector.replace(/[.]/g, "\\.")}\\s*\\{([^}]*)\\}`).exec(css)?.[1] ??
    "";

  it("never writes the sentence in needs-you.base", () => {
    expect(rule(".needs-you-count")).toMatch(/(?:^|[;\s])color:\s*var\(--text-primary\)/);
    expect(rule(".needs-you-count")).not.toMatch(/(?:^|[;\s])color:\s*var\(--needs-you-base\)/);
  });

  it("fills the number with needs-you.base under needs-you.text, the measured pair", () => {
    expect(rule(".needs-you-number")).toMatch(/background:\s*var\(--needs-you-base\)/);
    expect(rule(".needs-you-number")).toMatch(/(?:^|[;\s])color:\s*var\(--needs-you-text\)/);
  });
});
