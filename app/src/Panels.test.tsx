/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Panels } from "./Panels";
import { catalogue, catalogued, type Catalogued, type Offer } from "./actions";
import { noTabs } from "./tabs";
import type { Panels as PanelsModel, PersonaDetails } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The project these answers are for. A persona belongs to a plane, and a window holds
 *  several: two of them can both have a `steward`. */
const PLANE = "/home/dev/plane";

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

/**
 * The panel, with the window's own state around it.
 *
 * **Which persona's card is open belongs to the window now** (charter-app#174): opening it is
 * a catalogue row, so a context menu and the palette can open it from outside this panel. The
 * wrapper here is that window, in as few lines as the claim needs — without it these tests
 * would be asserting that a card opens against a prop that never changes.
 */
function draw(
  on: {
    workspace?: string;
    state?: WorkspaceState;
    queue?: number[];
    quiet?: string[];
    showChat?: (session: number) => void;
    offers?: Catalogued;
    onPress?: (offer: Offer) => void;
  } = {},
) {
  function Window() {
    const [shownPersona, setShownPersona] = useState<string>();
    return (
      <Panels
        plane={PLANE}
        workspace={"workspace" in on ? on.workspace : "alpha"}
        state={on.state ?? state()}
        queue={on.queue ?? []}
        quiet={on.quiet ?? []}
        nameOf={(session) => `ide.${session}`}
        showChat={on.showChat ?? (() => {})}
        offers={on.offers ?? new Map()}
        onPress={on.onPress ?? (() => {})}
        shownPersona={shownPersona}
        onShowPersona={setShownPersona}
      />
    );
  }
  render(<Window />);
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
  // Alerts, which are the window's now
  // ---------------------------------------------------------------------------------------

  it("holds no alerts section, because alerts cross projects and this region is one project's", () => {
    // They moved to the window's drawer, opened from the status line (`AlertsDrawer.tsx`). A
    // second, per-project copy here would be a count of one plane's alerts drawn beside a
    // button counting all of them — two numbers for one question.
    draw({ state: state() });

    expect(screen.queryByTestId("panel-alerts")).toBeNull();
    expect(screen.getByTestId("panels")).not.toHaveTextContent(/alert/i);
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

// ---------------------------------------------------------------------------------------
// What a persona row opens (the operator: *"personas list in right sidebar is just texts,
// without click action"*)
// ---------------------------------------------------------------------------------------

/** A definition the core would have answered with. */
function definition(on: Partial<PersonaDetails> = {}): PersonaDetails {
  return {
    name: "devops",
    role: "DevOps Engineer",
    delegate_when: "CI/CD pipelines, k8s deploys",
    tools: ["kubectl", "glab"],
    vault: "devops",
    declares_no_vault: false,
    lineage: ["devops"],
    file: "personas/devops/persona.md",
    ...on,
  };
}

/** The core, answering `persona_details` and counting what it was asked. */
function core(answer: (persona: string) => unknown): { asked: Record<string, unknown>[] } {
  const asked: Record<string, unknown>[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    if (cmd !== "persona_details") return undefined;
    asked.push(given);
    return answer(String(given.persona));
  });
  return { asked };
}

/** Opens a persona's row and waits for the card it opens. */
async function open(persona: string): Promise<HTMLElement> {
  const user = userEvent.setup();
  await user.click(
    within(screen.getByTestId("panel-personas")).getByRole("button", { name: new RegExp(persona) }),
  );
  return waitFor(() => screen.getByTestId(`persona-details-${persona}`));
}

describe("a persona's details", () => {
  it("shows what the definition says: its role, when to delegate to it, its tools and its vault", async () => {
    core(() => definition());
    draw();

    const card = await open("devops");

    expect(card).toHaveTextContent("DevOps Engineer");
    expect(card).toHaveTextContent("CI/CD pipelines, k8s deploys");
    expect(card).toHaveTextContent("kubectl, glab");
    expect(card).toHaveTextContent("devops");
    expect(card).toHaveTextContent("personas/devops/persona.md");
  });

  it("asks the core about this plane's persona, and asks again the next time it is opened", async () => {
    // A definition is a file an operator edits while charter is running — `charter persona
    // create` is how one arrives — so a card that answered from the first read would show a
    // role that was corrected an hour ago.
    const { asked } = core(() => definition());
    draw();

    await open("devops");
    await userEvent.setup().keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("persona-details-devops")).toBeNull());
    await open("devops");

    expect(asked).toEqual([
      { plane: PLANE, persona: "devops" },
      { plane: PLANE, persona: "devops" },
    ]);
  });

  it("says a persona holds no credentials only where it says so itself", async () => {
    // **Three answers and not two.** `vault: none` is a declaration; no `vault:` line at all
    // is charter-app not having looked — charter's own `vault_of` falls back to the vault
    // registry, and nothing in Rust reads that yet. Rounding the second down to the first
    // would have the window claim a persona holds no credentials on nobody's authority.
    core((persona) =>
      persona === "steward"
        ? definition({ name: "steward", vault: null, declares_no_vault: true })
        : definition({ name: "devops", vault: null, declares_no_vault: false }),
    );
    draw();

    expect(await open("steward")).toHaveTextContent("holds no credentials");
    await userEvent.setup().keyboard("{Escape}");

    const devops = await open("devops");
    expect(devops).toHaveTextContent("not declared");
    expect(devops).not.toHaveTextContent("holds no credentials");
  });

  it("names the vault and nothing that is in it", async () => {
    // charter refuses a secret by kind and never echoes one, and a panel gets no exception:
    // the card says WHICH vault a chat as this persona would open, and the word beside it
    // says that is all it says.
    core(() => definition({ vault: "devops" }));
    draw();

    const card = await open("devops");

    expect(card.querySelector("code")?.textContent).toBe("devops");
    expect(card).toHaveTextContent("what is in it is never shown here");
  });

  it("draws the chain a persona inherits from, child first, and only when there is one", async () => {
    core((persona) =>
      persona === "devops"
        ? definition({ lineage: ["devops", "base"] })
        : definition({ name: "steward", lineage: ["steward"] }),
    );
    draw();

    expect(await open("devops")).toHaveTextContent("devops → base");
    await userEvent.setup().keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("persona-details-devops")).toBeNull());

    expect(await open("steward")).not.toHaveTextContent("Inherits");
  });

  it("says a definition that declares no delegate-when routes nothing to itself", async () => {
    // `delegate-when` is what makes a persona findable: it becomes the description whoever
    // is routing reads. A card that drew nothing there would look like a persona that had
    // one rather than a persona that needs one.
    core(() => definition({ delegate_when: null }));
    draw();

    expect(await open("devops")).toHaveTextContent("nothing declared");
  });

  it("draws the core's own refusal rather than an empty card", async () => {
    core(() => {
      throw new Error("persona 'devops' does not load from personas/devops/persona.md");
    });
    draw();

    const card = await open("devops");

    expect(within(card).getByRole("alert")).toHaveTextContent("does not load from");
  });

  it("says the plane's default is the one a chat started here adopts", async () => {
    core(() => definition({ name: "steward" }));
    draw();

    expect(await open("steward")).toHaveTextContent("default");
  });

  it("is not modal, so the queue this region exists for stays reachable", async () => {
    // charter ADR 0038: nothing in this region may compete with the needs-you queue. A Radix
    // DIALOG marks everything outside itself `aria-hidden`, which would take the queue off
    // the accessibility tree while somebody read a persona's role.
    core(() => definition());
    draw({ queue: [3] });

    await open("devops");

    expect(
      within(screen.getByLabelText("Needs you")).getByRole("button", { name: "ide.3" }),
    ).toBeInTheDocument();
  });

  it("closes on Escape and puts the keyboard back on the row", async () => {
    core(() => definition());
    draw();

    const row = within(screen.getByTestId("panel-personas")).getByRole("button", {
      name: /devops/,
    });
    await open("devops");
    await userEvent.setup().keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByTestId("persona-details-devops")).toBeNull());
    expect(row).toHaveFocus();
  });
});

/**
 * Right-click on a persona row (charter-app#174).
 *
 * **In jsdom, because a WebDriver right-click sends no `contextmenu` event** — measured on
 * both engines and recorded in `docs/ui-primitives.md`. What is asserted is the wiring: the
 * row has charter's own menu and it lists the catalogue's row for that persona. What the row
 * SAYS is `actions.test.ts`'s.
 *
 * The one row is the point rather than a shortfall. A persona is a file `charter persona
 * create` writes and an operator edits; reading it is the whole of what this window can do to
 * one, and a menu with three invented verbs would be the second list `actions.ts` refuses.
 */
describe("a persona row's menu", () => {
  const offers = () =>
    catalogued(
      catalogue({
        tabs: noTabs(),
        workspaces: [],
        plane: PLANE,
        personas: ["devops", "steward"],
        needsYou: [],
        nameOf: String,
      }),
    );

  /** Right-clicks a persona row, the way a WebView's own pointer does. */
  function rightClick(persona: string) {
    const row = within(screen.getByTestId("panel-personas")).getByRole("button", {
      name: new RegExp(persona),
    });
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  }

  it("opens on a right-click with the catalogue's row for that persona", async () => {
    core(() => definition());
    draw({ offers: offers() });

    rightClick("devops");

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual(["Show what devops is"]);
  });

  it("hands back the row that opens the card, which is the state the window holds", async () => {
    // The card's openness is the window's now, so a row run from anywhere — this menu, the
    // palette, the row's own click — lands in one place. That is what a catalogue row buys,
    // and it is why `PersonaRow` no longer owns whether it is open.
    core(() => definition());
    const pressed: Offer[] = [];
    draw({ offers: offers(), onPress: (offer) => pressed.push(offer) });

    rightClick("devops");
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "Show what devops is" }));

    expect(pressed.map((offer) => offer.id)).toEqual(["persona.show:devops"]);
    expect(pressed[0].does).toEqual({ verb: "showPersona", persona: "devops" });
  });
});
