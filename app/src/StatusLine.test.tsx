import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { StatusLine } from "./StatusLine";
import type { Panels, PanelTodo, Piece } from "./bindings";
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
    absent: [],
    refused: [],
    todos: [],
    todos_refused: null,
    personas: [],
    persona: null,
    ...over,
  };
}

function state(over: Partial<WorkspaceState> = {}): WorkspaceState {
  return { pieces: {}, piecesRefused: {}, reading: false, ...over };
}

type Props = Parameters<typeof StatusLine>[0];

/** The line, with everything read and nothing to count unless a test says so. */
function draw(over: Partial<Props> = {}) {
  const props: Props = { plane: PLANE, where: "alpha", workspaces: 2, state: state(), ...over };
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
    draw({ where: undefined, workspaces: undefined });

    expect(words()).toContain("reading the plane…");
    // And `ws 0` is not drawn either, for the same reason: nobody has counted yet.
    expect(screen.queryByTestId("status-workspaces")).toBeNull();
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
  it("says the alert row is not ported rather than showing a zero", () => {
    // charter's alert row is `charter/statusline.py:_alerts` and nothing of it is ported.
    // `footer.rs` names the omission in its own output and `Panels` says the same in prose:
    // an empty list — or a `0` — would be a claim charter has no way to make.
    draw();

    const button = screen.getByRole("button", { name: "Alerts — not drawn by this build" });
    expect(button).toBeDisabled();
    expect(button.getAttribute("title")).toContain("not ported");
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
