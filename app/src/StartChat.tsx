import { useId, useRef, useState } from "react";
import * as Checkbox from "@radix-ui/react-checkbox";
import * as Dialog from "@radix-ui/react-dialog";
import * as RadioGroup from "@radix-ui/react-radio-group";
import { ApprovalSentence, ProfileMeta } from "./ProfileApproval";
import type { ProfileRow, StartOptions, WithoutSandbox } from "./bindings";

/**
 * The value that stands for "no persona at all".
 *
 * A radio group's values are strings and one of them has to mean nothing. The empty string is
 * the one value no persona can have — a persona is a directory under `personas/`, and a
 * directory has a name — so it cannot collide with a real row the way a sentinel like
 * `__none__` could.
 */
const NO_PERSONA = "";

/**
 * What a new chat asks before anything runs.
 *
 * **No harness starts until somebody picks a profile** (ADR 0022). It shows even when one
 * profile is available, because skipping it would bring back the harness nobody picked on a
 * one-harness machine, and one profile costs one Enter. Escape closes it having started
 * nothing — no harness ran, and no identity was recorded.
 *
 * A dialog rather than a pane that draws itself: the tmux frame put the selector in the
 * chat's own pane because tmux had already made the pane, and here there is no chat yet.
 * Nothing is opened until a row is picked, so cancelling leaves nothing to tear down.
 *
 * A profile whose command charter has not recorded running shows that command and asks. The
 * file is gitignored, so an edit to it leaves no diff for a reviewer to catch, and nothing
 * stops a chat editing plane config — which is why the ask is about the words that are
 * about to run and not about the profile's name.
 *
 * **The footer checkbox is asked here and nowhere else** (ADR 0029). Inside a pane
 * charter prints an empty line where its own footer would go, because the app's panels
 * already draw the plane — and this is where an operator says "not this chat". The choice is
 * between charter's footer and nothing: `charter statusline` IS Claude Code's `statusLine`
 * command, so a blank line there leaves the harness with no status line at all.
 *
 * It is asked at the start rather than offered as a switch on a running pane because that
 * command inherits the environment its harness was exec'd with: a toggle on a live chat would
 * appear to work and would not.
 *
 * **The Name field is optional** (charter-app#254). Empty is the default — the persona and the
 * chat's number, `steward 3` — and a name typed here is what the tab says instead. It is
 * charter's label for the chat, not the harness's own name, and the core is what refuses one
 * it will not draw: its refusal comes back into this dialog, before anything has started.
 *
 * **A chat that starts in a repo starts on a new branch in it, unless the box is cleared**
 * (GL-1, ADR 0072 §4). `repo` names the repo whose clone this chat would start in, and only
 * then is the box drawn — ticked, because two chats writing in one working tree is the case
 * this exists to end. Cleared, the chat works on whatever branch the repo has checked out,
 * shared with every other chat there. The core cuts the branch and takes it back if the start
 * is refused, so cancelling here or being refused leaves nothing behind. Not remembered between
 * chats, for the footer box's reason: a box that silently stayed cleared would be a setting.
 *
 * **"Start without the sandbox" is asked here, for this one chat** (ADR 0067 §7, ruling V78 a).
 * Shown only where the project turned the sandbox on, beside what the sandbox does for the
 * picked profile: on, or the reason it cannot be applied here — a missing program, with the
 * distribution's install command (SD-30), or a system with no backend. Where it cannot be
 * applied, the one way on is to start without it, said on the button itself. Not remembered
 * between chats, for the footer box's reason, and never offered by anything but this picker:
 * no CLI word, file or chat makes one, and charter records who turned it off and why.
 *
 * **Every control here is a Radix primitive** (`docs/ui-primitives.md`). It was hand-rolled
 * markup, and the hand-rolling is what broke it: five spans in one `<label>` with no rule to
 * lay them out ran together into `claudeclaudeclaudebuilt-indefault`, and the accessible name
 * of every row was that same run of words. A row's name is now its name, the rest of the row
 * describes it, and the layout is a grid rather than whatever the spans fell into.
 *
 * **Every radio row picks itself on focus, and that is not decoration.** A radio group's pick
 * follows the keyboard — an arrow moves to the next row AND chooses it, which is what the
 * native inputs here did for nothing. Radix means to do it too: `RadioGroupItem` selects on
 * focus while an arrow key is down, and it learns that an arrow key is down from a `keydown`
 * listener it adds to `document`. It never learns it here, and the reason is not this app's.
 * React attaches its delegated listeners to the root container and to each portal container,
 * both of which are BELOW `document`, so one arrow press runs `document` capture, then React's
 * handler — which moves the focus — and only then `document` bubble, where Radix would have
 * set its flag. Measured rather than assumed: the three listeners were logged in that order,
 * the focus moved to the next row, and `onValueChange` was never called. Nothing about jsdom
 * causes it; the same nesting holds in the webview. Focus can only arrive at a row here by
 * arrow or by pointer — a roving tabindex is entered at the row that is already checked, so
 * tabbing in picks what was already picked — which makes "focused" and "picked" the same thing
 * for this control rather than a second behaviour. "moves between harnesses with the arrow
 * keys" goes red the moment an `onFocus` comes off a row.
 */
export function StartChat({
  options,
  repo,
  starting = false,
  prefer,
  trouble,
  onStart,
  onApprove,
  onInstall,
  onCancel,
}: {
  options: StartOptions;
  /** The repo whose clone this chat would start in, when it would start in one. */
  repo?: string;
  /** Whether a start from this picker is running: Start says so and cannot be pressed again,
   *  because a second press was a second chat (GL-1). */
  starting?: boolean;
  /**
   * The harness (a profile's `kind`) to start on, when something already knows which one is
   * wanted — a harness started by hand in a shell tab, opened as a chat instead (ADR 0062).
   * The default profile when it is of that kind, else the first that is; the default when none
   * is. It picks a row and starts nothing: the operator still presses Start.
   */
  prefer?: string;
  /** Why the last attempt did not start, if it did not. */
  trouble?: string;
  /** `label` is the Name field, or `null` when it was left empty. `newBranch` is the branch
   *  box, and `false` when there is none to tick. `withoutSandbox` is the person's opt-out for
   *  this chat, with the reason typed, or `null` for a sandboxed start. */
  onStart: (
    profile: string,
    persona: string | null,
    showFooter: boolean,
    label: string | null,
    newBranch: boolean,
    withoutSandbox: WithoutSandbox | null,
  ) => void;
  /** The profile, the persona, the footer choice, and the exact line the operator read — so
   *  the approval is for what was on screen and not for whatever the file says by the time
   *  it is clicked — and the Name field, as `onStart` has it. */
  onApprove: (
    profile: string,
    persona: string | null,
    showFooter: boolean,
    shown: string,
    label: string | null,
    newBranch: boolean,
    withoutSandbox: WithoutSandbox | null,
  ) => void;
  /** SD-30's install action: the window opens a shell tab at the project root with the
   *  install command typed and not run (ruling V78 c). Absent, the command is only shown. */
  onInstall?: () => void;
  onCancel: () => void;
}) {
  const [profile, setProfile] = useState<string | undefined>(() => {
    const ofKind = options.profiles.filter((p) => prefer !== undefined && p.kind === prefer);
    return (
      (ofKind.find((p) => p.is_default) ?? ofKind[0])?.name ??
      options.profiles.find((p) => p.is_default)?.name ??
      options.profiles[0]?.name
    );
  });
  // Only a persona there is a row for. The core already filters `[persona] default`
  // against the personas the plane has, so this should be unreachable from the app — but
  // the alternative, if it ever arrives, is a chat started on a persona the operator can
  // neither see nor change, and refused for it a moment later.
  const [persona, setPersona] = useState<string | null>(
    options.persona !== null && options.personas.includes(options.persona) ? options.persona : null,
  );
  // Off, which is the app as it has always behaved: a pane's footer is blank unless
  // this chat asks for it (ADR 0029). Not remembered between chats on purpose —
  // there is no plane-wide or machine-wide setting for it, and a box that silently stayed
  // ticked would be one.
  const [showFooter, setShowFooter] = useState(false);
  // Ticked: a chat that starts in a repo gets a branch of its own by default (GL-1). What is
  // sent is the box AND that there was one, so a chat started outside every repo never asks
  // the core for a branch.
  const [onABranch, setOnABranch] = useState(true);
  const newBranch = repo !== undefined && onABranch;
  // What the Name field says. Sent as typed — the core trims it and holds it to its rule — and
  // as nothing at all when there is nothing in it but spaces, which is "the default".
  const [name, setName] = useState("");
  const label = name.trim() === "" ? null : name;
  const nameId = useId();
  const picked = options.profiles.find((p) => p.name === profile);
  // What the sandbox does for a chat on the picked profile, where the project turned it on.
  const sandbox = picked?.sandbox ?? null;
  // The opt-out box, for a chat the sandbox would hold. Off, and not remembered between chats.
  const [optedOut, setOptedOut] = useState(false);
  // Where the sandbox cannot be applied, the start IS the opt-out, and the button says so.
  const refused = sandbox?.state === "refused";
  const [reason, setReason] = useState("");
  const reasonId = useId();
  const withoutSandbox: WithoutSandbox | null =
    (sandbox?.state === "sandboxed" && optedOut) || refused
      ? { reason: reason.trim() === "" ? null : reason }
      : null;
  const startWord = withoutSandbox !== null ? "Start without the sandbox" : "Start";
  // Cancel, so the dialog can put the keyboard on it itself. React's `autoFocus` and the
  // focus trap's own opening move both aim at mount, and which of them lands last is not
  // something to leave to ordering: the trap is told to do nothing and this is focused here.
  const cancel = useRef<HTMLButtonElement>(null);

  return (
    // Escape starts nothing, and it is the dialog's own Escape rather than a listener on the
    // window: focus is trapped inside, so "wherever focus happens to be inside it" is now a
    // property of the surface rather than a thing this component has to arrange.
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          className="warning starting"
          aria-labelledby="start-chat"
          // A click outside answers nothing — Cancel and Escape are the two ways out, as they
          // have always been. Turned off explicitly rather than left to the default, so a
          // reviewer sees it was decided rather than inherited.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            // Cancel, and not the first thing in tab order: starting a chat runs a command
            // with nothing between the key and the exec, so it is never what a stray Return
            // key finds.
            e.preventDefault();
            cancel.current?.focus();
          }}
        >
          <Dialog.Title id="start-chat">Start a chat</Dialog.Title>

          {options.ignore_fix && (
            <p className="honest mid-turn" role="alert">
              git would carry <code>charter.local.toml</code>, so every profile it declares is
              refused until that is fixed: <code>{options.ignore_fix}</code>
            </p>
          )}
          {options.declares_none && !options.ignore_fix && (
            <p className="honest">
              {/* Said rather than shown as an empty list: a plane that declares nothing is the
                ordinary first state, not a fault, and the built-ins below still start. */}
              This plane declares no profiles of its own, so these are charter&apos;s built-ins.
              Declare your own in <code>charter.local.toml</code>, which stays on this machine.
            </p>
          )}

          {/* The group is the radio group itself, named by the heading above it. Not a
              `<fieldset>` around it: the primitive's rows are buttons rather than inputs, so
              a fieldset would add nothing but a second group with the same name in it. */}
          <h3 className="choices-name" id="pick-harness">
            Harness
          </h3>
          <RadioGroup.Root
            className="choices profiles"
            name="profile"
            value={profile ?? ""}
            onValueChange={setProfile}
            aria-labelledby="pick-harness"
          >
            {options.profiles.map((row) => (
              <Row key={row.name} row={row} onPick={setProfile} />
            ))}
          </RadioGroup.Root>

          <h3 className="choices-name" id="pick-persona">
            Persona
          </h3>
          <RadioGroup.Root
            className="choices personas"
            name="persona"
            value={persona ?? NO_PERSONA}
            onValueChange={(value) => setPersona(value === NO_PERSONA ? null : value)}
            aria-labelledby="pick-persona"
          >
            <div className="choice">
              <RadioGroup.Item
                className="dot"
                value={NO_PERSONA}
                id="persona-none"
                onFocus={() => setPersona(null)}
              >
                <RadioGroup.Indicator className="dot-mark" />
              </RadioGroup.Item>
              <label className="who" htmlFor="persona-none">
                none
              </label>
            </div>
            {options.personas.map((who) => (
              <div className="choice" key={who}>
                <RadioGroup.Item
                  className="dot"
                  value={who}
                  id={`persona-${who}`}
                  onFocus={() => setPersona(who)}
                  aria-describedby={who === options.persona ? `persona-${who}-default` : undefined}
                >
                  <RadioGroup.Indicator className="dot-mark" />
                </RadioGroup.Item>
                <label className="who" htmlFor={`persona-${who}`}>
                  {who}
                </label>
                {who === options.persona && (
                  <span className="meta" id={`persona-${who}-default`}>
                    <span className="what">plane default</span>
                  </span>
                )}
              </div>
            ))}
          </RadioGroup.Root>

          <h3 className="choices-name">Footer</h3>
          <div className="choices surface">
            <div className="choice">
              <Checkbox.Root
                className="box"
                name="pane-footer"
                id="pane-footer"
                // In the window's tab sequence, said out loud (`docs/ui-primitives.md`,
                // charter-app#186). Radix's checkbox is a `<button>`, and WebKit leaves a form
                // control out of the tab sequence unless its `tabindex` is written down. The
                // radio rows above already carry one from the roving focus; this is the same
                // attribute for the same reason.
                tabIndex={0}
                checked={showFooter}
                onCheckedChange={(checked) => setShowFooter(checked === true)}
                aria-describedby="pane-footer-why"
              >
                <Checkbox.Indicator className="box-mark">✓</Checkbox.Indicator>
              </Checkbox.Root>
              <label className="who" htmlFor="pane-footer">
                draw charter&apos;s footer in this chat
              </label>
              {/* Said rather than left to be discovered. The reason for the default and the
                reason against it both belong on screen: the panels repeat most of what the
                footer says, and the footer says it about THIS chat's own workspace. */}
              <span className="meta" id="pane-footer-why">
                <span className="what">
                  blank by default, because the panels already draw the plane. The footer says which
                  workspace this chat is on, which the panels say only for the focused one. This
                  chat only, and only from its next start.
                </span>
              </span>
            </div>
          </div>

          {repo !== undefined && (
            <>
              <h3 className="choices-name">Branch</h3>
              <div className="choices surface">
                <div className="choice">
                  <Checkbox.Root
                    className="box"
                    name="new-branch"
                    id="new-branch"
                    // In the tab sequence, said out loud, for the footer box's reason above.
                    tabIndex={0}
                    checked={onABranch}
                    onCheckedChange={(checked) => setOnABranch(checked === true)}
                    aria-describedby="new-branch-why"
                  >
                    <Checkbox.Indicator className="box-mark">✓</Checkbox.Indicator>
                  </Checkbox.Root>
                  <label className="who" htmlFor="new-branch">
                    start on a new branch in {repo}
                  </label>
                  <span className="meta" id="new-branch-why">
                    <span className="what">
                      its own branch and folder, cut from what {repo} has checked out, so this
                      chat&apos;s changes stay apart from other chats&apos;. Named after the chat,
                      or chat-1, chat-2 and on. Cleared, it works on the branch {repo} has checked
                      out, shared with every chat there.
                    </span>
                  </span>
                </div>
              </div>
            </>
          )}

          <h3 className="choices-name">
            <label htmlFor={nameId}>Name</label>
          </h3>
          <div className="asks">
            <input
              id={nameId}
              value={name}
              autoComplete="off"
              spellCheck={false}
              aria-describedby={`${nameId}-why`}
              onChange={(event) => setName(event.target.value)}
            />
            <p className="came-back" id={`${nameId}-why`}>
              Optional. Left empty, the chat is named after its persona, or its harness, and its
              number. You can rename it from its tab later.
            </p>
          </div>

          {sandbox !== null && (
            <>
              <h3 className="choices-name">Sandbox</h3>
              {sandbox.state === "sandboxed" && (
                <div className="choices surface">
                  <div className="choice">
                    <Checkbox.Root
                      className="box"
                      name="no-sandbox"
                      id="no-sandbox"
                      // In the tab sequence, said out loud, for the footer box's reason above.
                      tabIndex={0}
                      checked={optedOut}
                      onCheckedChange={(checked) => setOptedOut(checked === true)}
                      aria-describedby="no-sandbox-why"
                    >
                      <Checkbox.Indicator className="box-mark">✓</Checkbox.Indicator>
                    </Checkbox.Root>
                    <label className="who" htmlFor="no-sandbox">
                      start without the sandbox
                    </label>
                    <span className="meta" id="no-sandbox-why">
                      <span className="what">
                        this project runs every chat sandboxed: it reaches only the hosts the
                        project allows, and never your vaults. Ticked, this one chat runs without
                        it, its tab says so, and charter records that you turned it off. Nothing
                        inherits it: a relaunch or a resume asks the sandbox again.
                      </span>
                    </span>
                  </div>
                </div>
              )}
              {/* Sandboxed, with what the picker could not check yet: a profile nobody has
                  approved is not run, not even to ask its version (ruling V87g). */}
              {sandbox.state === "sandboxed" && sandbox.said !== "" && (
                <p className="honest">{sandbox.said}</p>
              )}
              {sandbox.state === "unsandboxed" && <p className="honest">{sandbox.said}</p>}
              {refused && (
                <>
                  <p className="honest mid-turn" role="alert">
                    {sandbox.said}
                  </p>
                  {sandbox.install !== null && (
                    <p className="honest">
                      Install it with <code>{sandbox.install}</code>. It needs sudo, so charter
                      types it in a shell tab and leaves running it to you.{" "}
                      {onInstall && (
                        <button type="button" className="dismiss" tabIndex={0} onClick={onInstall}>
                          Type it in a shell tab
                        </button>
                      )}
                    </p>
                  )}
                </>
              )}
              {withoutSandbox !== null && (
                <div className="asks">
                  <input
                    id={reasonId}
                    value={reason}
                    autoComplete="off"
                    spellCheck={false}
                    aria-label="Why, if you want it recorded"
                    aria-describedby={`${reasonId}-why`}
                    onChange={(event) => setReason(event.target.value)}
                  />
                  <p className="came-back" id={`${reasonId}-why`}>
                    Optional. Kept on this machine with the record that this chat ran without the
                    sandbox.
                  </p>
                </div>
              )}
            </>
          )}

          {options.refused.length > 0 && (
            <details className="refused">
              {/* A missing profile is a row that is not in the list — easy to miss in a way a
                missing panel is not — so the ones charter will not use say why. */}
              <summary>{options.refused.length} refused</summary>
              <ul>
                {options.refused.map(([name, why]) => (
                  <li key={name}>
                    <span className="who">{name}</span> {why}
                  </li>
                ))}
              </ul>
            </details>
          )}

          <ApprovalSentence row={picked} />
          {trouble && (
            <p className="honest mid-turn" role="alert">
              {trouble}
            </p>
          )}

          {/* **Every button here says `tabIndex={0}`, and on this dialog it is what makes
              `Start` reachable at all** (charter-app#186). WebKit leaves a `<button>` out of
              the tab sequence unless its `tabindex` is written down, and Radix's focus scope
              only acts at the scope's two edges — so `Cancel`, which is neither edge nor
              engine-tabbable, could be reached only by being focused on opening, and a
              keyboard that left it could not come back. ADR 0022 makes this dialog the only
              way a chat starts, so that was the keyboard-only path to starting one.
              `docs/ui-primitives.md` holds the measurement and the engine's own rule. */}
          <div className="answer">
            {/* Cancel first and focused: see `onOpenAutoFocus` above. */}
            <button ref={cancel} tabIndex={0} onClick={onCancel}>
              Cancel
            </button>
            {picked?.approval ? (
              <button
                className="ends-it"
                tabIndex={0}
                onClick={() =>
                  onApprove(
                    picked.name,
                    persona,
                    showFooter,
                    picked.shown,
                    label,
                    newBranch,
                    withoutSandbox,
                  )
                }
                disabled={!picked || starting}
              >
                {starting ? "Starting…" : `Approve and ${startWord.toLowerCase()}`}
              </button>
            ) : (
              <button
                tabIndex={0}
                onClick={() =>
                  profile && onStart(profile, persona, showFooter, label, newBranch, withoutSandbox)
                }
                disabled={!profile || starting}
              >
                {starting ? "Starting…" : startWord}
              </button>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * One harness profile, as a row of the group.
 *
 * **Its accessible name is the profile's name and nothing else.** Everything else the row
 * shows — what kind of harness it is, the command line, where it was declared, whether it is
 * the default, whether it needs approving — describes that name rather than joining it, so a
 * screen reader announces "work, radio" and then the detail, instead of reading a run-on of
 * every column in the row.
 */
function Row({ row, onPick }: { row: ProfileRow; onPick: (name: string) => void }) {
  const id = `profile-${row.name}`;
  return (
    <div className="choice">
      <RadioGroup.Item
        className="dot"
        value={row.name}
        id={id}
        onFocus={() => onPick(row.name)}
        aria-describedby={`${id}-meta`}
      >
        <RadioGroup.Indicator className="dot-mark" />
      </RadioGroup.Item>
      <label className="who" htmlFor={id}>
        {row.name}
      </label>
      <ProfileMeta row={row} id={`${id}-meta`} />
    </div>
  );
}
