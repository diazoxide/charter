import { useRef, useState } from "react";
import { PersonaMark } from "./PersonaMark";
import * as Dialog from "@radix-ui/react-dialog";
import { HarnessSummary } from "./HarnessCard";
import { ApprovalSentence, ProfileMeta } from "./ProfileApproval";
import type { StartOptions, WithoutSandbox } from "./bindings";
import { Choice, Field, SettingActions, SettingGroup, SettingRow } from "./settings/components";
import { AnswerBar } from "./AnswerBar";
import { profileCommand, profilePage } from "./settings/profileAddress";

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
 * no CLI word, file or chat makes one, and charter records who turned it off and why. Where an
 * administrator's policy forbids the opt-out (#1343), no box is drawn and the picker says who
 * locked it; a chat the sandbox cannot hold here then does not start.
 *
 * **Every control here is a row of the settings set** (`settings/components.tsx`, DS-3d #1176),
 * so the picker reads as Settings does. It was hand-rolled markup once, and the hand-rolling is
 * what broke it: five spans in one `<label>` ran together into
 * `claudeclaudeclaudebuilt-indefault`, and the accessible name of every row was that same run
 * of words. A harness's name is now its name, and the rest of its row describes it.
 *
 * **An arrow key picks the harness or persona it moves to**, as a radio does. Radix learns an
 * arrow is down too late for a single press here; `Choice` hears it first, and why is written
 * there. A pick here starts nothing — Start does — so it needs no button of its own.
 * "moves between harnesses with the arrow keys" goes red if `Choice` stops picking on arrows.
 *
 * **Picking a persona picks its own profile** (#1445). A persona's definition may name the
 * profile its chats start on, and its row says so; picking that persona moves the harness row
 * there, and picking one that names none moves it back to the default. **A harness the person
 * picked is never moved**: once they have touched the Harness row, or something asked for a
 * harness (`prefer`), picking a persona changes the persona and nothing else.
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
  fixing = false,
  onFix,
  onOpenSettings,
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
  /** Whether a fix pressed here is running: its button says so and cannot be pressed again. */
  fixing?: boolean;
  /** Applies a doctor fix by its id — the `local-ignore` the refusal offers (NO-8) — and reads
   *  the picker again. Absent, the refusal shows the fix's words alone. */
  onFix?: (id: string) => void;
  /** Opens Settings at a group (SE-22's address), with the setting to focus there: where a
   *  refused profile is mended (NO-8, #1296). */
  onOpenSettings?: (group: string, setting?: string) => void;
  onCancel: () => void;
}) {
  // Only a persona there is a row for. The core already filters `[persona] default`
  // against the personas the plane has, so this should be unreachable from the app — but
  // the alternative, if it ever arrives, is a chat started on a persona the operator can
  // neither see nor change, and refused for it a moment later.
  const [persona, setPersona] = useState<string | null>(
    options.persona !== null && options.personas.includes(options.persona) ? options.persona : null,
  );
  // A persona's own profile (#1445), where its definition names one the project offers and
  // the picker has a row for it.
  const ownProfile = (who: string | null): string | undefined => {
    const own = who === null ? undefined : options.persona_profiles[who];
    return options.profiles.some((p) => p.name === own) ? own : undefined;
  };
  // A harness something already asked for (`prefer`), where there is a row of that kind.
  const [preferred] = useState<string | undefined>(() => {
    const ofKind = options.profiles.filter((p) => prefer !== undefined && p.kind === prefer);
    return (ofKind.find((p) => p.is_default) ?? ofKind[0])?.name;
  });
  // The row the picker is on when no persona's own profile moves it: the default.
  const [resting] = useState<string | undefined>(
    () =>
      preferred ?? options.profiles.find((p) => p.is_default)?.name ?? options.profiles[0]?.name,
  );
  const [profile, setProfile] = useState<string | undefined>(
    // A harness something already asked for wins; then the picked persona's own profile.
    () => preferred ?? ownProfile(persona) ?? resting,
  );
  // Whether the harness row is somebody's choice already: the person picked one here, or
  // something asked for a harness. After that a persona never moves it.
  const [harnessChosen, setHarnessChosen] = useState(preferred !== undefined);
  const pickProfile = (name: string) => {
    setHarnessChosen(true);
    setProfile(name);
  };
  // Picking a persona moves the harness row to that persona's own profile, and to the row it
  // rests on for a persona that names none, so no harness is left over from a persona that is
  // no longer picked. It picks a row and starts nothing. A harness the person chose stays.
  const pickPersona = (who: string | null) => {
    setPersona(who);
    if (!harnessChosen) setProfile(ownProfile(who) ?? resting);
  };
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
  const picked = options.profiles.find((p) => p.name === profile);
  // What the sandbox does for a chat on the picked profile, where the project turned it on.
  const sandbox = picked?.sandbox ?? null;
  // The opt-out box, for a chat the sandbox would hold. Off, and not remembered between chats.
  const [optedOut, setOptedOut] = useState(false);
  // Where the sandbox cannot be applied, the start IS the opt-out, and the button says so.
  const refused = sandbox?.state === "refused";
  const [reason, setReason] = useState("");
  // Policy may forbid the opt-out (#1343): then no box, and a chat the sandbox cannot hold
  // here does not start at all, saying who locked it.
  const locked = sandbox?.locked ?? null;
  const withoutSandbox: WithoutSandbox | null =
    locked === null && ((sandbox?.state === "sandboxed" && optedOut) || refused)
      ? { reason: reason.trim() === "" ? null : reason }
      : null;
  const blockedByPolicy = refused && locked !== null;
  const startWord = withoutSandbox !== null ? "Start without the sandbox" : "Start";
  // Cancel, so the dialog can put the keyboard on it itself. React's `autoFocus` and the
  // focus trap's own opening move both aim at mount, and which of them lands last is not
  // something to leave to ordering: the trap is told to do nothing and this is focused here.
  const cancel = useRef<HTMLButtonElement>(null);
  // What had the keyboard when the dialog came up — the `+` it was opened from, or the terminal
  // a shortcut was typed in — read once, on the first draw (#1600). The window closes the dialog
  // by no longer drawing it, and the focus trap's own way back lands on the page instead.
  const [opener] = useState(() =>
    document.activeElement instanceof HTMLElement && document.activeElement !== document.body
      ? document.activeElement
      : null,
  );
  // Whether it was left with nothing started — Escape or Cancel. Only then does the keyboard go
  // back to the opener: a start hands it to the new chat, and Settings or the install shell take
  // it where they open.
  const cancelled = useRef(false);
  const leave = () => {
    cancelled.current = true;
    onCancel();
  };

  return (
    // Escape starts nothing, and it is the dialog's own Escape rather than a listener on the
    // window: focus is trapped inside, so "wherever focus happens to be inside it" is now a
    // property of the surface rather than a thing this component has to arrange.
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) leave();
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
          // Left with nothing started, the keyboard goes back where it was, when that is still
          // on the page; otherwise the trap's default.
          onCloseAutoFocus={(e) => {
            if (!cancelled.current || opener === null || !opener.isConnected) return;
            e.preventDefault();
            opener.focus({ preventScroll: true });
          }}
        >
          <Dialog.Title id="start-chat">Start a chat</Dialog.Title>

          {options.ignore_fix && (
            <p className="honest mid-turn" role="alert">
              git would carry <code>charter.local.toml</code>, so every profile it declares is
              refused until that is fixed: <code>{options.ignore_fix}</code>
            </p>
          )}
          {/* The doctor's own fix for this state (NO-8, #1233), the one its `harness profiles`
              row offers, where one ignore line is the whole cure. */}
          {options.ignore_fix_id && onFix && (
            <SettingActions>
              <button
                type="button"
                tabIndex={0}
                disabled={fixing}
                onClick={() => options.ignore_fix_id && onFix(options.ignore_fix_id)}
              >
                {fixing ? "Adding the ignore line…" : "Add the ignore line"}
              </button>
            </SettingActions>
          )}
          {options.declares_none && !options.ignore_fix && (
            <p className="honest">
              {/* Said rather than shown as an empty list: a project that declares nothing is the
                ordinary first state, not a fault, and the built-ins below still start. */}
              This project declares no profiles of its own, so these are purlis&apos;s built-ins.
              Declare your own in <code>charter.local.toml</code>, which stays on this machine.
            </p>
          )}

          {/* Every control is a row of the settings set (DS-3d, #1176), as Settings draws one:
              a name, the control, and the line that says what it means. */}
          <SettingRow
            label="Harness"
            grouped
            control={(ids) => (
              <Choice
                ids={ids}
                kind="radio"
                options={options.profiles.map((row) => ({
                  value: row.name,
                  label: row.name,
                  says: <ProfileMeta row={row} />,
                }))}
                value={profile}
                onValueChange={pickProfile}
              />
            )}
          />
          {/* The picked harness's card, at a glance (HP-19): what it runs at and lacks, before
              anything starts. Its whole card is a tab, opened from the chat's header. */}
          {picked?.harness && <HarnessSummary glance={picked.harness} />}

          <SettingRow
            label="Persona"
            grouped
            control={(ids) => (
              <Choice
                ids={ids}
                kind="radio"
                options={[
                  { value: NO_PERSONA, label: "None" },
                  ...options.personas.map((who) => {
                    const own = ownProfile(who);
                    const says = [
                      // What the persona says of itself in a line (#1460).
                      options.persona_descriptions?.[who],
                      who === options.persona ? "project default" : undefined,
                      own === undefined ? undefined : `its profile is ${own}`,
                    ].filter((word) => word !== undefined);
                    return {
                      value: who,
                      label: who,
                      mark: <PersonaMark persona={who} />,
                      says: says.length === 0 ? undefined : says.join(" · "),
                    };
                  }),
                ]}
                value={persona ?? NO_PERSONA}
                onValueChange={(value) => pickPersona(value === NO_PERSONA ? null : value)}
              />
            )}
          />

          {/* The reason for the default and the reason against it both belong on screen: the
              panels repeat most of what the footer says, and the footer says it about THIS
              chat's own workspace. */}
          <SettingRow
            label="Draw purlis's footer in this chat"
            help={
              "Blank by default, because the panels already draw the project. The footer says " +
              "which workspace this chat is on, which the panels say only for the focused one. " +
              "This chat only, and only from its next start."
            }
            control={(ids) => (
              <Choice
                ids={ids}
                kind="toggle"
                checked={showFooter}
                onCheckedChange={setShowFooter}
              />
            )}
          />

          {repo !== undefined && (
            <SettingRow
              label={`Start on a new branch in ${repo}`}
              help={
                `Its own branch and folder, cut from what ${repo} has checked out, so this ` +
                "chat's changes stay apart from other chats'. Named after the chat, or chat-1, " +
                `chat-2 and on. Cleared, it works on the branch ${repo} has checked out, shared ` +
                "with every chat there."
              }
              control={(ids) => (
                <Choice
                  ids={ids}
                  kind="toggle"
                  checked={onABranch}
                  onCheckedChange={setOnABranch}
                />
              )}
            />
          )}

          <SettingRow
            label="Name"
            help={
              "Optional. Left empty, the chat is named after its persona, or its harness, and " +
              "its number. You can rename it from its tab later."
            }
            control={(ids) => <Field ids={ids} kind="text" value={name} onChange={setName} />}
          />

          {/* One heading over every state of it, so the sentence about a sandbox that is off
              or cannot be applied is not a stray line under the Name box. */}
          {sandbox !== null && (
            <SettingGroup label="Sandbox">
              {sandbox.state === "sandboxed" && locked === null && (
                <SettingRow
                  label="Start without the sandbox"
                  help={
                    "This project runs every chat sandboxed: it reaches only the hosts the " +
                    "project allows, and never your vaults. Ticked, this one chat runs without " +
                    "it, its tab says so, and purlis records that you turned it off. Nothing " +
                    "inherits it: a relaunch or a resume asks the sandbox again."
                  }
                  control={(ids) => (
                    <Choice
                      ids={ids}
                      kind="toggle"
                      checked={optedOut}
                      onCheckedChange={setOptedOut}
                    />
                  )}
                />
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
                      Install it with <code>{sandbox.install}</code>. It needs sudo, so purlis types
                      it in a shell tab and leaves running it to you.{" "}
                      {onInstall && (
                        <button type="button" tabIndex={0} onClick={onInstall}>
                          Type it in a shell tab
                        </button>
                      )}
                    </p>
                  )}
                </>
              )}
              {locked !== null && <p className="honest">{locked}</p>}
              {withoutSandbox !== null && (
                <SettingRow
                  label="Why, if you want it recorded"
                  help="Optional. Kept on this machine with the record that this chat ran without the sandbox."
                  control={(ids) => (
                    <Field ids={ids} kind="text" value={reason} onChange={setReason} />
                  )}
                />
              )}
            </SettingGroup>
          )}

          {options.refused.length > 0 && (
            <details className="refused">
              {/* A missing profile is a row that is not in the list — easy to miss in a way a
                missing panel is not — so the ones purlis will not use say why. */}
              <summary>{options.refused.length} refused</summary>
              <ul>
                {options.refused.map(([name, why]) => (
                  <li key={name}>
                    <span>{name}</span> {why}
                    {/* Where the profile is mended (NO-8, #1296): its own page, its command
                      focused. One with no page (a committed profile, a whole file) lands on
                      Harness & profiles, the group its page would be under. */}
                    {onOpenSettings && (
                      <>
                        {" "}
                        <button
                          type="button"
                          tabIndex={0}
                          onClick={() => onOpenSettings(profilePage(name), profileCommand(name))}
                        >
                          Open {name} in Settings
                        </button>
                      </>
                    )}
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
          <AnswerBar>
            {/* Cancel first and focused: see `onOpenAutoFocus` above. */}
            <button ref={cancel} tabIndex={0} onClick={leave}>
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
                disabled={!picked || starting || blockedByPolicy}
              >
                {starting ? "Starting…" : `Approve and ${startWord.toLowerCase()}`}
              </button>
            ) : (
              <button
                tabIndex={0}
                onClick={() =>
                  profile && onStart(profile, persona, showFooter, label, newBranch, withoutSandbox)
                }
                disabled={!profile || starting || blockedByPolicy}
              >
                {starting ? "Starting…" : startWord}
              </button>
            )}
          </AnswerBar>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
