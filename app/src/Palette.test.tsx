import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Palette, opensIt } from "./Palette";
import { CHAT_KEYBOARD, type Offer, type Ran } from "./actions";

afterEach(cleanup);

const ready = (id: string, title: string) =>
  ({ id, title, available: true, reason: "", does: { verb: "nothing" } }) satisfies Offer;

const refused = (id: string, title: string, reason: string) =>
  ({ id, title, available: false, reason, does: { verb: "nothing" } }) satisfies Offer;

/** The window carrying a row out, standing in for `App`'s own dispatcher: it answers the
 *  way `perform` does, so a test never proves something the real one would not do. */
const ok = (offer: Offer): Ran =>
  offer.available ? { ok: true } : { ok: false, refused: offer.reason };

/** The rows a test drives, unless it says otherwise. */
const OFFERS: Offer[] = [
  refused("pane.split.right", "Split right", "No chat is in front, so there is no pane to split."),
  ready("chat.new", "New tab"),
  ready("workspace.focus:beta", "Focus workspace beta"),
];

/** Opens it the way an operator does: one key, from wherever the keyboard happens to be. */
async function open() {
  await userEvent.keyboard("{F2}");
  return screen.findByRole("dialog", { name: "Command palette" });
}

const rows = () =>
  screen.getAllByRole("option").map((row) => row.querySelector(".palette-title")?.textContent);

const aimed = () =>
  screen
    .getAllByRole("option")
    .find((row) => row.getAttribute("aria-selected") === "true")
    ?.querySelector(".palette-title")?.textContent;

describe("the command palette", () => {
  it("is not on screen until a key opens it", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await open();
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
  });

  it("lists every action the window has, in the order the catalogue gave them", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();

    expect(rows()).toEqual(["Split right", "New tab", "Focus workspace beta"]);
  });

  it("narrows as you type, without caring about case", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();

    await userEvent.keyboard("WORKSPACE");

    expect(rows()).toEqual(["Focus workspace beta"]);
  });

  it("says so rather than going blank when nothing matches", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();

    await userEvent.keyboard("zzz");

    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(screen.getByText("No action matches what you typed.")).toBeInTheDocument();
  });

  it("keeps a row that cannot run, with its reason in words beside it", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();

    const row = screen.getByRole("option", { name: /Split right/ });
    expect(row).toHaveAttribute("aria-disabled", "true");
    // The reason is TEXT. Dimming it is decoration, and a reason nobody can read is a row
    // that merely looks broken.
    expect(within(row).getByText(/No chat is in front/)).toBeInTheDocument();
  });

  it("aims Enter at the first row that can run, not at the first row", async () => {
    const onRun = vi.fn(ok);
    render(<Palette offers={[OFFERS[0], ready("chat.new", "New tab")]} onRun={onRun} />);
    await open();

    expect(aimed()).toBe("New tab");
    await userEvent.keyboard("{Enter}");

    expect(onRun).toHaveBeenCalledTimes(1);
    expect(onRun.mock.calls[0][0].id).toBe("chat.new");
  });

  it("runs nothing when Enter is on a row that cannot run, and says why", async () => {
    render(<Palette offers={[OFFERS[0]]} onRun={ok} />);
    await open();

    // Nothing is aimed at, because nothing here can run.
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(screen.getAllByText(/No chat is in front/).length).toBeGreaterThan(0);
  });

  it("moves over every row with the arrows, refused ones included", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();

    expect(aimed()).toBe("New tab");
    await userEvent.keyboard("{ArrowUp}");
    expect(aimed()).toBe("Split right");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(aimed()).toBe("Focus workspace beta");
  });

  it("runs the row Enter is on and then leaves", async () => {
    const onRun = vi.fn((offer: Offer): Ran => ({ ok: true, said: `${offer.title} was run` }));
    render(<Palette offers={[ready("workspace.focus:beta", "Focus beta")]} onRun={onRun} />);
    await open();

    await userEvent.keyboard("{Enter}");

    expect(onRun).toHaveBeenCalledTimes(1);
    expect(onRun.mock.calls[0][0].id).toBe("workspace.focus:beta");
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("stays open on a refusal, so the operator can answer it", async () => {
    const onRun = vi.fn((): Ran => ({ ok: false, refused: "svc/fix-it has uncommitted changes" }));
    render(<Palette offers={[ready("worktree.remove", "Remove")]} onRun={onRun} />);
    await open();

    await userEvent.keyboard("{Enter}");

    await vi.waitFor(() => expect(onRun).toHaveBeenCalledTimes(1));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("shows a refusal in the core's own words, unchanged", async () => {
    render(
      <Palette
        offers={OFFERS}
        said={{
          from: "worktree.remove",
          refused: true,
          words: "svc/fix-it has uncommitted changes; commit or stash them first",
        }}
        onRun={ok}
      />,
    );
    await open();

    const alert = within(screen.getByRole("dialog")).getByRole("alert");
    expect(alert).toHaveTextContent(
      "svc/fix-it has uncommitted changes; commit or stash them first",
    );
  });

  it("leaves on Escape, having run nothing", async () => {
    const onRun = vi.fn(ok);
    render(<Palette offers={[ready("chat.new", "New tab")]} onRun={onRun} />);
    await open();

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(onRun).not.toHaveBeenCalled();
  });

  it("leaves on Escape even when the keyboard has moved off the box", async () => {
    // The one key that always leaves has to be true wherever the focus has got to. On the
    // box it was only true while the box had it, and a modal surface nobody can leave is the
    // worst thing a modal surface can be.
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();
    screen.getByRole("combobox").blur();
    expect(document.activeElement).not.toBe(screen.getByRole("combobox"));

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("gives the keyboard back to whatever had it", async () => {
    // An operator opens this mid-sentence, from inside a pane's terminal. Escape has to put
    // them back in it, or the palette costs a click every time it is opened by mistake.
    render(
      <>
        <textarea aria-label="a pane's terminal" />
        <Palette offers={OFFERS} onRun={ok} />
      </>,
    );
    const pane = screen.getByLabelText("a pane's terminal");
    pane.focus();

    await open();
    expect(document.activeElement).toBe(screen.getByRole("combobox"));

    await userEvent.keyboard("{Escape}");
    expect(document.activeElement).toBe(pane);
  });

  it("forgets what was typed, so the next opening starts from the whole list", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();
    await userEvent.keyboard("workspace{Escape}");

    await open();

    expect(rows()).toHaveLength(3);
  });

  it("tells the window when it is up, so a report is never said twice", async () => {
    const onOpened = vi.fn();
    render(<Palette offers={OFFERS} onRun={ok} onOpened={onOpened} />);

    await open();
    expect(onOpened).toHaveBeenLastCalledWith(true);

    await userEvent.keyboard("{Escape}");
    expect(onOpened).toHaveBeenLastCalledWith(false);
  });

  it("does not open a second one over the first", async () => {
    render(<Palette offers={OFFERS} onRun={ok} />);
    await open();

    await userEvent.keyboard("{F2}");

    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });
});

describe("the key that opens it", () => {
  const press = (init: Partial<KeyboardEventInit> & { key: string }) =>
    opensIt(new KeyboardEvent("keydown", init));

  it("is F2, the key the tmux frame trained these fingers on", () => {
    expect(press({ key: "F2" })).toBe(true);
  });

  it("is also the desktop's own, on either platform's modifier", () => {
    expect(press({ key: "k", metaKey: true })).toBe(true);
    expect(press({ key: "k", ctrlKey: true })).toBe(true);
    expect(press({ key: "K", metaKey: true })).toBe(true);
  });

  it("is not a bare k, which is a letter somebody is typing into a chat", () => {
    expect(press({ key: "k" })).toBe(false);
  });

  it("is not F2 with a modifier on it, which is somebody else's binding", () => {
    expect(press({ key: "F2", ctrlKey: true })).toBe(false);
    expect(press({ key: "F2", shiftKey: true })).toBe(false);
  });
});

/**
 * The way out of the key the palette claimed (charter-app#47).
 *
 * `F2` is taken on the window, capture-phase, so a pane's terminal never sees it. tmux
 * answers the same question with `send-prefix` — press the prefix twice and the second goes
 * through — and so does this. What runs is the catalogue's own `pane.sendkey` row, which is
 * what keeps the chord from becoming a second implementation of it.
 */
describe("handing F2 back to the chat", () => {
  const SENDS = ready("pane.sendkey", "Send F2 to the chat in front");
  const WITH_SEND: Offer[] = [...OFFERS, SENDS];

  it("runs the row that sends it, and gets out of the way", async () => {
    const onRun = vi.fn(ok);
    render(<Palette offers={WITH_SEND} onRun={onRun} />);
    await open();

    await userEvent.keyboard("{F2}");

    expect(onRun).toHaveBeenCalledWith(SENDS);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("says so the moment it opens, because a way out nobody can find is not one", async () => {
    render(<Palette offers={WITH_SEND} onRun={ok} />);
    await open();

    expect(screen.getByText("Press F2 again to send F2 to the chat in front.")).toBeInTheDocument();
  });

  it("promises nothing when there is nowhere to send it", async () => {
    const nowhere = [
      ...OFFERS,
      refused("pane.sendkey", "Send F2 to the chat in front", "No chat is in front."),
    ];
    render(<Palette offers={nowhere} onRun={ok} />);
    await open();

    expect(screen.queryByText(/Press F2 again/)).not.toBeInTheDocument();
    // The row is still listed with its reason, which is where an operator asks about it.
    expect(screen.getByText("No chat is in front.")).toBeInTheDocument();
  });

  it("stays open and says the reason rather than pretending, when the row cannot run", async () => {
    const nowhere = [
      ...OFFERS,
      refused("pane.sendkey", "Send F2 to the chat in front", "No chat is in front."),
    ];
    const onRun = vi.fn(ok);
    render(<Palette offers={nowhere} onRun={onRun} />);
    await open();

    await userEvent.keyboard("{F2}");

    expect(onRun).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("does not hand anything back for Ctrl-K, whose own way out is filed separately", async () => {
    // Scoped deliberately: `F2` is the key #47 is about, and what a terminal makes of
    // `⌘K`/`Ctrl-K` is a different question with a different answer.
    const onRun = vi.fn(ok);
    render(<Palette offers={WITH_SEND} onRun={onRun} />);
    await open();

    await userEvent.keyboard("{Control>}k{/Control}");

    expect(onRun).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});

/**
 * Which keys belong to the chat, and which to the window (charter-app#106).
 *
 * `Ctrl-K` was claimed on the window, capture-phase, with `preventDefault()` — the same way
 * `F2` was before #47 — and it is kill-to-end-of-line in readline, which is to say in the
 * shell every chat starts in. So an operator killing the rest of a line got the palette.
 *
 * **The rule these pin**: a chord a terminal encodes belongs to the chat whenever a chat has
 * the keyboard, unless the window claims it there deliberately — and a key claimed from under
 * a focused chat must have a way to hand it back. `F2` has one; `⌘K` needs none, because
 * xterm.js sends no bytes for a `⌘`-chord but `⌘A`; `Ctrl-K` had neither, so the chat keeps
 * it.
 *
 * **They are unit tests and not scenarios on purpose.** A scenario is the right shape for
 * "the chat received these bytes", but this rig cannot press a modifier chord at all:
 * `browser.keys(["Shift", "Tab"])` was measured in #176 arriving with `shiftKey` unset, and
 * `docs/ui-primitives.md` records it. jsdom dispatches the real event, so the claim can be
 * made here and nowhere else.
 */
describe("a chord the chat's own terminal would encode", () => {
  /** A pane, marked the way `SessionPane` marks itself, with xterm's textarea inside it. */
  const chat = () => (
    <div {...{ [CHAT_KEYBOARD]: "" }}>
      <textarea aria-label="a pane's terminal" />
    </div>
  );

  const watching: (() => void)[] = [];
  afterEach(() => {
    for (const stop of watching.splice(0)) stop();
  });

  /**
   * Every keydown that got past the window, and whether it had already been stopped.
   *
   * Capture on `document`, which is one step BELOW the window in the capture path, so the
   * palette's own listener has had its say by the time this runs. It is the half of the claim
   * that matters: the palette not opening is not the same thing as the keystroke surviving,
   * and it is the survival that reaches the shell.
   */
  function watchTheDocument() {
    const seen: { key: string; prevented: boolean }[] = [];
    const watch = (e: KeyboardEvent) => {
      seen.push({ key: e.key, prevented: e.defaultPrevented });
    };
    document.addEventListener("keydown", watch, true);
    watching.push(() => document.removeEventListener("keydown", watch, true));
    return seen;
  }

  /** Puts the keyboard where an operator mid-sentence has it: inside the pane's terminal. */
  function inTheChat() {
    screen.getByLabelText("a pane's terminal").focus();
  }

  it("reaches the chat when it is Ctrl-K, which is the shell's kill-to-end-of-line", async () => {
    const seen = watchTheDocument();
    render(
      <>
        {chat()}
        <Palette offers={OFFERS} onRun={ok} />
      </>,
    );
    inTheChat();

    await userEvent.keyboard("{Control>}k{/Control}");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    // Unprevented and unstopped, so xterm's textarea gets it and sends the control byte.
    expect(seen).toContainEqual({ key: "k", prevented: false });
  });

  it("still opens the palette from anywhere that is not a chat, so the chord survives", async () => {
    // Why `Ctrl-K` is not simply dropped: on Linux there is no `⌘`, and taking the desktop's
    // own chord away everywhere would be the regression `opensIt` was written to avoid. It is
    // the chat that has a claim on it, not the whole window.
    render(
      <>
        {chat()}
        <Palette offers={OFFERS} onRun={ok} />
      </>,
    );

    await userEvent.keyboard("{Control>}k{/Control}");

    expect(await screen.findByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
  });

  it("opens on ⌘K even inside a chat, because a terminal sends nothing for it", async () => {
    // Measured in the version this app depends on: xterm.js 6.0.0's `evaluateKeyboardEvent`
    // answers a `⌘`-chord with `SELECT_ALL` for `⌘A` and with no bytes at all for anything
    // else. There is nothing for the window to be taking, so it takes it.
    render(
      <>
        {chat()}
        <Palette offers={OFFERS} onRun={ok} />
      </>,
    );
    inTheChat();

    await userEvent.keyboard("{Meta>}k{/Meta}");

    expect(await screen.findByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
  });

  it("opens on F2 even inside a chat, because #47 gave F2 a way back", async () => {
    // The precedent, unchanged and deliberately so: `F2` IS claimed from under a focused
    // chat, and what makes that defensible is `send-prefix` — a second press sends it on.
    render(
      <>
        {chat()}
        <Palette offers={OFFERS} onRun={ok} />
      </>,
    );
    inTheChat();

    await userEvent.keyboard("{F2}");

    expect(await screen.findByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
  });
});

/**
 * The aim, kept on screen (charter-app#48).
 *
 * The list scrolls at 55vh and the catalogue is well past a hundred rows with fifty chats
 * open, so past the first screenful the arrows were moving `aria-selected` onto a row nobody
 * could see. jsdom has no layout and therefore no `scrollIntoView`, so the test plants one
 * and watches for the call — which is the whole of what the component can be asked to do.
 */
describe("the row Enter is aimed at", () => {
  const MANY: Offer[] = Array.from({ length: 60 }, (_, at) =>
    ready(`tab.select:${at}`, `Switch to tab ${at}`),
  );

  function watchScrolling(): ReturnType<typeof vi.fn> {
    const scrolled = vi.fn();
    // Element.prototype, because the row is found by id after it is drawn.
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      writable: true,
      value: scrolled,
    });
    return scrolled;
  }

  afterEach(() => {
    delete (Element.prototype as unknown as { scrollIntoView?: unknown }).scrollIntoView;
  });

  it("is brought on screen when the arrows reach past what is visible", async () => {
    const scrolled = watchScrolling();
    render(<Palette offers={MANY} onRun={ok} />);
    await open();
    scrolled.mockClear();

    await userEvent.keyboard("{ArrowDown}{ArrowDown}");

    expect(scrolled).toHaveBeenCalledWith({ block: "nearest" });
    expect(aimed()).toBe("Switch to tab 2");
  });

  it("is still drawn where there is no scrolling to be done", async () => {
    // No `scrollIntoView` at all, which is jsdom's own state and a real webview's during a
    // teardown. A palette that threw here would be unusable rather than merely unscrolled.
    render(<Palette offers={MANY} onRun={ok} />);
    await open();

    await userEvent.keyboard("{ArrowDown}");

    expect(aimed()).toBe("Switch to tab 1");
  });
});

describe("a key held down", () => {
  const SENDS = ready("pane.sendkey", "Send F2 to the chat in front");

  /** A keystroke the browser is repeating because the finger has not come off it. */
  const held = (key: string) =>
    window.dispatchEvent(new KeyboardEvent("keydown", { key, repeat: true, bubbles: true }));

  it("does not flicker the palette or spray the key at the chat", async () => {
    // Without this the repeats alternate: open, hand back, open, hand back, for as long as
    // the finger rests on F2.
    const onRun = vi.fn(ok);
    render(<Palette offers={[...OFFERS, SENDS]} onRun={onRun} />);
    await open();

    held("F2");
    held("F2");
    held("F2");

    expect(onRun).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("does not open it either, so a rested finger is one palette and not a stream", () => {
    render(<Palette offers={[...OFFERS, SENDS]} onRun={ok} />);

    act(() => void held("F2"));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
