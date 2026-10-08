import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import type { ListedChat } from "./chatsTree";
import { PaneCrumbs } from "./PaneCrumbs";

/**
 * **A pane's breadcrumb, as it is drawn** (#1486). What it does in the window is
 * `TaskInTab.window.test.tsx`'s; how it fits a narrow pane is `pane-crumbs.e2e.ts`'s, which
 * draws these same elements and classes against the built stylesheet. This file holds the
 * component to that shape, so a class renamed in one place and not the other fails here.
 */

function listed(session: number, name: string, more: Partial<ListedChat> = {}): ListedChat {
  return {
    session,
    name,
    persona: "steward",
    workspace: "alpha",
    shell: false,
    parent: null,
    mode: "task",
    from: null,
    tab: false,
    branch: null,
    report: "sent",
    outcome: "done",
    asking: null,
    harness: "Claude Code",
    ...more,
  };
}

const path = [
  listed(4, "steward 4", { mode: null, report: null, outcome: null, tab: true }),
  listed(9, "talk", { parent: 4 }),
  listed(12, "deep", { parent: 9 }),
];

afterEach(cleanup);

describe("a pane's breadcrumb", () => {
  it("reads the session, each task on the way, the task shown, and its state", () => {
    render(<PaneCrumbs crumbs={{ path, elsewhere: null }} onShow={() => undefined} />);
    const crumbs = screen.getByRole("navigation", { name: "Chat path" });
    expect(crumbs.textContent).toBe("steward 4 › talk › deep · done");
    expect(crumbs.getAttribute("title")).toBe("steward 4 › talk › deep");
    // Each name is read once: a button is named by its own text and by nothing else.
    for (const button of within(crumbs).getAllByRole("button")) {
      expect(button.getAttribute("aria-label")).toBeNull();
      expect(button.getAttribute("title")).toBeNull();
    }
  });

  it("says the workspace beside the task's name, for a task that works in another one", () => {
    render(
      <PaneCrumbs
        crumbs={{ path: path.slice(0, 2), elsewhere: "beta" }}
        onShow={() => undefined}
      />,
    );
    expect(screen.getByRole("navigation").textContent).toBe("steward 4 › talk in beta · done");
  });

  it("says a task the person asked for is theirs, after its state, and in its tooltip", () => {
    // #1492, V100-70. Only the chat shown says it: the path is not whose each chat was.
    const theirs = [path[0], { ...path[1], byYou: true }];
    render(<PaneCrumbs crumbs={{ path: theirs, elsewhere: null }} onShow={() => undefined} />);
    const crumbs = screen.getByRole("navigation", { name: "Chat path" });
    expect(crumbs.textContent).toBe("steward 4 › talk · done · asked by you");
    expect(crumbs.getAttribute("title")).toBe("steward 4 › talk, asked by you");
    // After the state, in a box of its own that gives way before the state does (`App.css`).
    expect(crumbs.lastElementChild?.className).toBe("crumb-by");
    expect(crumbs.querySelector(".shown-state")?.nextElementSibling?.className).toBe("crumb-by");
    cleanup();

    // And where the state is handed in, for a task that has ended.
    render(
      <PaneCrumbs
        crumbs={{ path: theirs, elsewhere: null }}
        onShow={() => undefined}
        state={<span>failed</span>}
      />,
    );
    expect(screen.getByRole("navigation").textContent).toBe(
      "steward 4 › talk · failed · asked by you",
    );
    cleanup();

    // A task under it that its own chat dispatched says nothing of the kind.
    render(
      <PaneCrumbs
        crumbs={{ path: [...theirs, path[2]], elsewhere: null }}
        onShow={() => undefined}
      />,
    );
    expect(screen.getByRole("navigation").textContent).toBe("steward 4 › talk › deep · done");
  });

  it("goes to each chat before the last, and the last is where the person is", async () => {
    const shown = vi.fn();
    render(<PaneCrumbs crumbs={{ path, elsewhere: null }} onShow={shown} />);

    await userEvent.click(screen.getByRole("button", { name: "talk" }));
    await userEvent.click(screen.getByRole("button", { name: "steward 4" }));

    expect(shown.mock.calls).toEqual([[9], [4]]);
    expect(screen.queryByRole("button", { name: "deep" })).toBeNull();
    expect(screen.getByText("deep").getAttribute("aria-current")).toBe("page");
  });

  it("draws a chat on the path that has ended as a name and no way, and the state it is handed", () => {
    render(
      <PaneCrumbs
        crumbs={{ path, elsewhere: null }}
        onShow={() => undefined}
        gone={(session) => session === 9}
        state={<span>failed</span>}
      />,
    );
    const crumbs = screen.getByRole("navigation", { name: "Chat path" });
    expect(within(crumbs).queryByRole("button", { name: "talk" })).toBeNull();
    expect(within(crumbs).getByRole("button", { name: "steward 4" })).toBeTruthy();
    expect(crumbs.textContent).toBe("steward 4 › talk › deep · failed");
  });

  it("is the shape the geometry spec draws by hand: every class here is one the spec names", () => {
    render(<PaneCrumbs crumbs={{ path, elsewhere: "beta" }} onShow={() => undefined} />);
    const crumbs = screen.getByRole("navigation");
    const drawn = new Set(
      [crumbs, ...crumbs.querySelectorAll("*")].flatMap((el) =>
        typeof el.className === "string" ? el.className.split(" ").filter(Boolean) : [],
      ),
    );
    // From the app's folder, which is where the tests are run from.
    const spec = readFileSync(resolve(process.cwd(), "e2e/specs/pane-crumbs.e2e.ts"), "utf8");
    for (const name of drawn) expect(spec, `the spec never draws .${name}`).toContain(name);
  });

  it("is drawn with the elements and classes its stylesheet and its geometry spec name", () => {
    render(<PaneCrumbs crumbs={{ path, elsewhere: "beta" }} onShow={() => undefined} />);
    const crumbs = screen.getByRole("navigation");
    expect(crumbs.className).toBe("pane-crumbs");
    const shape = (el: Element): string =>
      `${el.tagName.toLowerCase()}.${el.className.split(" ").join(".")}`;
    expect([...crumbs.children].map(shape)).toEqual([
      "span.crumb-path",
      "span.crumb-where",
      "span.crumb-sep",
      "span.shown-state",
    ]);
    expect([...(crumbs.querySelector(".crumb-path")?.children ?? [])].map(shape)).toEqual([
      "button.crumb.own",
      "span.crumb-sep",
      "button.crumb.between",
      "span.crumb-sep",
      "span.crumb.shown",
    ]);
    expect([...(crumbs.querySelector(".shown-state")?.children ?? [])].map(shape)).toEqual([
      "span.shape",
      "span.word",
    ]);
    // Every name in the path is reachable by Tab, said out loud for WebKit.
    for (const button of within(crumbs).getAllByRole("button"))
      expect(button.getAttribute("tabindex")).toBe("0");
  });
});
