import { useRef } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ChatState } from "./NeedsYou";
import { type State } from "./chatState";

/**
 * One chat a quit is about to end, whichever project it is in.
 *
 * **Its state travels with it, already looked up.** A session number names a chat only inside
 * its own project — every project numbers its chats from one — so a dialog handed one
 * `ChatStates` and a flat list of sessions would paint project A's `running` onto project B's
 * chat 1 and tell the operator that a chat nobody is running is mid-turn. The pair is the
 * identity, and resolving it here would mean this component holding a map per project for no
 * reason: the window already has both halves.
 */
export type Ending = {
  /** Unique across projects, because a session number is not. */
  key: string;
  /** Which project it is in, drawn when the window holds more than one. */
  project?: string;
  name: string;
  harness: string | null;
  cwd: string | null;
  state: State;
};

/**
 * What quitting asks before it ends anything.
 *
 * It lists the sessions that are about to end and what each one is doing. Until M1.3 it said
 * charter could not tell whether a session was mid-turn; now a harness's own hooks say so
 * (spec decision 3), and the ones that still cannot are named rather than lumped in with the
 * rest. Nothing here is guessed from a session's output, which is the one thing the app never
 * does (ADR 0018).
 *
 * **Every project the window holds, not the one in front.** Quit ends the process, and the
 * process holds them all — a warning that counted only what was on screen would be a warning
 * that understated what it was about to end by however many projects the operator had merged
 * into the window.
 *
 * A Radix dialog (`docs/ui-primitives.md`), which is what makes "over everything" true rather
 * than drawn: the rest of the window is inert and out of the accessibility tree while it is up,
 * and the keyboard cannot leave it for a pane behind it.
 *
 * **Escape answers it now, and did not before.** It answers what Cancel answers — nothing is
 * ended, and the core is told, so the next quit warns again rather than going straight out. A
 * click outside answers nothing at all.
 */
export function QuitWarning({
  chats,
  onQuit,
  onCancel,
}: {
  chats: readonly Ending[];
  onQuit: () => void;
  onCancel: () => void;
}) {
  const running = chats.filter((chat) => chat.state === "running");
  const unknown = chats.filter((chat) => chat.state === "unknown");
  // Only when there is more than one: naming the project on every row of a window holding one
  // is a column that says the same thing all the way down.
  const several = new Set(chats.map((chat) => chat.project ?? "")).size > 1;
  // Cancel, focused by the dialog itself rather than by `autoFocus`: see `StartChat`.
  const cancel = useRef<HTMLButtonElement>(null);
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          className="warning"
          aria-labelledby="quit-warning"
          // A click outside answers nothing. Cancel and Escape are the two ways out.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            cancel.current?.focus();
          }}
        >
          <Dialog.Title id="quit-warning">
            {chats.length === 1
              ? "1 session will be ended"
              : `${chats.length} sessions will be ended`}
          </Dialog.Title>
          <ul className="ending">
            {chats.map((chat) => (
              <li key={chat.key}>
                <span className="what">{chat.harness ?? "shell"}</span>
                <span className="who">{chat.name}</span>
                <ChatState state={chat.state} />
                {several && chat.project && <code className="where">{chat.project}</code>}
                {chat.cwd && <code className="where">{chat.cwd}</code>}
              </li>
            ))}
          </ul>
          {running.length > 0 && (
            <p className="honest mid-turn" role="alert">
              {running.length === 1
                ? `${running[0].name} is mid-turn and will be interrupted.`
                : `${running.length} sessions are mid-turn and will be interrupted.`}
            </p>
          )}
          {unknown.length > 0 && (
            <p className="honest">
              {/* Named, not counted into the reassuring number. A harness that reports nothing
                could be mid-turn and charter would never know — saying "nothing is running"
                over the top of it would be the app claiming something it cannot see. */}
              {unknown.length === 1
                ? `${unknown[0].name} reports no state, so charter cannot tell whether it is mid-turn.`
                : `${unknown.length} sessions report no state, so charter cannot tell whether they are mid-turn.`}
            </p>
          )}
          {running.length === 0 && unknown.length === 0 && (
            <p className="honest">No session is mid-turn.</p>
          )}
          <div className="answer">
            {/* Cancel first, and focused: the destructive answer is never the one a stray
              Return key finds. */}
            <button ref={cancel} onClick={onCancel}>
              Cancel
            </button>
            <button className="ends-it" onClick={onQuit}>
              Quit charter
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
