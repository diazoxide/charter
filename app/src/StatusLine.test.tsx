import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { StatusLine } from "./StatusLine";
import type { FactBadge, Panels, PanelTodo, Piece } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

/**
 * **The status line's own rules**, on the component and nothing else.
 *
 * The window-level questions — that it is under every region, that the project's path is no
 * longer on the bar — are in `FourRegions.test.tsx` against the whole app, and the ones that
 * need a browser that really lays out are in `app/e2e/specs/status-line.e2e.ts`. What is here
 * is what a props-in/markup-out test can hold: which counts are drawn, which are dropped, and
 * what the alerts button says when charter has no alerts to count.
 */

afterEach(cleanup);

const PLANE = "/home/dev/plane";

function todo(slug: string): PanelTodo {
  return { slug, title: slug, stamp: "2026-09-22" };
}

function piece(name: string): Piece {
  return { piece: name, path: `${PLANE}/${name}`, branch: name, wired: true, stale: false };
}

function panels(over: Partial<Panels> = {}): Panels {
  return {
    workspace: "alpha",
    repos: [],
    paths: {},
    absent: [],
    refused: [],
    todos: [],
    todos_refused: null,
    personas: [],
    persona: null,
    contributed: [],
    ...over,
  };
}

function state(over: Partial<WorkspaceState> = {}): WorkspaceState {
  return { pieces: {}, piecesRefused: {}, reading: false, ...over };
}

type Props = Parameters<typeof StatusLine>[0];

/** The line, with everything read and nothing to count unless a test says so. */
function draw(over: Partial<Props> = {}) {
  const props: Props = {
    plane: PLANE,
    read: true,
    where: "alpha",
    workspaces: 2,
    state: state(),
    ...over,
  };
  return render(<StatusLine {...props} />);
}

/** What the line says, as one string with its spacing normalised. */
const words = () => (screen.getByTestId("status-line").textContent ?? "").replace(/\s+/g, " ");

describe("the status line says where you are", () => {
  it("names the workspace the window is on", () => {
    draw({ where: "alpha" });

    expect(words()).toContain("alpha");
  });

  it("says it is still reading rather than naming no workspace", () => {
    // Before the plane has answered there is no workspace to name, and an empty cell there
    // would read as "you are nowhere" — which is a different claim from "charter has not
    // looked yet".
    draw({ read: false, where: undefined, workspaces: undefined });

    expect(words()).toContain("reading the plane…");
    // And `ws 0` is not drawn either, for the same reason: nobody has counted yet.
    expect(screen.queryByTestId("status-workspaces")).toBeNull();
  });

  it("says the window is on no workspace once the plane has answered with none", () => {
    // A plane that holds no workspaces answers perfectly well. "reading the plane…" under it
    // would be charter waiting forever for something that has already happened.
    draw({ read: true, where: undefined, workspaces: 0 });

    expect(words()).toContain("no workspace");
    expect(words()).not.toContain("reading");
    // Zero IS drawn for `ws`: an empty plane is a fact, not an absence of one.
    expect(within(screen.getByTestId("status-workspaces")).getByText("0")).toBeInTheDocument();
  });

  it("carries the project's directory, whole, as the path it is", () => {
    draw();

    const said = screen.getByTestId("status-line").querySelector("code");
    expect(said?.textContent).toBe(PLANE);
  });

  it("says how many workspaces the project has", () => {
    draw({ workspaces: 7 });

    expect(within(screen.getByTestId("status-workspaces")).getByText("7")).toBeInTheDocument();
  });
});

describe("a count is drawn only when charter can stand behind it", () => {
  it("counts the focused workspace's open todos", () => {
    draw({ state: state({ panels: panels({ todos: [todo("one"), todo("two")] }) }) });

    expect(words()).toContain("todo 2");
  });

  it("draws no todo cell at zero, because presence is the signal", () => {
    // charter's own footer drops the cell rather than printing `todo 0`: a zero present every
    // turn is furniture, and a real `todo 7` in that spot then draws no more attention.
    draw({ state: state({ panels: panels({ todos: [] }) }) });

    expect(screen.queryByTestId("status-todos")).toBeNull();
    expect(words()).not.toContain("todo");
  });

  it("draws no todo cell when the store refused, even though it read some", () => {
    // **The refusal is partial, on purpose.** `todos_refused` says where charter would not
    // look, and it can be set with todos already in hand — so `todos.length` is a real number
    // and a wrong one. A cell drawn from it would be the smaller, quieter answer, and it would
    // contradict the refusal the right-hand region draws in full. A test that paired the
    // refusal with an empty list would pass with this guard deleted, because the zero rule
    // above already drops it: measured, by deleting the guard and watching this stay green.
    draw({
      state: state({
        panels: panels({
          todos: [todo("one"), todo("two")],
          todos_refused: "one todo store is a link out of the plane",
        }),
      }),
    });

    expect(screen.queryByTestId("status-todos")).toBeNull();
  });

  it("counts every clone's worktrees together", () => {
    draw({
      state: state({
        panels: panels({ repos: ["svc", "tool"] }),
        pieces: { svc: [piece("one"), piece("two")], tool: [piece("three")] },
      }),
    });

    expect(words()).toContain("pieces 3");
  });

  it("draws no piece count while a clone has not answered yet", () => {
    // `worktree_list` runs per clone and comes back per clone. A total taken with `tool` still
    // listing is smaller than the truth and carries no mark saying so.
    draw({
      state: state({
        panels: panels({ repos: ["svc", "tool"] }),
        pieces: { svc: [piece("one"), piece("two")] },
      }),
    });

    expect(screen.queryByTestId("status-pieces")).toBeNull();
  });

  it("draws no piece count when a clone's listing was refused", () => {
    draw({
      state: state({
        panels: panels({ repos: ["svc", "tool"] }),
        pieces: { svc: [piece("one")] },
        piecesRefused: { tool: "not a git repository" },
      }),
    });

    expect(screen.queryByTestId("status-pieces")).toBeNull();
  });

  it("draws no piece count for a workspace whose clones have none", () => {
    draw({ state: state({ panels: panels({ repos: ["svc"] }), pieces: { svc: [] } }) });

    expect(screen.queryByTestId("status-pieces")).toBeNull();
  });
});

describe("the alerts button", () => {
  it("is disabled and says so when no drawer is behind it", () => {
    // A control that answered a press with nothing would teach the operator that alerts are
    // quiet. The window always wires the drawer; a status line drawn on its own does not.
    draw();

    const button = screen.getByRole("button", { name: "Alerts — nothing to open here" });
    expect(button).toBeDisabled();
  });

  it("drops a count charter cannot stand behind and still opens the drawer", async () => {
    // Not every project has answered, or charter stopped looking in one: a partial total is a
    // wrong total. The dash is not a zero, and the drawer is where the reason is.
    const open = vi.fn();
    draw({ alerts: { count: undefined, open } });

    const button = screen.getByRole("button", { name: "Alerts: not counted" });
    expect(button).toBeEnabled();
    expect(button).not.toHaveTextContent(/\d/);
    expect(button).toHaveTextContent("—");
    await userEvent.click(button);
    expect(open).toHaveBeenCalledTimes(1);
  });

  it("draws no badge at zero, and says none to a screen reader", () => {
    // The footer's rule: a `0` there every day is furniture, and a real `2` in that spot then
    // draws no more attention than the zero did.
    draw({ alerts: { count: 0, open: vi.fn() } });

    const button = screen.getByRole("button", { name: "Alerts: none" });
    expect(button).toBeEnabled();
    expect(button).not.toHaveTextContent("0");
    expect(button).not.toHaveTextContent("—");
  });

  it("draws the count and opens the drawer once there is a source for one", async () => {
    // The seam M6.5 attaches to. Tested now so that the drawer's arrival is a prop and not a
    // change to this file.
    const open = vi.fn();
    draw({ alerts: { count: 3, open } });

    const button = screen.getByRole("button", { name: "Alerts: 3" });
    expect(button).toBeEnabled();
    await userEvent.click(button);

    expect(open).toHaveBeenCalledTimes(1);
  });
});

describe("the region toggles", () => {
  // The operator asked four times, with a screenshot of Zed: the buttons that show and hide the
  // regions are at the status line's LEFT edge. #207 put them at the right-hand end; this is
  // what keeps them from going back there.
  it("are the first thing on the line, at its left edge", () => {
    const onToggle = vi.fn();
    draw({
      regions: {
        placed: [
          { id: "explorer", side: "left", order: 0, collapsed: false },
          { id: "aside", side: "right", order: 0, collapsed: true },
        ],
        onToggle,
      },
    });
    const line = screen.getByTestId("status-line");
    const first = line.firstElementChild;
    expect(first?.className).toBe("regions-doing");
    const buttons = within(first as HTMLElement).getAllByRole("button");
    expect(buttons.map((b) => b.getAttribute("aria-label"))).toEqual(["Explorer", "Attention"]);
  });
});

describe("an extension's badges (charter-app#340)", () => {
  const badge = (over: Partial<FactBadge> = {}): FactBadge => ({
    extension: "prs",
    name: "Pull requests",
    id: "open",
    label: "PRs",
    value: "3",
    age_seconds: 20,
    stale: false,
    ...over,
  });

  it("draws each badge's label and value, from its facts file", () => {
    draw({ badges: [badge()] });

    const drawn = screen.getByTestId("status-badge-prs-open");
    expect(drawn).toHaveTextContent("PRs 3");
    expect(drawn).not.toHaveClass("stale");
    expect(drawn.getAttribute("title")).toContain("Pull requests");
  });

  it("dims a value older than its extension declared fresh, and says how old it is", () => {
    draw({ badges: [badge({ stale: true, age_seconds: 7200 })] });

    const drawn = screen.getByTestId("status-badge-prs-open");
    expect(drawn).toHaveClass("stale");
    expect(drawn).toHaveTextContent("PRs 3 · 2h ago");
  });

  it("says why an extension shows nothing, rather than letting its badge vanish", () => {
    draw({ factNotes: ["Pull requests changed since you approved it"] });

    const said = screen.getByTestId("status-fact-notes");
    expect(said).toHaveTextContent("1 extension note");
    expect(said.getAttribute("title")).toBe("Pull requests changed since you approved it");
  });

  it("draws no badge when no extension has one", () => {
    draw({ badges: [] });

    expect(screen.queryByTestId(/^status-badge-/)).toBeNull();
  });
});
