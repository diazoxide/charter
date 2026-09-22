import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ENDS_IT } from "./actions";
import { AlertsDrawer } from "./AlertsDrawer";
import { ApproveExtension } from "./ApproveExtension";
import { ApprovePlane } from "./ApprovePlane";
import { EndingChat } from "./EndingChat";
import { Extensions } from "./Extensions";
import { Health } from "./Doctor";
import { Palette } from "./Palette";
import { QuitWarning } from "./QuitWarning";
import { StartChat } from "./StartChat";
import { PinItem, UpdateItem } from "./Updates";

/**
 * **What a keyboard can reach in each of the window's modal surfaces — one file, because the
 * answer was decided once for the window** (charter-app#186).
 *
 * Two mechanisms decide where Tab goes inside a Radix modal, and the defect lives in the gap
 * between them.
 *
 * - **Radix's `FocusScope` handles the two EDGES of the scope and nothing else**
 *   (`@radix-ui/react-focus-scope`, `handleKeyDown`): on the first tabbable it acts on
 *   Shift+Tab and calls `focus(last)` itself, on the last it acts on Tab and calls
 *   `focus(first)`. In between it does not touch the event.
 * - **In between, the engine decides — and the engine is WebKit**, a WKWebView on macOS and
 *   WebKitGTK on Linux, because charter embeds the system WebView. WebKit leaves a form
 *   control out of the tab sequence unless macOS's full keyboard access is on. That is not a
 *   quirk of one runner: it is `HTMLFormControlElement::isKeyboardFocusable`, and it was
 *   measured in the real window in charter-app#176 before it was read in the source.
 *
 * So a `<button>` that is neither edge is reachable by neither, and every control in these
 * dialogs is a `<button>` — including Radix's checkbox and its radio rows, which render
 * `Primitive.button` rather than an `<input>`.
 *
 * **The fix is the engine's own escape hatch and not a handler of ours.** WebKit's rule, in
 * full, since r263447 (`[popover] Improve focus handling`, 2023-04-29, shipped in Safari 17
 * and WebKitGTK 2.42):
 *
 * ```cpp
 * bool HTMLFormControlElement::isKeyboardFocusable(const FocusEventData& focusEventData) const
 * {
 *     if (!!tabIndexSetExplicitly())
 *         return Element::isKeyboardFocusable(focusEventData);
 *     return isFocusable() && document().frame()
 *         && document().frame()->eventHandler().tabsToAllFormControls(focusEventData);
 * }
 * ```
 *
 * **A `tabindex` that is written down is not consulted against full keyboard access at all.**
 * That is why Radix's radio rows were always reachable — roving focus writes one on them — and
 * why nothing else was. Every button in a modal now says `tabIndex={0}`, which is the
 * platform's answer rather than charter's, and leaves the primitives' own behaviour untouched
 * (charter ADR 0037). `docs/ui-primitives.md` holds the reasoning.
 *
 * **Why this is measured here and not in a scenario.** A scenario cannot answer it. Measured in
 * charter-app#176: WebDriver key actions carry no implicit activation, so no key a spec sends
 * will ever press a button, and `browser.keys(["Shift", "Tab"])` is not delivered as a chord —
 * the Tab arrives with `shiftKey` unset, so Radix's edge handling never fires either. A
 * scenario can neither confirm nor refute reachability. jsdom can, as long as the engine's
 * rule is spelled rather than inherited — which is what {@link inWebKitsTabSequence} is, and
 * it is the one assumption in this file worth attacking.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The `<input>` types WebKit tabs to with full keyboard access off (`TextFieldInputType`). */
const TEXT_ENTRY = new Set(["text", "search", "url", "tel", "email", "password", "number"]);

/**
 * Whether WebKit puts this element in the tab sequence, with full keyboard access off.
 *
 * Written from the engine's own decision rather than from a description of it, so that the one
 * thing this whole file rests on is a claim a reviewer can check against WebKit's source:
 *
 * - **an explicit `tabindex` ends the question** — `HTMLFormControlElement::isKeyboardFocusable`
 *   hands straight over to `Element::isKeyboardFocusable`, which asks only whether the element
 *   is focusable and the index is not negative;
 * - **a form control without one is skipped** — that is the `tabsToAllFormControls` gate, and
 *   it is off unless macOS's full keyboard access is on;
 * - **except a text field**, which `TextFieldInputType::isKeyboardFocusable` answers for
 *   itself and never consults the gate — which is why Tab moves between text boxes in Safari
 *   and between nothing else;
 * - **anything else focusable is in**: a link, a `<summary>`, a `div` that was given an index.
 */
function inWebKitsTabSequence(el: HTMLElement): boolean {
  if (el.hasAttribute("disabled")) return false;
  if (el.tabIndex < 0) return false;
  if (el.hasAttribute("tabindex")) return true;
  const tag = el.tagName.toLowerCase();
  if (tag === "a") return el.hasAttribute("href");
  if (tag === "input") return TEXT_ENTRY.has((el as HTMLInputElement).type);
  if (tag === "button" || tag === "select") return false;
  return true;
}

/** The surface the keyboard is inside — the focus scope, which is what Radix acts on. */
function scopeOf(el: Element): HTMLElement | null {
  return el.closest<HTMLElement>('[role="dialog"], [role="alertdialog"]');
}

/** Everything in this surface that WebKit would stop at, in the order it would stop. */
function engineSequence(scope: HTMLElement): HTMLElement[] {
  return [...scope.querySelectorAll<HTMLElement>("*")].filter(inWebKitsTabSequence);
}

/**
 * Where the engine's sequence goes from here.
 *
 * **The focus is often somewhere the engine would never have stopped**, because the dialog put
 * it there itself: every one of these surfaces focuses an answer as it opens, and an answer is
 * a `<button>`. The engine still knows where that button sits in the document and carries on
 * from there, so the step is taken over document order rather than over an index — and when
 * there is nothing further, the answer is nothing: the focus scope pulls the keyboard back to
 * where it was rather than letting it leave for the window behind.
 */
function nextInSequence(order: HTMLElement[], from: HTMLElement, shift: boolean) {
  const at = order.indexOf(from);
  if (at >= 0) return order[at + (shift ? -1 : 1)];
  const side = shift ? Node.DOCUMENT_POSITION_PRECEDING : Node.DOCUMENT_POSITION_FOLLOWING;
  const beyond = order.filter((el) => from.compareDocumentPosition(el) & side);
  return shift ? beyond[beyond.length - 1] : beyond[0];
}

/**
 * One Tab press, in a window whose engine is WebKit.
 *
 * The key is dispatched for real, so the surface's own handlers decide first and Radix's edge
 * handling is exercised rather than modelled: when it acts it calls `focus()` itself and
 * prevents the default, and there is nothing left for the engine to do. Only when the event
 * comes back unprevented does the engine's own sequence move the focus — and when that sequence
 * has nowhere to go, the focus stays where it is, which is what the trap leaves you with.
 */
async function tab({ shift = false } = {}) {
  const from = document.activeElement as HTMLElement | null;
  if (!from) return;
  const scope = scopeOf(from);
  if (!scope) return;
  if (!fireEvent.keyDown(from, { key: "Tab", shiftKey: shift })) return;
  const next = nextInSequence(engineSequence(scope), from, shift);
  if (next) await act(async () => next.focus());
}

/** How a control reads to whoever is looking for it: its role, and the words it goes by. */
function said(el: Element): string {
  const role = el.getAttribute("role") ?? el.tagName.toLowerCase();
  const aria = el.getAttribute("aria-label");
  const by = el.getAttribute("aria-labelledby");
  const labelled = by ? document.getElementById(by)?.textContent : undefined;
  const labelFor = el.id ? document.querySelector(`label[for="${el.id}"]`)?.textContent : undefined;
  const named = aria ?? labelled ?? labelFor ?? el.textContent ?? "";
  return `${role} "${named.trim()}"`;
}

/**
 * Every control this surface's keyboard can reach, starting from wherever it opened.
 *
 * Tab until the focus comes round to something already seen, which is what a real operator
 * does to find out what is there. A control the keyboard cannot get to is simply missing from
 * the answer, and the assertion says which.
 */
async function reachableByKeyboard({ shift = false } = {}): Promise<string[]> {
  let at = document.activeElement as HTMLElement;
  const seen = [said(at)];
  let stuck = 0;
  for (let press = 0; press < 30; press += 1) {
    await tab({ shift });
    const now = document.activeElement as HTMLElement | null;
    if (!now || now === document.body) break;
    if (now === at) {
      // **A press that lands back where it started is not always the end of the line**, and
      // reading it as one is what made an early draft of this file call `Start` unreachable
      // when it was merely unfindable. Radix's roving `tabindex` is a render behind: stepping
      // into a radio group lands on its ROOT, whose `onFocus` hands the focus to a row, and
      // only the render after that takes the root out of the sequence and makes the row the
      // scope's edge. So the first press off that row goes nowhere and the second wraps. Two
      // in a row with no movement is stuck; one is the primitive catching up.
      if ((stuck += 1) === 2) break;
      continue;
    }
    stuck = 0;
    at = now;
    const words = said(now);
    if (seen.includes(words)) break;
    seen.push(words);
  }
  return seen;
}

const PROFILES = [
  {
    name: "claude",
    kind: "claude" as const,
    shown: "claude",
    source: "built-in",
    is_default: true,
    approval: null,
  },
];

function picker() {
  render(
    <StartChat
      options={{
        profiles: PROFILES,
        refused: [["broken", "declares no command"]],
        personas: ["steward"],
        persona: "steward",
        ignore_fix: null,
        declares_none: false,
      }}
      onStart={() => {}}
      onApprove={() => {}}
      onCancel={() => {}}
    />,
  );
}

describe("what a keyboard reaches in the window's modal surfaces", () => {
  it("reaches Start in the picker, which ADR 0022 makes the only way a chat begins", async () => {
    // **The defect charter-app#186 was filed for, and the measurement is narrower than the
    // ticket's wording — which is the reason to take it.** The picker's controls are a harness
    // radio group, a persona radio group, a footer checkbox, the refused list's `<summary>`,
    // then `Cancel` and `Start`. The radio groups carry a `tabindex` from Radix's roving focus
    // and a `<summary>` is not a form control, so those were in WebKit's sequence; the
    // checkbox and both answers, all three `<button>`s with nothing written down, were not.
    //
    // Walked before the attribute went on, this dialog answered: **Tab from `Cancel` moved
    // nowhere at all** — there is nothing after it in the engine's sequence and `Cancel` is
    // not an edge, so neither mechanism had anything to say — while **Shift+Tab eventually
    // reached `Start`**, five presses backwards through the whole form and out the far side by
    // Radix's first edge. So the ticket's `Start` was reachable in the strict sense and
    // unreachable in every sense that matters: not by the key an operator presses, and only
    // by walking a dialog backwards. The footer checkbox (charter ADR 0029's one choice) was
    // reachable by neither. What this test pins is the plain thing: **Tab, forwards, reaches
    // all six.**
    picker();

    expect(await reachableByKeyboard()).toEqual([
      'button "Cancel"',
      'button "Start"',
      'radio "claude"',
      'radio "steward"',
      'checkbox "draw charter\'s footer in this chat"',
      'summary "1 refused"',
    ]);
  });

  it("reaches all six the other way too, which is the half that half-worked before", async () => {
    // Shift+Tab was the only direction that went anywhere in this dialog, and it went most of
    // the way: everything but the footer checkbox, by the engine's sequence as far as the
    // first edge and by Radix's `focus(last)` after that. Asserted because "the fix did not
    // cost the direction that used to work" is not something the forward test can say, and
    // because the wrap is the one behaviour here that is Radix's rather than the engine's.
    picker();

    expect(await reachableByKeyboard({ shift: true })).toEqual([
      'button "Cancel"',
      'summary "1 refused"',
      'checkbox "draw charter\'s footer in this chat"',
      'radio "steward"',
      'radio "claude"',
      'button "Start"',
    ]);
  });

  it("reaches the approve-and-start answer too, which is the same button under another name", async () => {
    // A profile whose command line charter has not seen draws `Approve and start` where
    // `Start` was. It is a different element, so it is worth one assertion of its own rather
    // than an assumption that the branch above covers it.
    render(
      <StartChat
        options={{
          profiles: [{ ...PROFILES[0], approval: "new", shown: "claude --dangerous" }],
          refused: [],
          personas: [],
          persona: null,
          ignore_fix: null,
          declares_none: false,
        }}
        onStart={() => {}}
        onApprove={() => {}}
        onCancel={() => {}}
      />,
    );

    expect(await reachableByKeyboard()).toEqual([
      'button "Cancel"',
      'button "Approve and start"',
      'radio "claude"',
      'radio "none"',
      'checkbox "draw charter\'s footer in this chat"',
    ]);
  });

  it("starts a chat from the keyboard alone, which is the whole of what #186 is about", async () => {
    // The end of the route, not a step of it: reach `Start` by Tab and press it. jsdom is the
    // only place the press can be tested at all — a scenario's synthesised `Enter` does not
    // activate a focused button (charter-app#176) — so the reach and the press are asserted
    // together here rather than split across two rigs that each hold half a claim.
    const started: unknown[] = [];
    render(
      <StartChat
        options={{
          profiles: PROFILES,
          refused: [],
          personas: [],
          persona: null,
          ignore_fix: null,
          declares_none: false,
        }}
        onStart={(profile, persona, footer) => started.push({ profile, persona, footer })}
        onApprove={() => {}}
        onCancel={() => {}}
      />,
    );

    // Cancel is focused on opening and `Start` is the next control in the surface — one
    // press, and it is the press the engine used to refuse.
    await tab();
    expect(screen.getByRole("button", { name: "Start" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");

    expect(started).toEqual([{ profile: "claude", persona: null, footer: false }]);
  });

  it("reaches every row's Review and Remove in the extensions dialog", async () => {
    // **The worst of them, and the one nobody had looked at.** `Add an extension…` is the
    // scope's first edge and `Done` is its last, which left every installed row's two buttons
    // in the middle. Reviewing or removing an extension was a mouse-only act on the one
    // surface in the window that exists to be a consent decision (charter ADR 0041).
    mockIPC((cmd) => {
      if (cmd === "installed_extensions")
        return {
          extensions: [
            {
              id: "solarized",
              name: "Solarized",
              path: "/home/dev/ext/solarized",
              standing: "trusted",
              refused: null,
              themes_in_force: [],
              ask: {
                id: "solarized",
                name: "Solarized",
                path: "/home/dev/ext/solarized",
                declares: [],
                fingerprint: "a".repeat(64),
                first: false,
                runs_as_you: "runs as you do",
                fingerprint_note: "fingerprinted",
                state_note: null,
              },
            },
          ],
          built_in_themes: ["charter-dark"],
          dropped: [],
          unreadable: null,
        };
      if (cmd === "extension_themes") return [];
      return null;
    });
    render(<Extensions onClose={() => {}} />);
    await screen.findByRole("button", { name: "Review" });

    expect(await reachableByKeyboard()).toEqual([
      'button "Add an extension…"',
      'button "Review"',
      'button "Remove"',
      'button "Done"',
    ]);
  });

  it("reaches Check again in the doctor, past the summary that was hiding it", async () => {
    // Three tabbables whenever a row is unchecked — the `<summary>`, `Check again`, `Close` —
    // and `Check again` was the one in the middle. The `<summary>` is what makes this dialog
    // a good witness: it is not a form control, so the engine always stopped at it, and the
    // hole was on the far side of it.
    const rows = [
      { name: "git", status: "ok" as const, detail: "found", hint: "install git", checked: true },
      {
        name: "tmux",
        status: "warn" as const,
        detail: "not checked (not ported)",
        hint: "a later build checks this",
        checked: false,
      },
    ];
    render(
      <Health
        doctor={{
          running: false,
          run: () => {},
          report: { rows, app_rows: [], full: false, path: "/usr/bin:/bin" },
        }}
      />,
    );
    await userEvent.click(screen.getByTestId("status-doctor"));
    await screen.findByRole("dialog");

    expect(await reachableByKeyboard()).toEqual([
      'summary "Not checked by this build (1)"',
      'button "Check again"',
      'button "Close"',
    ]);
  });

  it("reaches Install in the update offer, which was the one act that ends every chat", async () => {
    // The channel radios are the scope's first edge and `Close` is its last, so `Install` and
    // `Check now` sat between them. Installing is the act that ends every running chat
    // (`INSTALL_ENDS_SESSIONS`), and it could only be reached with a mouse.
    render(
      <UpdateItem
        updates={{
          state: {
            kind: "offered",
            offer: { version: "0.2.0", current: "0.1.0", channel: "stable", notes: "Fixes." },
          },
          channel: "stable",
          check: () => {},
          install: () => {},
          choose: () => {},
        }}
      />,
    );
    await userEvent.click(screen.getByTestId("status-update"));
    await screen.findByRole("dialog");

    expect(await reachableByKeyboard()).toEqual([
      'radio "stable"',
      'button "Install 0.2.0"',
      'button "Check now"',
      'button "Close"',
    ]);
  });

  it("reaches both answers of every two-answer dialog, forwards", async () => {
    // These five were already whole, and for a reason that was never about them: their two
    // answers ARE the two edges Radix handles, so Shift+Tab from the first was Radix's own
    // `focus()` call. That made "the keyboard works here" a property of the NUMBER of buttons,
    // which a third control would have taken away in silence — and it left plain Tab, the key
    // an operator actually presses, doing nothing at all. Both are why the attribute goes on
    // a dialog that did not appear to need it.
    const two: [string, () => void, string[]][] = [
      [
        "the quit warning",
        () =>
          void render(
            <QuitWarning
              chats={[
                {
                  key: "a",
                  name: "session 1",
                  harness: "claude",
                  cwd: "/home/dev/plane",
                  state: "running",
                },
              ]}
              onQuit={() => {}}
              onCancel={() => {}}
            />,
          ),
        ['button "Cancel"', 'button "Quit charter"'],
      ],
      [
        "the first-open prompt",
        () =>
          void render(
            <ApprovePlane
              ask={{
                path: "/home/dev/plane",
                first: true,
                changes: [],
                contributes: { plugins: [], env: [], starts: [], profiles: [] },
              }}
              onApprove={() => {}}
              onCancel={() => {}}
            />,
          ),
        ['button "Cancel"', 'button "Open project"'],
      ],
      [
        "the extension prompt",
        () =>
          void render(
            <ApproveExtension
              ask={{
                id: "solarized",
                name: "Solarized",
                path: "/home/dev/ext/solarized",
                declares: ["a theme"],
                fingerprint: "a".repeat(64),
                first: true,
                runs_as_you: "runs as you do",
                fingerprint_note: "fingerprinted",
                state_note: null,
              }}
              onApprove={() => {}}
              onCancel={() => {}}
            />,
          ),
        ['button "Cancel"', 'button "Trust it"'],
      ],
      [
        "the question before a chat ends",
        () =>
          void render(
            <EndingChat
              offer={{
                id: "tab.close:1",
                title: "End chat 1 steward",
                available: true,
                reason: "",
                does: { verb: "closeTab", tab: 1 },
                note: ENDS_IT,
              }}
              onEnd={() => {}}
              onCancel={() => {}}
            />,
          ),
        ['button "Cancel"', 'button "End chat 1 steward"'],
      ],
    ];
    for (const [what, show, answers] of two) {
      show();
      expect(await reachableByKeyboard(), what).toEqual(answers);
      cleanup();
    }
  });

  it("reaches the one control of every surface that has one", async () => {
    // A surface with a single tabbable is its own first and last edge, so Radix would keep it
    // reachable whatever the engine did. They are here because the claim this file makes is
    // about the window and not about the dialogs that happened to be interesting: a list that
    // leaves surfaces out is a list the next person has to re-derive.
    render(
      <AlertsDrawer
        open
        onOpenChange={() => {}}
        reading={{ at: "read", planes: [{ plane: "/home/dev/plane", stopped: null, alerts: [] }] }}
        planes={["/home/dev/plane"]}
        nameOf={() => "plane"}
      />,
    );
    expect(await reachableByKeyboard()).toEqual(['button "Close"']);
    cleanup();

    render(
      <PinItem
        pin={{
          drift: true,
          pinned: "0.1.0",
          brought: "0.2.0",
          said: ["the plane pins an older charter"],
          news: [],
          more_news: 0,
        }}
        again={() => {}}
      />,
    );
    await userEvent.click(screen.getByTestId("status-pin"));
    await screen.findByRole("dialog");
    expect(await reachableByKeyboard()).toEqual(['button "Close"']);
  });

  it("reaches the palette's box, which was never in doubt and says why", async () => {
    // The one modal in the window that needed nothing. Its only tabbable is a text `<input>`,
    // and `TextFieldInputType::isKeyboardFocusable` answers for itself without ever consulting
    // full keyboard access — which is exactly why Tab moves between text boxes in Safari and
    // between nothing else. It is the control case for {@link inWebKitsTabSequence}: if this
    // test ever needs a `tabindex` to pass, the model above has drifted from the engine.
    render(<Palette offers={[]} onRun={() => ({ ok: true }) as const} />);
    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog");

    expect(await reachableByKeyboard()).toEqual(['combobox "Run an action"']);
  });

  it("keeps the keyboard inside the surface when there is nowhere further to go", async () => {
    // The trap, asserted once rather than relied on eleven times above. `reachableByKeyboard`
    // reads a repeat as "it has come round", and a focus that escaped to the document would
    // read the same way — so this is what separates the two: Tab off the last answer lands on
    // the first, by Radix's own `focus(first)`, and never on `<body>`.
    picker();
    const start = screen.getByRole("button", { name: "Start" });
    await act(async () => start.focus());

    await tab();

    expect(document.activeElement).not.toBe(document.body);
    await waitFor(() => expect(screen.getByRole("radio", { name: "claude" })).toHaveFocus());
  });
});
