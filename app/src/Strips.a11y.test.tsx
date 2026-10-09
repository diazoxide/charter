import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import type { OpenChat } from "./bindings";
import { findStripNamed, stripNamed } from "./test-strips";

/**
 * **A tab strip owns only tabs** (#1204): the rule axe calls `aria-required-children`. A
 * `tablist` may own `tab` elements and nothing else, and what it owns is every element under it
 * in the DOM — through any wrapper with no role of its own — and every element its `aria-owns`
 * names. A `button` it owns is an element of the wrong role, and a screen reader then announces
 * the strip with the wrong count, or as a list of mixed items.
 *
 * axe-core is not a dependency of this app, so the rule is written out here as axe reads it
 * (`ownedByTheRule`), with the cases that decide the design checked against small trees first.
 * **The one that matters for #1204: `aria-owns` adds to what a tablist owns, and never takes its
 * DOM children away.** So a tablist that keeps its cells and lists only the tabs in `aria-owns`
 * still owns every `×` and gear in those cells; the role has to move off an element that holds
 * them, or they have to leave it.
 *
 * **So the role is on an element of its own** (`StripTablist.tsx`): empty, inside the strip,
 * owning the strip's tabs through `aria-owns`. The window's three strips are held to it here:
 * each tablist owns its tabs and nothing else, and the `×`, the gears and the strip's own
 * controls are still drawn in the strip. The list of what a strip owns that is not a tab was
 * debt that #1204 paid off; a control that comes back into a tablist fails here.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => <div>session {session}</div>,
}));

/** The roles an element has without saying, for the elements a strip is built of. */
function implicitRole(element: Element): string | undefined {
  switch (element.tagName.toLowerCase()) {
    case "button":
      return "button";
    case "a":
      return element.hasAttribute("href") ? "link" : undefined;
    case "input": {
      const type = (element.getAttribute("type") ?? "text").toLowerCase();
      if (type === "checkbox") return "checkbox";
      if (type === "radio") return "radio";
      if (type === "button" || type === "submit" || type === "reset") return "button";
      return "textbox";
    }
    case "textarea":
      return "textbox";
    case "select":
      return "combobox";
    case "img":
      return element.getAttribute("alt") === "" ? undefined : "img";
    case "svg":
      return "graphics-document";
    case "nav":
      return "navigation";
    case "ul":
    case "ol":
      return "list";
    case "li":
      return "listitem";
    default:
      return undefined;
  }
}

/** Whether a keyboard or a script can focus it: native controls, and anything with a
 *  `tabindex`, `-1` included — axe counts both. */
function focusable(element: Element): boolean {
  if (element.hasAttribute("tabindex")) return true;
  if ((element as HTMLButtonElement).disabled) return false;
  const tag = element.tagName.toLowerCase();
  if (tag === "a") return element.hasAttribute("href");
  return ["button", "input", "select", "textarea"].includes(tag);
}

/** The ARIA attributes any element may carry, whose presence makes a role-less element an
 *  owned element of its own rather than a wrapper the rule looks through. */
const GLOBAL_ARIA = [
  "aria-atomic",
  "aria-busy",
  "aria-controls",
  "aria-current",
  "aria-describedby",
  "aria-description",
  "aria-details",
  "aria-dropeffect",
  "aria-flowto",
  "aria-grabbed",
  "aria-haspopup",
  "aria-keyshortcuts",
  "aria-label",
  "aria-labelledby",
  "aria-live",
  "aria-owns",
  "aria-relevant",
  "aria-roledescription",
];

/** Hidden from assistive technology, and so from the rule. */
function hidden(element: Element): boolean {
  return (
    element.getAttribute("aria-hidden") === "true" ||
    element.hasAttribute("hidden") ||
    element.hasAttribute("inert")
  );
}

/** The role it is read as: what it says, else what it is. `none` and `presentation` are no role,
 *  unless it is focusable or carries a global attribute, which a browser does not let them hide. */
function roleOf(element: Element): string | undefined {
  const said = element.getAttribute("role")?.trim().split(/\s+/)[0];
  if (said === "none" || said === "presentation") {
    const kept = focusable(element) || GLOBAL_ARIA.some((name) => element.hasAttribute(name));
    return kept ? implicitRole(element) : undefined;
  }
  return said || implicitRole(element);
}

/**
 * **What a tablist owns, as axe's `aria-required-children` reads it**: its DOM children and
 * what its `aria-owns` names; through every one with no role, no global attribute and no way to
 * be focused, to its children in turn; and leaving out what is hidden from assistive technology.
 */
function ownedByTheRule(list: Element): { role: string | undefined; element: Element }[] {
  const named = (list.getAttribute("aria-owns") ?? "")
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => list.ownerDocument.getElementById(id))
    .filter((element): element is HTMLElement => element !== null);
  const queue: Element[] = [...list.children, ...named];
  const owned: { role: string | undefined; element: Element }[] = [];
  for (let at = 0; at < queue.length; at++) {
    const element = queue[at];
    if (hidden(element)) continue;
    const role = roleOf(element);
    const global = GLOBAL_ARIA.some((name) => element.hasAttribute(name));
    if (role === undefined && !global && !focusable(element)) queue.push(...element.children);
    else owned.push({ role, element });
  }
  return owned;
}

/** What a strip owns that is not a tab, each said as the element it is: its tag, its role
 *  when it says one, and its classes — `button.closer` — and a strip's own control its name. */
function notTabs(list: Element): string[] {
  return ownedByTheRule(list)
    .filter(({ role }) => role !== "tab")
    .map(({ element }) => {
      const said = element.getAttribute("role");
      const classes = [...element.classList].map((name) => `.${name}`).join("");
      const own = `${element.tagName.toLowerCase()}${said ? `[role=${said}]` : ""}${classes}`;
      // A strip's own controls by their names too, since they share one look.
      return element.closest(".strip-doing")
        ? `.strip-doing ${own} "${element.getAttribute("aria-label") ?? ""}"`
        : own;
    })
    .sort();
}

/** A small tree to read the rule against. */
function tree(html: string): Element {
  document.body.innerHTML = html;
  const list = document.body.querySelector('[role="tablist"]');
  if (!list) throw new Error("the tree has no tablist");
  return list;
}

afterEach(() => {
  cleanup();
  clearMocks();
  document.body.innerHTML = "";
});

describe("the rule, as axe reads it", () => {
  it("passes a tablist of tabs, through wrappers with no role", () => {
    const list = tree(
      `<div role="tablist"><span class="cell"><button role="tab">a</button></span>` +
        `<button role="tab">b</button></div>`,
    );
    expect(notTabs(list)).toEqual([]);
  });

  it("fails a button beside a tab in its cell, whether or not it is a Tab stop", () => {
    const list = tree(
      `<div role="tablist"><span><button role="tab">a</button>` +
        `<button class="closer" tabindex="-1">×</button></span></div>`,
    );
    expect(notTabs(list)).toEqual(["button.closer"]);
  });

  it("does not let aria-owns take a tablist's own children away", () => {
    const list = tree(
      `<div role="tablist" aria-owns="a"><span><button id="a" role="tab">a</button>` +
        `<button class="gear">⚙</button></span></div>`,
    );
    expect(notTabs(list)).toEqual(["button.gear"]);
  });

  it("passes a tablist that holds no cells and lists its tabs in aria-owns", () => {
    const list = tree(
      `<div><span role="tablist" aria-owns="a b"></span>` +
        `<span role="group"><button id="a" role="tab">a</button><button>×</button></span>` +
        `<span role="group"><button id="b" role="tab">b</button><button>×</button></span></div>`,
    );
    expect(notTabs(list)).toEqual([]);
  });

  it("leaves out what is hidden from assistive technology", () => {
    const list = tree(
      `<div role="tablist"><button role="tab">a</button>` +
        `<button aria-hidden="true" tabindex="-1">×</button></div>`,
    );
    expect(notTabs(list)).toEqual([]);
  });

  it("counts a wrapper that carries a label as an element of its own", () => {
    const list = tree(
      `<div role="tablist"><span aria-label="cell"><button role="tab">a</button></span></div>`,
    );
    expect(notTabs(list)).toEqual(["span"]);
  });
});

const PLANE = "/home/dev/plane";

function workspace(name: string, chats: OpenChat[] = []) {
  return {
    name,
    path: `${PLANE}/workspaces/${name}`,
    vision: "",
    todos: [],
    chats,
    colour: null,
    live: false,
  };
}

function chat(session: number): OpenChat {
  return {
    session,
    name: `alpha.${session}`,
    cwd: `${PLANE}/workspaces/alpha`,
    harness: null,
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    card: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
  };
}

/** A project with two pinned workspaces, the first in front with two chats open in it, so every
 *  strip draws tabs, and the project's and the workspace's gears are drawn. */
function core() {
  mockIPC(
    (cmd) => {
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [chat(1), chat(2)];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "plane_pins")
        return { project: false, workspaces: ["alpha", "beta"], missing: [] };
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: [],
          persona: null,
          unfiled: [],
          workspaces: [workspace("alpha", [chat(1), chat(2)]), workspace("beta")],
        };
      return null;
    },
    { shouldMockEvents: true },
  );
}

describe("the window's three strips", () => {
  it("own only tabs (#1204)", async () => {
    core();
    render(<App />);
    const tablist = (name: string) => screen.getByRole("tablist", { name });
    // Drawn as the test means them: alpha in front with its two chat tabs, and the gears on
    // the strips that have one.
    await findStripNamed("Projects");
    await userEvent.click(
      await within(await findStripNamed("Workspaces")).findByRole("tab", { name: /alpha/ }),
    );
    await vi.waitFor(() => expect(within(stripNamed("Tabs")).getAllByRole("tab")).toHaveLength(2));
    await within(stripNamed("Workspaces")).findByRole("button", { name: /settings/i });
    // What the debt list held is still drawn, and still in its strip: the `×`, the gears and
    // the project strip's own controls moved nowhere.
    expect(
      within(stripNamed("Projects")).getAllByRole("button", { name: /^Close project/ }),
    ).toHaveLength(1);
    expect(
      within(stripNamed("Projects")).getByRole("button", { name: "Open a project…" }),
    ).toBeInTheDocument();
    expect(
      within(stripNamed("Projects")).getByRole("button", { name: "New project…" }),
    ).toBeInTheDocument();
    expect(
      within(stripNamed("Projects")).getByRole("button", { name: /settings/i }),
    ).toBeInTheDocument();
    expect(
      within(stripNamed("Tabs")).getAllByRole("button", { name: /^(End|Close) / }),
    ).toHaveLength(2);

    expect({
      projects: notTabs(tablist("Projects")),
      workspaces: notTabs(tablist("Workspaces")),
      chats: notTabs(tablist("Tabs")),
    }).toEqual({ projects: [], workspaces: [], chats: [] });
    // And each owns every tab its strip draws, in order (`stripNamed` holds it to that).
    for (const name of ["Projects", "Workspaces", "Tabs"])
      expect(ownedByTheRule(tablist(name)).map(({ element }) => element)).toEqual([
        ...stripNamed(name).querySelectorAll('[role="tab"]'),
      ]);
  });

  it("owns no name box while a chat tab is being renamed (#1204)", async () => {
    core();
    render(<App />);
    await userEvent.click(
      await within(await findStripNamed("Workspaces")).findByRole("tab", { name: /alpha/ }),
    );
    await vi.waitFor(() => expect(within(stripNamed("Tabs")).getAllByRole("tab")).toHaveLength(2));

    within(stripNamed("Tabs")).getAllByRole("tab")[0].focus();
    await userEvent.keyboard("{F2}");

    // The name box is drawn in the tab's place, in the strip, and the tablist does not own it.
    expect(await within(stripNamed("Tabs")).findByRole("textbox")).toBeInTheDocument();
    expect(within(stripNamed("Tabs")).getAllByRole("tab")).toHaveLength(1);
    expect(notTabs(screen.getByRole("tablist", { name: "Tabs" }))).toEqual([]);
  });

  it("owns a tab that goes into the background while its name is open (#1204)", async () => {
    core();
    render(<App />);
    await userEvent.click(
      await within(await findStripNamed("Workspaces")).findByRole("tab", { name: /alpha/ }),
    );
    await vi.waitFor(() => expect(within(stripNamed("Tabs")).getAllByRole("tab")).toHaveLength(2));
    within(stripNamed("Tabs")).getAllByRole("tab")[0].focus();
    await userEvent.keyboard("{F2}");
    await within(stripNamed("Tabs")).findByRole("textbox");

    // The core says the chat is wrapping up, from wherever it was asked: its tab becomes a chip,
    // which is a tab again, so the tablist owns it (`stripNamed` holds it to that).
    await act(() => emit("smart-close", { plane: PLANE, session: 1, phase: "sent", record: null }));
    await vi.waitFor(() => expect(document.querySelector(".tab.chip")).not.toBeNull());
    expect(within(stripNamed("Tabs")).getAllByRole("tab")).toHaveLength(2);
  });
});
