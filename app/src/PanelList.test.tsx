import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { PAGE, PanelList, SHORTEST, shorten } from "./PanelList";
import type { PanelRow } from "./bindings";

afterEach(cleanup);

/**
 * The list primitive, which is the four things the operator asked for about the todos panel
 * and then asked for again about personas:
 *
 * > *"rows are too long, they are wrapping in 3 lines — should be shortened automatically"*
 * > *"on clicking we should show full body — like persona description"*
 * > *"max 10 or 20 todos should be loaded, with scroll inside panel and load more"*
 * > *"search input should be visible when count is bigger than N"*
 *
 * **In jsdom, because three of the four are about the DOM and the fourth cannot be anywhere
 * else.** `docs/ui-primitives.md` records the two measured facts that decide where a claim
 * goes: jsdom lays nothing out, so a claim about *pixels* is a scenario's; and the scenario
 * driver dispatches a synthetic keydown with no default action of any kind, so a claim about
 * *activation* is jsdom's. Shortening is both — the character cut is here, the pixel fit is
 * CSS and is the scenario's to see.
 */
function row(key: string, text: string, over: Partial<PanelRow> = {}): PanelRow {
  return {
    key,
    text,
    note: null,
    mark: "dot",
    tone: "plain",
    detail: { kind: "text", text: `the whole of ${text}` },
    runs: null,
    actions: [],
    ...over,
  };
}

const EMPTY = { headline: "Nothing here", body: null, offer: null };

/** The list, with the window's own open-row state around it — which is where it lives. */
function draw(rows: PanelRow[], over: { page?: number; onRun?: (id: string) => void } = {}) {
  function Window() {
    const [open, setOpen] = useState<string>();
    return (
      <PanelList
        rows={rows}
        empty={EMPTY}
        label="Todos"
        testid="list"
        open={open}
        onOpen={setOpen}
        onRun={over.onRun}
        page={over.page}
      />
    );
  }
  render(<Window />);
}

/** `n` rows called `row-0`…, each with a short title. */
function many(n: number): PanelRow[] {
  return Array.from({ length: n }, (_, at) => row(`k${at}`, `Row ${at}`));
}

describe("a row is one line", () => {
  it("shortens words longer than a line rather than letting them wrap", () => {
    // **The defect, in the operator's words: rows "are wrapping in 3 lines".** CSS does the
    // pixel-exact fit; this is the half a DOM test can see, and it is the half that keeps a
    // 400-character title out of the accessible name and out of every `getByText`.
    const long =
      "Land the panel contribution contract and prove it by moving the two panels that already exist onto it";
    draw([row("a", long)]);

    const drawn = screen.getByRole("button").textContent ?? "";

    expect(drawn.length).toBeLessThan(long.length);
    expect(drawn).toContain("…");
    expect(drawn).toContain("Land the panel");
  });

  it("cuts on a word rather than mid-word, so the ellipsis has one job", () => {
    // A cut inside a word reads as a truncated WORD, and the reader then cannot tell whether
    // the row is long or the data is broken.
    expect(shorten("alpha beta gamma delta epsilon zeta eta theta iota kappa lambda", 40)).toBe(
      "alpha beta gamma delta epsilon zeta eta…",
    );
    // …unless there is no word boundary anywhere near the end, in which case a hard cut is
    // the only honest answer.
    expect(shorten("a".repeat(80), 10)).toBe("aaaaaaaaaa…");
  });

  it("leaves a row that already fits completely alone", () => {
    draw([row("a", "Short enough")]);

    expect(screen.getByRole("button")).toHaveTextContent("Short enough");
    expect(screen.getByRole("button").textContent).not.toContain("…");
  });

  it("keeps the whole of a shortened row where a pointer can still read it", () => {
    // The card is the answer for a reader who clicks; `title` is the answer for one who does
    // not, and it costs nothing. A row that fits carries no `title`, because a tooltip that
    // repeats what is already on screen is noise on every row in the region.
    const long = `Land ${"the contract ".repeat(8)}`.trim();
    draw([row("a", long), row("b", "Short")]);

    const [shortened, fits] = screen.getAllByRole("button");
    expect(shortened.querySelector(".row-text")).toHaveAttribute("title", long);
    expect(fits.querySelector(".row-text")).not.toHaveAttribute("title");
  });
});

describe("a row opens its card", () => {
  it("shows the whole of the row's words, which is what the persona card does", async () => {
    // The operator: *"on clicking we should show full body — like persona description"*. One
    // popover pattern (charter-app#173), reused rather than a second surface per panel.
    draw([row("a", "Review the rollout plan")]);

    await userEvent.click(screen.getByRole("button"));

    const card = await waitFor(() => screen.getByTestId("row-detail-a"));
    expect(card).toHaveTextContent("the whole of Review the rollout plan");
  });

  it("is a button exactly when it opens something, so a read-only list stays out of the tabs", () => {
    // A row with neither a card nor a catalogue verb is a `<span>`: a screen reader is not
    // told it can be pressed, and the region does not grow N tab stops that do nothing.
    draw([row("a", "Opens"), row("b", "Inert", { detail: null })]);

    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.getByText("Inert")).toBeInTheDocument();
  });

  it("runs the row's catalogue verb on the way open and never on the way shut", async () => {
    // A persona row's `persona.show:<name>` is the same verb the palette and a context menu
    // run (charter-app#174). Running it again on dismissal would re-open what was just closed.
    const ran: string[] = [];
    draw([row("steward", "steward", { runs: "persona.show:steward" })], {
      onRun: (id) => ran.push(id),
    });

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => screen.getByTestId("row-detail-steward"));
    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByTestId("row-detail-steward")).toBeNull());
    expect(ran).toEqual(["persona.show:steward"]);
  });
});

describe("the count is bounded", () => {
  it("draws a page and offers the rest, rather than every row there is", () => {
    // *"max 10 or 20 todos should be loaded … and load more"*. The scroll is the CSS half
    // (`.panel-rows` has a `max-height`); this is the half that keeps the DOM small, which is
    // the half that matters at the 500 rows `charter_core::panel::MOST_ROWS` allows.
    draw(many(30), { page: 5 });

    expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(5);
    expect(screen.getByRole("button", { name: /Show 5 more/ })).toBeInTheDocument();
  });

  it("grows by a page each time, and says how many are left", async () => {
    draw(many(12), { page: 5 });

    await userEvent.click(screen.getByRole("button", { name: /Show 5 more/ }));

    expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(10);
    // Two left, so the offer says two rather than a page.
    await userEvent.click(screen.getByRole("button", { name: /Show 2 more/ }));
    expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(12);
    expect(screen.queryByRole("button", { name: /Show/ })).toBeNull();
  });

  it("offers nothing more when everything already fits", () => {
    draw(many(3), { page: 5 });

    expect(screen.queryByRole("button", { name: /Show/ })).toBeNull();
  });

  it("defaults to a page charter chose rather than to however many there are", () => {
    draw(many(PAGE + 3));

    expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(PAGE);
  });
});

describe("the search", () => {
  it("appears once there are more rows than a page, and not before", () => {
    draw(many(PAGE));
    expect(screen.queryByRole("searchbox")).toBeNull();

    cleanup();
    draw(many(PAGE + 1));
    expect(screen.getByRole("searchbox")).toBeInTheDocument();
  });

  it("filters on what a row says, which is what the reader can see", async () => {
    // A match on something invisible reads as a bug, and there is nowhere on the row to show
    // the reader why it matched.
    const rows = [
      ...many(PAGE),
      row("hit", "Ship the contract", { note: "2026-09-23" }),
      row("miss", "Something else", { detail: { kind: "text", text: "contract" } }),
    ];
    draw(rows);

    await userEvent.type(screen.getByRole("searchbox"), "contract");

    expect(screen.getByText("Ship the contract")).toBeInTheDocument();
    expect(screen.queryByText("Something else")).toBeNull();
  });

  it("matches a row's note as well as its words", async () => {
    draw([...many(PAGE), row("hit", "A persona", { note: "12 memories" })]);

    await userEvent.type(screen.getByRole("searchbox"), "12 memories");

    expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(1);
  });

  it("says nothing matches rather than drawing the list's empty state", async () => {
    // The list is not empty; the search found nothing in it. Drawing "Nothing here" would
    // claim the workspace has no todos, which is a different fact and a worse one.
    draw(many(PAGE + 1));

    await userEvent.type(screen.getByRole("searchbox"), "zzzz");

    expect(screen.getByText(/Nothing here matches/)).toBeInTheDocument();
    expect(screen.queryByTestId("list-empty")).toBeNull();
  });

  it("stays on screen when a search narrows the list below a page", async () => {
    // Against the whole list and never the filtered one: a box that vanished mid-search would
    // take away the control being used.
    draw(many(PAGE + 5));

    await userEvent.type(screen.getByRole("searchbox"), "Row 1");

    expect(screen.getByRole("searchbox")).toBeInTheDocument();
  });

  it("goes back to one page when the search changes", async () => {
    // Otherwise a grown limit shows forty matches for one word and twelve for the next, which
    // reads as rows going missing.
    draw(many(40), { page: 5 });
    await userEvent.click(screen.getByRole("button", { name: /Show 5 more/ }));
    expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(10);

    await userEvent.type(screen.getByRole("searchbox"), "Row");

    expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(5);
  });
});

describe("an empty list", () => {
  it("says the panel's own sentence and not a shared default", () => {
    // *"Nothing to do"* and *"No personas on this plane"* are different claims, which is why
    // the sentence is part of the contract (`charter_core::panel::Empty`) rather than a
    // default this component supplies.
    render(
      <PanelList
        rows={[]}
        empty={{ headline: "Nothing to do", body: "Todos are files.", offer: null }}
        label="Todos"
        testid="list"
        open={undefined}
        onOpen={() => {}}
      />,
    );

    expect(screen.getByTestId("list-empty")).toHaveTextContent("Nothing to do");
    expect(screen.getByText("Todos are files.")).toBeInTheDocument();
  });

  it("offers no search and no load-more, because there is nothing to search or load", () => {
    render(<PanelList rows={[]} empty={EMPTY} label="Todos" open={undefined} onOpen={() => {}} />);

    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(screen.queryByRole("list")).toBeNull();
  });
});

describe("the cut", () => {
  it("is a character budget and the ellipsis is CSS's job as well", () => {
    // Said out loud in a test because it is the thing a later reader will get wrong: this
    // number does not fit text to a region, and it is not trying to. It bounds the DOM.
    expect(SHORTEST).toBeGreaterThan(40);
    expect(shorten("x".repeat(SHORTEST))).toHaveLength(SHORTEST);
    expect(shorten("x".repeat(SHORTEST + 1))).toHaveLength(SHORTEST + 1);
  });
});

describe("the keyboard in a list (charter-app#189)", () => {
  it("is one Tab stop: the first row that does something, and Up and Down move", async () => {
    draw([row("a", "first"), row("b", "read only", { detail: null }), row("c", "third")]);
    const list = screen.getByRole("list", { name: "Todos" });
    const [first, third] = within(list).getAllByRole("button");
    // A row that does nothing is a `<span>` and no stop at all; of the two that do, one is.
    expect([first, third].map((one) => one.getAttribute("tabindex"))).toEqual(["0", "-1"]);

    first.focus();
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() => expect(third).toHaveFocus());
    expect(third).toHaveAttribute("tabindex", "0");
  });

  it("comes back in on the row whose card is open", async () => {
    draw([row("a", "first"), row("b", "second")]);
    const list = screen.getByRole("list", { name: "Todos" });
    const [, second] = within(list).getAllByRole("button");
    await userEvent.click(second);
    await screen.findByTestId("row-detail-b");
    // The card has the keyboard now, and the list's stop is the row it came from.
    await waitFor(() => expect(second).toHaveAttribute("tabindex", "0"));
    expect(within(list).getAllByRole("button")[0]).toHaveAttribute("tabindex", "-1");
  });
});
