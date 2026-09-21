import { useRef, useState } from "react";
import * as Checkbox from "@radix-ui/react-checkbox";
import * as Dialog from "@radix-ui/react-dialog";
import * as RadioGroup from "@radix-ui/react-radio-group";
import type { ProfileRow, StartOptions } from "./bindings";

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
 * **The footer checkbox is asked here and nowhere else** (charter ADR 0029). Inside a pane
 * charter prints an empty line where its own footer would go, because the app's panels
 * already draw the plane — and this is where an operator says "not this chat". The choice is
 * between charter's footer and nothing: `charter statusline` IS Claude Code's `statusLine`
 * command, so a blank line there leaves the harness with no status line at all.
 *
 * It is asked at the start rather than offered as a switch on a running pane because that
 * command inherits the environment its harness was exec'd with: a toggle on a live chat would
 * appear to work and would not.
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
  trouble,
  onStart,
  onApprove,
  onCancel,
}: {
  options: StartOptions;
  /** Why the last attempt did not start, if it did not. */
  trouble?: string;
  onStart: (profile: string, persona: string | null, showFooter: boolean) => void;
  /** The profile, the persona, the footer choice, and the exact line the operator read — so
   *  the approval is for what was on screen and not for whatever the file says by the time
   *  it is clicked. */
  onApprove: (profile: string, persona: string | null, showFooter: boolean, shown: string) => void;
  onCancel: () => void;
}) {
  const [profile, setProfile] = useState<string | undefined>(
    () => options.profiles.find((p) => p.is_default)?.name ?? options.profiles[0]?.name,
  );
  // Only a persona there is a row for. The core already filters `[persona] default`
  // against the personas the plane has, so this should be unreachable from the app — but
  // the alternative, if it ever arrives, is a chat started on a persona the operator can
  // neither see nor change, and refused for it a moment later.
  const [persona, setPersona] = useState<string | null>(
    options.persona !== null && options.personas.includes(options.persona) ? options.persona : null,
  );
  // Off, which is the app as it has always behaved: a pane's footer is blank unless
  // this chat asks for it (charter ADR 0029). Not remembered between chats on purpose —
  // there is no plane-wide or machine-wide setting for it, and a box that silently stayed
  // ticked would be one.
  const [showFooter, setShowFooter] = useState(false);
  const picked = options.profiles.find((p) => p.name === profile);
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

          {picked?.approval && (
            <p className="honest approve" role="alert">
              charter has not run this profile{" "}
              {picked.approval === "new" ? "before" : "as it now stands"}. It would run:{" "}
              <code>{picked.shown}</code>
            </p>
          )}
          {trouble && (
            <p className="honest mid-turn" role="alert">
              {trouble}
            </p>
          )}

          <div className="answer">
            {/* Cancel first and focused: see `onOpenAutoFocus` above. */}
            <button ref={cancel} onClick={onCancel}>
              Cancel
            </button>
            {picked?.approval ? (
              <button
                className="ends-it"
                onClick={() => onApprove(picked.name, persona, showFooter, picked.shown)}
                disabled={!picked}
              >
                Approve and start
              </button>
            ) : (
              <button
                onClick={() => profile && onStart(profile, persona, showFooter)}
                disabled={!profile}
              >
                Start
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
      <span className="meta" id={`${id}-meta`}>
        <span className="what">{row.kind}</span>
        {/* Already contained by the core: a profile is a file a chat can write, and a control
          byte in a command must never redraw this row. */}
        <code className="where">{row.shown}</code>
        <span className="from">{row.source}</span>
        {row.is_default && <span className="what">default</span>}
        {row.approval && <span className="what needs-approval">{row.approval}</span>}
      </span>
    </div>
  );
}
