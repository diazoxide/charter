import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";

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
 * **And it lists the debt**, the way `Notice.guard.test.ts` lists its own: every element the
 * three strips own today that is not a tab, exactly, per strip. A new control inside a strip
 * fails here; #1204's fix crosses its entries off, until each list is empty.
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
  mockIPC((cmd) => {
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [chat(1), chat(2)];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "plane_pins") return { project: false, workspaces: ["alpha", "beta"], missing: [] };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: [],
        persona: null,
        unfiled: [],
        workspaces: [workspace("alpha", [chat(1), chat(2)]), workspace("beta")],
      };
    return null;
  });
}

describe("the window's three strips", () => {
  it("own only tabs, but for the debt #1204 is paying off", async () => {
    core();
    render(<App />);
    const strip = (name: string) => screen.getByRole("tablist", { name });
    // Drawn as the test means them: alpha in front with its two chat tabs, and the gears on
    // the strips that have one.
    await screen.findByRole("tablist", { name: "Projects" });
    await userEvent.click(
      await within(await screen.findByRole("tablist", { name: "Workspaces" })).findByRole("tab", {
        name: /alpha/,
      }),
    );
    await vi.waitFor(() => expect(within(strip("Tabs")).getAllByRole("tab")).toHaveLength(2));
    await within(strip("Workspaces")).findByRole("button", { name: /settings/i });

    expect({
      projects: notTabs(strip("Projects")),
      workspaces: notTabs(strip("Workspaces")),
      chats: notTabs(strip("Tabs")),
    }).toEqual({
      // The project's `×` and gear in its cell, and the strip's own controls, which sit inside
      // the tablist so that `useRoom` can measure them (`App.tsx`).
      projects: [
        '.strip-doing button.bare "New project…"',
        '.strip-doing button.bare "Open a project…"',
        "button.closer",
        "button.gear",
      ],
      // The focused workspace's gear in its cell (SE-23).
      workspaces: ["button.gear"],
      // Each chat tab's `×`.
      chats: ["button.closer", "button.closer"],
    });
  });
});
