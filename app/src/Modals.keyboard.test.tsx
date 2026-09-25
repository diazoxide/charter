import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ENDS_IT } from "./actions";
import { AboutCharter } from "./About";
import { AlertsDrawer } from "./AlertsDrawer";
import { DeleteWorkspace } from "./DeleteWorkspace";
import { NewProject } from "./NewProject";
import { NewWorkspace } from "./NewWorkspace";
import { ApproveExtension } from "./ApproveExtension";
import { ApprovePlane } from "./ApprovePlane";
import { EndingChat } from "./EndingChat";
import { Extensions } from "./Extensions";
import { Health } from "./Doctor";
import { Palette } from "./Palette";
import { QuitWarning } from "./QuitWarning";
import { RelaunchAsk } from "./RelaunchAsk";
import { StartChat } from "./StartChat";
import { PinItem, UpdateItem } from "./Updates";
import { sequenceIn } from "./tabSequence";

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
 * (ADR 0037). `docs/ui-primitives.md` holds the reasoning.
 *
 * **Why this is measured here and not in a scenario — which was tried, in a real window, and
 * cannot be done.** The obvious objection to this whole file is that jsdom is not WebKit, so a
 * spec was written to walk the picker with `browser.keys(["Tab"])` and read
 * `document.activeElement` — an unmodified key and a DOM read, neither of which runs into what
 * charter-app#176 measured. It reported that Tab reached nothing at all, including the radio
 * rows, which have carried an explicit `tabindex` from Radix's roving focus since long before
 * this change and were never in doubt. That was the tell. Two plain text `<input>`s injected
 * into the running app settled it: **Tab does not move the focus between them either**, while
 * the keydown arrives unprevented. This driver dispatches a synthetic DOM event and performs no
 * default action of any kind, which is one fact wearing the three faces #176 caught it in.
 * `docs/ui-primitives.md` has the trace.
 *
 * So a scenario can neither confirm nor refute reachability, and the half it CAN prove is kept
 * in `picker.e2e.ts`: that the attribute the engine's rule needs survives the build and is on
 * the element in the shipped app, which is the one thing jsdom cannot see. What is left
 * unproven anywhere is the step between the two — that WebKit, handed the attribute, then tabs
 * to it. That is `inWebKitsTabSequence` (`tabSequence.ts`), it is read from the engine's source rather than
 * measured in a running one, and it is the one assumption in this file worth attacking.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

// `inWebKitsTabSequence` — the engine's rule written down — lives in `tabSequence.ts` since
// charter-app#189, because a terminal's Ctrl+Tab walks the same sequence in the shipped window.

/** The surface the keyboard is inside — the focus scope, which is what Radix acts on. */
function scopeOf(el: Element): HTMLElement | null {
  return el.closest<HTMLElement>('[role="dialog"], [role="alertdialog"]');
}

/** Everything in this surface that WebKit would stop at, in the order it would stop. */
function engineSequence(scope: HTMLElement): HTMLElement[] {
  return sequenceIn(scope);
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
  // **The elements, and the words only at the end** (charter-app#197). This used to remember
  // where it had been by what each control is CALLED, and that is a false positive waiting for
  // a dialog with two controls of the same name: `NewProject` grew a second directory picker,
  // its second `Browse…` read as a place already visited, the walk stopped there, and the
  // three controls after it were reported unreachable when every one of them was fine. A guard
  // that says a working dialog is broken is worse than no guard, because the obvious repair is
  // to weaken the expectation until it passes.
  const visited = [at];
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
    if (visited.includes(now)) break;
    visited.push(now);
  }
  return visited.map(said);
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
    // by walking a dialog backwards. The footer checkbox (ADR 0029's one choice) was
    // reachable by neither. What this test pins is the plain thing: **Tab, forwards, reaches
    // all seven**, the Name field (charter-app#254) among them.
    picker();

    expect(await reachableByKeyboard()).toEqual([
      'button "Cancel"',
      'button "Start"',
      'radio "claude"',
      'radio "steward"',
      'checkbox "draw charter\'s footer in this chat"',
      // The Name field (charter-app#254): an `<input>`, which every engine puts in the sequence.
      'input "Name"',
      'summary "1 refused"',
    ]);
  });

  it("reaches all seven the other way too, which is the half that half-worked before", async () => {
    // Shift+Tab was the only direction that went anywhere in this dialog, and it went most of
    // the way: everything but the footer checkbox, by the engine's sequence as far as the
    // first edge and by Radix's `focus(last)` after that. Asserted because "the fix did not
    // cost the direction that used to work" is not something the forward test can say, and
    // because the wrap is the one behaviour here that is Radix's rather than the engine's.
    picker();

    expect(await reachableByKeyboard({ shift: true })).toEqual([
      'button "Cancel"',
      'summary "1 refused"',
      'input "Name"',
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
      'input "Name"',
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
    // surface in the window that exists to be a consent decision (ADR 0041).
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
          restart: () => {},
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

  it("reaches Restart to update, and both answers of the ask about a chat mid-turn", async () => {
    // The restart ends every chat, so it is the same kind of act Install was — and the ask in
    // front of it has the safe answer first.
    render(
      <UpdateItem
        updates={{
          state: { kind: "installed", version: "0.2.0" },
          channel: "stable",
          check: () => {},
          install: () => {},
          choose: () => {},
          restart: () => {},
        }}
        chats={[{ key: "a/1", name: "ide.1", harness: "claude", cwd: null, state: "running" }]}
      />,
    );
    await userEvent.click(screen.getByTestId("status-update"));
    await screen.findByRole("dialog");

    expect(await reachableByKeyboard()).toEqual([
      'radio "stable"',
      'button "Restart to update"',
      'button "Close"',
    ]);

    await userEvent.click(screen.getByRole("button", { name: "Restart to update" }));
    await screen.findByRole("alertdialog");
    await waitFor(() => expect(screen.getByRole("button", { name: "Wait" })).toHaveFocus());

    expect(await reachableByKeyboard()).toEqual(['button "Wait"', 'button "Restart now"']);
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
                title: "End chat steward 1",
                available: true,
                reason: "",
                does: { verb: "closeTab", tab: 1, ends: true },
                note: ENDS_IT,
              }}
              onEnd={() => {}}
              onCancel={() => {}}
            />,
          ),
        ['button "Cancel"', 'button "End chat steward 1"'],
      ],
      [
        "the question a relaunch asks",
        () =>
          void render(
            <RelaunchAsk
              question={{
                projects: [{ plane: "/home/dev/plane", chats: 2, views: 0 }],
                after_update: false,
              }}
              nameOf={(plane) => plane}
              onAnswer={() => {}}
            />,
          ),
        ['button "Reopen all sessions"', 'button "Start fresh"'],
      ],
      [
        "the question a relaunch asks",
        () =>
          void render(
            <RelaunchAsk
              question={{
                projects: [{ plane: "/home/dev/plane", chats: 2, views: 0 }],
                after_update: false,
              }}
              nameOf={(plane) => plane}
              onAnswer={() => {}}
            />,
          ),
        ['button "Reopen all sessions"', 'button "Start fresh"'],
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
    cleanup();

    // **About charter**, which arrived with the title bar. It is here on the day it was
    // written rather than after somebody noticed, which is the whole argument this file makes:
    // the hole is not a mistake anybody made, it is what a modal in a WebView does by default.
    // Its command is not mocked and it does not need to be — a dialog that could not read the
    // changelog draws the refusal, and either way it has the one control this walk is about.
    render(<AboutCharter />);
    await userEvent.click(screen.getByTestId("title-about"));
    await screen.findByRole("dialog");
    expect(await reachableByKeyboard()).toEqual(['button "Close"']);
  });

  it("reaches the three surfaces that arrived while this was being measured", async () => {
    // **#172 landed the workspace lifecycle between the survey and this branch, and every one
    // of its dialogs had the defect.** That is the argument for a file like this one rather
    // than a fix per dialog: the hole is not a mistake anybody made, it is what a modal in a
    // WebView does by default, so a new surface has it on the day it is written.
    //
    // `NewProject` is the sharpest of the three. Only its folder box was in the engine's
    // sequence at all — a text `<input>` — and it is the scope's first edge, with `Cancel` the
    // last. `Browse…`, the checkbox that decides whether charter writes into a repository the
    // operator already has, and `Create project` were all in between, reachable by nothing.
    //
    // **And it grew a control after the sweep, which is the second half of the argument**
    // (charter-app#197, ADR 0035's adopt default). A second directory picker arrived with no
    // `tabIndex={0}` on its `Browse…`, because the sweep was over by the time it was written —
    // so the class of defect came back on the one dialog that had just been fixed for it. It
    // is the case for a walk that enumerates rather than a rule somebody has to remember.
    //
    // The two `Browse…` buttons carry `aria-label`s naming the box each one fills, which is
    // what stops a screen reader announcing the same three words for two different pickers —
    // and it is also why this walk can tell them apart in its answer.
    render(<NewProject making={false} onCreate={() => {}} onCancel={() => {}} />);
    // Answered first: both dialogs disable their create button until they have been, and a
    // disabled control is out of the tab sequence everywhere and rightly so. The state worth
    // measuring is the one where the answer can be given.
    await userEvent.type(screen.getByLabelText("Folder"), "/where/it/goes");
    expect(await reachableByKeyboard()).toEqual([
      'input "Folder"',
      'button "Browse for the folder"',
      'input "Repository to adopt"',
      'button "Browse for the repository to adopt"',
      'checkbox "Make this repo itself the plane"',
      'button "Create project"',
      'button "Cancel"',
    ]);
    cleanup();

    // Two text boxes were reachable and the button that acts on them was not.
    render(<NewWorkspace plane="plane" making={false} onCreate={() => {}} onCancel={() => {}} />);
    await userEvent.type(screen.getByLabelText("Name"), "svc");
    expect(await reachableByKeyboard()).toEqual([
      'input "Name"',
      'textarea "What it is for (optional)"',
      'checkbox "Live"',
      'button "Create workspace"',
      'button "Cancel"',
    ]);
    cleanup();

    // Two answers, so the edges covered it — in one direction, and only while there are two.
    render(
      <DeleteWorkspace
        workspace="svc"
        atRisk={[]}
        deleting={false}
        onDelete={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(await reachableByKeyboard()).toEqual(['button "Cancel"', 'button "Delete workspace"']);
  });

  it("reaches the palette's box, which was never in doubt and says why", async () => {
    // The one modal in the window that needed nothing. Its only tabbable is a text `<input>`,
    // and `TextFieldInputType::isKeyboardFocusable` answers for itself without ever consulting
    // full keyboard access — which is exactly why Tab moves between text boxes in Safari and
    // between nothing else. It is the control case for `inWebKitsTabSequence` (`tabSequence.ts`): if this
    // test ever needs a `tabindex` to pass, the model above has drifted from the engine.
    render(<Palette offers={[]} onRun={() => ({ ok: true }) as const} />);
    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog");

    expect(await reachableByKeyboard()).toEqual(['combobox "Run an action"']);
  });

  it("keeps the keyboard inside the surface when there is nowhere further to go", async () => {
    // The trap, asserted once rather than relied on by every walk above. `reachableByKeyboard`
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
