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
    const crumbs = screen.getByRole("navigation", {
      name: "Where this pane is: steward 4 › talk › deep",
    });
    expect(crumbs.textContent).toBe("steward 4 › talk › deep · done");
    expect(within(crumbs).getByRole("img", { name: "done" })).toBeTruthy();
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

  it("goes to each chat before the last, and the last is where the person is", async () => {
    const shown = vi.fn();
    render(<PaneCrumbs crumbs={{ path, elsewhere: null }} onShow={shown} />);

    await userEvent.click(screen.getByRole("button", { name: "talk" }));
    await userEvent.click(screen.getByRole("button", { name: "steward 4" }));

    expect(shown.mock.calls).toEqual([[9], [4]]);
    expect(screen.queryByRole("button", { name: "deep" })).toBeNull();
    expect(screen.getByText("deep").getAttribute("aria-current")).toBe("page");
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
