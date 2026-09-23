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
import type { PanelRow, PanelView, Panels as PanelsModel, PersonaDetails } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The project these answers are for. A persona belongs to a plane, and a window holds
 *  several: two of them can both have a `steward`. */
const PLANE = "/home/dev/plane";

// ---------------------------------------------------------------------------------------
// The contributions, as `app/src-tauri/src/panels.rs` builds them
// ---------------------------------------------------------------------------------------

/**
 * **These fixtures are `charter_core::panel` values, and that is the change.**
 *
 * Until this file was rewritten it asserted that `Panels.tsx` drew a todo out of a `todos`
 * field. There is no such field in the renderer now: todos and personas are contributions, so
 * what this file tests is the thing that draws a panel, against the vocabulary it draws from.
 *
 * Where the other half went — that charter's own producer emits *these* values off a real
 * plane — is two places, both closer to the fact than a mock here could be:
 * `app/src-tauri/src/panels.rs`'s tests, against a plane on disk, and `regions.e2e.ts`, against
 * the real core through the real window.
 */
function row(key: string, text: string, over: Partial<PanelRow> = {}): PanelRow {
  return {
    key,
    text,
    note: null,
    mark: "dot",
    tone: "plain",
    detail: null,
    runs: null,
    ...over,
  };
}

function todosPanel(rows: PanelRow[], refused?: string): PanelView {
  return {
    key: "charter/todos",
    title: "Todos",
    order: 10,
    mark: "todo",
    from: null,
    blocks: [
      ...(refused === undefined ? [] : [{ kind: "note" as const, text: refused, tone: "trouble" }]),
      {
        kind: "list" as const,
        rows,
        empty: { headline: "Nothing to do", body: null, offer: null },
      },
    ],
  };
}

function personasPanel(names: string[], fallback: string | null): PanelView {
  return {
    key: "charter/personas",
    title: "Personas",
    order: 20,
    mark: "persona",
    from: null,
    blocks: [
      {
        kind: "list",
        rows: names.map((name) =>
          row(name, name, {
            mark: "persona",
            tone: name === fallback ? "default" : "plain",
            note: name === fallback ? "default · 2 memories" : "2 memories",
            detail: { kind: "persona", persona: name },
            runs: `persona.show:${name}`,
          }),
        ),
        empty: { headline: "No personas on this plane", body: null, offer: null },
      },
    ],
  };
}

/** What an approved extension contributes: declared rows, no verbs, and its id on the panel. */
function contributedPanel(over: Partial<PanelView> = {}): PanelView {
  return {
    key: "ext/acme/reviews",
    title: "Reviews",
    order: 15,
    mark: "note",
    from: "acme",
    blocks: [
      {
        kind: "list",
        rows: [
          row("a", "Land the panel contribution contract", {
            note: "2026-09-23",
            detail: { kind: "text", text: "The whole of what this review is about." },
          }),
        ],
        empty: { headline: "Nothing to review", body: null, offer: null },
      },
    ],
    ...over,
  };
}

const TODO = row("20260302-091400-review", "Review the rollout plan", {
  mark: "todo",
  note: "2026-03-02",
  detail: { kind: "text", text: "The rollout plan, in full." },
});

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
  contributed: [todosPanel([TODO]), personasPanel(["devops", "steward"], "steward")],
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
 * **Which row's card is open belongs to the window** (charter-app#174, generalised): opening a
 * persona's card is a catalogue row, so a context menu and the palette can open it from outside
 * this panel. The wrapper here is that window, in as few lines as the claim needs.
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
    contributed?: PanelView[];
  } = {},
) {
  function Window() {
    const [shownRow, setShownRow] = useState<string>();
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
        contributed={on.contributed ?? []}
        shownRow={shownRow}
        onShowRow={setShownRow}
      />
    );
  }
  render(<Window />);
}

describe("the right-hand region", () => {
  it("holds the needs-you queue, which used to share a line with six other things", () => {
    // charter ADR 0038 moved it here off `<header className="bar">`.
    draw({ queue: [7] });

    expect(within(screen.getByTestId("panels")).getByLabelText("Needs you")).toBeInTheDocument();
  });

  it("brings a chat forward from the queue", async () => {
    const shown: number[] = [];
    draw({ queue: [7], showChat: (session) => shown.push(session) });

    await userEvent.click(screen.getByRole("button", { name: /ide\.7/ }));

    expect(shown).toEqual([7]);
  });

  it("draws the queue even when no workspace is focused, because it is not the workspace's", () => {
    // A chat asking for you in a workspace nobody is looking at is exactly the one that must
    // not be hidden.
    draw({ workspace: undefined, queue: [7] });

    expect(screen.getByLabelText("Needs you")).toBeInTheDocument();
  });

  it("is named for what it is, and not a second answer to which workspace this is", () => {
    draw();

    expect(screen.getByTestId("panels")).toHaveAttribute("aria-label", "Attention · alpha");
  });

  it("holds no alerts section, because alerts cross projects and this region is one project's", () => {
    // ADR 0038 put alerts here; the window's status line has them instead, because the plane
    // with an alert is usually not the one on screen. `Panels.tsx` argues it at length.
    draw();

    expect(screen.getByTestId("panels").textContent).not.toMatch(/alert/i);
  });

  it("draws no workspace answers when none is focused", () => {
    draw({ workspace: undefined });

    expect(screen.getByText("No workspace focused.")).toBeInTheDocument();
    expect(screen.queryByTestId("panel-todos")).toBeNull();
  });

  it("no longer draws repos or CI, which are state and went to the bottom bar", () => {
    draw();

    expect(screen.queryByText("svc")).toBeNull();
    expect(screen.getByTestId("panels").textContent).not.toMatch(/\bCI\b/);
  });

  it("says when the core refused the workspace outright", () => {
    draw({ state: state({ panels: undefined, trouble: "no workspace 'alpha'" }) });

    expect(screen.getByRole("alert")).toHaveTextContent("no workspace 'alpha'");
  });

  it("says the plane is still being read rather than saying there is nothing to do", () => {
    // An unanswered ask and an empty answer are the two states this must never merge.
    draw({ state: state({ panels: undefined }) });

    expect(screen.getByText(/Reading the plane/)).toBeInTheDocument();
    expect(screen.queryByText("Nothing to do")).toBeNull();
  });
});

// ---------------------------------------------------------------------------------------
// What the contract bought: the same renderer for charter's panels and a stranger's
// ---------------------------------------------------------------------------------------

describe("a panel", () => {
  it("draws whatever is contributed, in the order the contributors asked for", () => {
    // charter's todos at 10, an extension's at 15, charter's personas at 20. A contributed
    // panel is not appended after charter's — it is sorted among them, which is what `order`
    // being a number rather than a flag is for.
    draw({ contributed: [contributedPanel()] });

    const headings = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(headings).toEqual(["Todos", "Reviews · acme", "Personas"]);
  });

  it("says whose it is, which is what charter ADR 0041 item 5 asks the window for", () => {
    // *Show what is in force, after approval and not only at it.* An operator has to be able
    // to tell a panel his own charter draws from one a stranger's extension contributed,
    // without opening a dialog to find out.
    draw({ contributed: [contributedPanel()] });

    expect(screen.getByTestId("panel-ext-acme-reviews")).toHaveAttribute("data-panel-from", "acme");
    expect(screen.getByTestId("panel-todos")).toHaveAttribute("data-panel-from", "charter");
    expect(within(screen.getByTestId("panel-ext-acme-reviews")).getByText(/acme/)).toBeVisible();
  });

  it("gives a contributed panel the list primitive, which is the test of the contract", async () => {
    // **The whole claim, in one test.** An extension declared rows in a manifest and got the
    // shortening, the card, the bound, the load-more and the search — none of which it asked
    // for, wrote, or can see. If this passes, the contract is worth having; if a contributed
    // panel needed one line of its own in `Panels.tsx` to get any of it, it is not.
    const rows = Array.from({ length: 20 }, (_, at) => row(`k${at}`, `Review ${at}`));
    draw({ contributed: [contributedPanel({ blocks: [listOf(rows)] })] });

    const panel = screen.getByTestId("panel-ext-acme-reviews");
    expect(within(panel).getByRole("searchbox")).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: /Show \d+ more/ })).toBeInTheDocument();
    expect(within(panel).getAllByRole("listitem").length).toBeLessThan(rows.length);
  });

  it("runs a row's catalogue verb through the catalogue, and an unknown id through nothing", async () => {
    // A row cannot invent a verb: what it names is looked up, and an id the catalogue has
    // stopped offering runs nothing rather than something else. That is `Doer`'s rule for the
    // bar's buttons, applied to a panel — and it is what keeps `actions.ts` the one list even
    // though a row now names one of its members by id.
    const pressed: Offer[] = [];
    draw({
      offers: everyOffer(),
      onPress: (offer) => pressed.push(offer),
      contributed: [
        contributedPanel({
          blocks: [listOf([row("a", "Invented", { runs: "acme.do-a-thing" })])],
        }),
      ],
    });

    await userEvent.click(within(screen.getByTestId("panel-ext-acme-reviews")).getByRole("button"));

    expect(pressed).toEqual([]);
  });

  it("draws a block charter could not fill as charter's own sentence, never as an empty list", () => {
    // A store charter would not read is not a workspace with nothing to do, and "Nothing to
    // do" is exactly what an empty list would claim about it.
    draw({
      state: state({
        panels: { ...PANELS, contributed: [todosPanel([], "todos/ is a link out of the plane")] },
      }),
    });

    expect(screen.getByRole("alert")).toHaveTextContent("todos/ is a link out of the plane");
  });

  it("says the panel's own empty sentence rather than a shared one", () => {
    draw({
      state: state({
        panels: { ...PANELS, contributed: [todosPanel([]), personasPanel([], null)] },
      }),
    });

    expect(screen.getByText("Nothing to do")).toBeInTheDocument();
    expect(screen.getByText("No personas on this plane")).toBeInTheDocument();
  });
});

/** One list block, for a fixture that wants different rows. */
function listOf(rows: PanelRow[]) {
  return {
    kind: "list" as const,
    rows,
    empty: { headline: "Nothing to review", body: null, offer: null },
  };
}

/** The whole catalogue for this plane, which is what a row's verb is looked up in. */
function everyOffer(): Catalogued {
  return catalogued(
    catalogue({
      tabs: noTabs(),
      workspaces: [],
      plane: PLANE,
      personas: ["devops", "steward"],
      needsYou: [],
      nameOf: String,
    }),
  );
}

// ---------------------------------------------------------------------------------------
// charter's own two, now that they arrive as contributions
// ---------------------------------------------------------------------------------------

describe("charter's own panels", () => {
  it("shows the focused workspace's open todos", () => {
    draw();

    expect(
      within(screen.getByTestId("panel-todos")).getByText(/Review the rollout plan/),
    ).toBeInTheDocument();
  });

  it("marks the plane's default persona, in the word and beside it", () => {
    // The word stays a word: the region's scenario spec asks the panel whether it says
    // `default`, and a screen reader gets the same sentence a sighted reader does. The star
    // the tone draws is decoration on top of it.
    draw();

    const rows = within(screen.getByTestId("panel-personas")).getAllByRole("listitem");
    const [devops, steward] = rows;
    expect(steward).toHaveTextContent("default");
    expect(steward).toHaveClass("is-default");
    expect(devops).not.toHaveClass("is-default");
  });

  it("carries how much each persona remembers, which the operator asked to see in the list", () => {
    draw();

    const notes = within(screen.getByTestId("panel-personas")).getAllByText(/2 memories/);
    expect(notes).toHaveLength(2);
    // `0 memories` is said rather than left off: a row that omits the count reads as one
    // charter did not look at, which is a different fact from a persona with none.
    expect(notes[1]).toHaveTextContent("default · 2 memories");
  });
});

/**
 * **The count's colour is only ever on a pair the contrast suite measures.**
 *
 * `needs-you.base` is `#b85050` in charter-dark, which is 3.64:1 on `surface.base` — under AA
 * for words. So the words beside the count are `text.primary`, and the number is
 * `needs-you.text` FILLED with `needs-you.base`, which is the pair `contrast.test.ts` holds at
 * 4.5:1. jsdom computes no colour, so this reads the rules.
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
// What a persona row opens
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

/** One memory, as `persona_memories` answers with — a row of the same vocabulary. */
function memory(key: string, title: string, body: string): PanelRow {
  return row(key, title, {
    mark: "note",
    note: "2026-09-20",
    detail: { kind: "text", text: body },
  });
}

/** The core, answering both of the card's asks and counting what it was asked. */
function core(
  answer: (persona: string) => unknown,
  memories: (persona: string) => PanelRow[] = () => [],
): { asked: Record<string, unknown>[] } {
  const asked: Record<string, unknown>[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    if (cmd === "persona_memories") return memories(String(given.persona));
    if (cmd !== "persona_details") return undefined;
    asked.push({ ...given, cmd });
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
  return waitFor(() => screen.getByTestId(`row-detail-${persona}`));
}

describe("a persona's card", () => {
  it("shows what the definition says: its role, when to delegate to it, its tools and its vault", async () => {
    core(() => definition());
    draw();

    const card = await open("devops");

    expect(card).toHaveTextContent("DevOps Engineer");
    expect(card).toHaveTextContent("CI/CD pipelines, k8s deploys");
    expect(card).toHaveTextContent("kubectl, glab");
    expect(card).toHaveTextContent("personas/devops/persona.md");
  });

  it("asks the core about this plane's persona, and asks again the next time it is opened", async () => {
    // A definition is a file an operator edits while charter is running, so a cached first
    // answer would show a role that was corrected an hour ago.
    const { asked } = core((persona) => definition({ name: persona }));
    draw();

    await open("devops");
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("row-detail-devops")).toBeNull());
    await open("devops");

    expect(asked.filter((one) => one.cmd === "persona_details")).toHaveLength(2);
    expect(asked[0]).toMatchObject({ plane: PLANE, persona: "devops" });
  });

  it("names the vault and nothing that is in it", async () => {
    // charter refuses a secret by kind and never echoes one; a panel gets no exception.
    core(() => definition({ vault: "devops" }));
    draw();

    const card = await open("devops");

    expect(card).toHaveTextContent("devops");
    expect(card).toHaveTextContent("what is in it is never shown here");
  });

  it("says a persona holds no credentials only where it says so itself", async () => {
    // "No `vault:` line" means charter has not looked, not that there is nothing —
    // `PersonaDetails::declares_no_vault` has the whole of why.
    core(() => definition({ vault: null, declares_no_vault: false }));
    draw();

    expect(await open("devops")).toHaveTextContent("not declared in its definition");
  });

  it("draws the core's own refusal rather than an empty card", async () => {
    mockIPC((cmd) => {
      if (cmd === "persona_memories") return [];
      if (cmd === "persona_details") throw new Error("no persona 'devops'");
      return undefined;
    });
    draw();

    expect(await open("devops")).toHaveTextContent("no persona 'devops'");
  });

  it("is not modal, so the queue this region exists for stays reachable", async () => {
    // Radix marks everything outside an open DIALOG `aria-hidden` — including the needs-you
    // queue two sections up, which ADR 0038 says this region must never compete with. A
    // popover takes the menu's decisions instead (ADR 0039).
    core(() => definition());
    draw({ queue: [7] });

    await open("devops");

    expect(screen.getByLabelText("Needs you")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ide\.7/ })).toBeInTheDocument();
  });

  it("closes on Escape and puts the keyboard back on the row", async () => {
    core(() => definition());
    draw();
    const card = await open("devops");
    expect(card).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByTestId("row-detail-devops")).toBeNull());
    expect(document.activeElement).toHaveTextContent("devops");
  });
});

describe("a persona's memories", () => {
  it("are readable from the window, which is what the operator asked for", async () => {
    // *"on clicking to persona we should open maybe list of memories … user will be able to
    // read all memories from ui"*.
    core(
      () => definition(),
      () => [memory("a", "Charter defects go upstream", "File the issue; do not patch around it.")],
    );
    draw();

    const card = await open("devops");

    expect(within(card).getByText(/Charter defects go upstream/)).toBeVisible();
  });

  it("get the same list primitive the panel itself has, one surface in", async () => {
    // **The second consumer, and the one that decides whether `PanelList` is a primitive or a
    // panel with a general-sounding name.** Nothing in the card knows what a memory is: they
    // arrive as rows, so they are searched, bounded and opened by the code the panel uses.
    core(
      () => definition(),
      () => Array.from({ length: 30 }, (_, at) => memory(`m${at}`, `Memory ${at}`, `body ${at}`)),
    );
    draw();

    const card = await open("devops");

    const search = await within(card).findByRole("searchbox");
    await userEvent.type(search, "Memory 7");
    await waitFor(() =>
      expect(within(card).getByLabelText("devops's memories").children).toHaveLength(1),
    );
  });

  it("says charter could not read them rather than saying there are none", async () => {
    mockIPC((cmd) => {
      if (cmd === "persona_memories") throw new Error("memory/ is a link out of the plane");
      if (cmd === "persona_details") return definition();
      return undefined;
    });
    draw();

    const card = await open("devops");

    await waitFor(() =>
      expect(within(card).getByRole("alert")).toHaveTextContent("link out of the plane"),
    );
  });

  it("says nothing is remembered yet where the store is simply empty", async () => {
    core(
      () => definition(),
      () => [],
    );
    draw();

    const card = await open("devops");

    await waitFor(() =>
      expect(within(card).getByText("Nothing remembered yet")).toBeInTheDocument(),
    );
  });
});

/**
 * Right-click on a persona row (charter-app#174).
 *
 * **In jsdom, because a WebDriver right-click sends no `contextmenu` event** — measured on both
 * engines and recorded in `docs/ui-primitives.md`. What is asserted is the wiring: the row has
 * charter's own menu and it lists the catalogue's row for that persona.
 *
 * **And it is charter's own panel that has one.** A contributed panel's rows get no context
 * menu, because a menu names what a row is ABOUT and a row is words, a key and at most a
 * catalogue id — see `PanelList`'s `RowMenu`. That asymmetry is the contract's, written down
 * rather than discovered.
 */
describe("a persona row's menu", () => {
  /** Right-clicks a row, the way a WebView's own pointer does. */
  function rightClick(testid: string, name: string) {
    const row = within(screen.getByTestId(testid)).getByRole("button", {
      name: new RegExp(name),
    });
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  }

  it("opens on a right-click with the catalogue's row for that persona", async () => {
    core(() => definition());
    draw({ offers: everyOffer() });

    rightClick("panel-personas", "devops");

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual(["Show what devops is"]);
  });

  it("hands back the row that opens the card, which is the state the window holds", async () => {
    core(() => definition());
    const pressed: Offer[] = [];
    draw({ offers: everyOffer(), onPress: (offer) => pressed.push(offer) });

    rightClick("panel-personas", "devops");
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "Show what devops is" }));

    expect(pressed.map((offer) => offer.id)).toEqual(["persona.show:devops"]);
    expect(pressed[0].does).toEqual({ verb: "showPersona", persona: "devops" });
  });

  it("gives a contributed panel's row no menu, which is the contract's asymmetry", async () => {
    core(() => definition());
    draw({ offers: everyOffer(), contributed: [contributedPanel()] });

    rightClick("panel-ext-acme-reviews", "Land the panel");

    await expect(vi.waitFor(() => screen.getByRole("menu"), { timeout: 200 })).rejects.toThrow();
  });
});
