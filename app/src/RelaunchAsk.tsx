import { useRef } from "react";
import * as Alert from "@radix-ui/react-alert-dialog";
import type { RelaunchChoice, RelaunchQuestion } from "./bindings";

/**
 * What a launch asks before it puts anything back (charter-app#250): reopen every session, or
 * start fresh.
 *
 * The operator's words: *"on reopen - we need to suggest re-open all sessions - or start fresh
 * session, and same will work for update flow."* It is asked once, only when the last quit left
 * something open, and **nothing starts while it is up** — the core holds the launch's own
 * project back until `relaunch` has the answer, and the window restores the other projects
 * after it (`App.tsx`).
 *
 * **Every way out that is not the "Start fresh" button is "Reopen all"**: the first button,
 * which has the keyboard, and Escape, which is the primitive's `Cancel`. Starting fresh clears
 * the record, and an answer lost to a stray key must never be the one that throws work away.
 *
 * **Radix's `AlertDialog`**, for `EndingChat`'s reasons: it arrives without being asked for, a
 * click outside answers nothing, and the primitive requires the `Cancel` the focus goes to.
 * Both answers carry `tabIndex={0}` (`docs/ui-primitives.md`, charter-app#186).
 *
 * `question.after_update` is charter-app#251's: Restart to update writes it into each record,
 * and the launch it restarts into says why it is being asked. "Reopen all" is in front either
 * way.
 *
 * Each project row carries its whole path as its title, for `calledOn`'s reason: two projects
 * can share a folder name.
 */
export function RelaunchAsk({
  question,
  nameOf,
  onAnswer,
}: {
  question: RelaunchQuestion;
  /** What a project is called on its tab, so the question names it the same way. */
  nameOf: (plane: string) => string;
  onAnswer: (choice: RelaunchChoice) => void;
}) {
  const reopen = useRef<HTMLButtonElement>(null);
  const chats = question.projects.reduce((sum, project) => sum + project.chats, 0);
  const views = question.projects.reduce((sum, project) => sum + project.views, 0);
  const open = counts(chats, views, " and ");
  return (
    <Alert.Root
      open
      onOpenChange={(isOpen) => {
        if (!isOpen) onAnswer("ReopenAll");
      }}
    >
      <Alert.Portal>
        <Alert.Overlay className="asking" />
        <Alert.Content
          className="warning"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            reopen.current?.focus();
          }}
        >
          <Alert.Title>Reopen your sessions?</Alert.Title>
          <Alert.Description className="honest">
            {question.after_update && "charter restarted to install an update. "}
            {`${open} ${chats + views === 1 ? "was" : "were"} open when charter last quit.`}
          </Alert.Description>
          <ul className="ending">
            {question.projects.map((project) => (
              <li key={project.plane} title={project.plane}>
                <span className="who">{nameOf(project.plane)}</span>
                <span className="what">{counts(project.chats, project.views, ", ")}</span>
              </li>
            ))}
          </ul>
          <div className="answer">
            <Alert.Cancel asChild>
              <button ref={reopen} tabIndex={0}>
                Reopen all sessions
              </button>
            </Alert.Cancel>
            {/* Not the primitive's `Action`: an `Action` also closes the dialog, and closing
                is this dialog's "Reopen all" — one press would send both answers. */}
            <button tabIndex={0} onClick={() => onAnswer("StartFresh")}>
              Start fresh
            </button>
          </div>
        </Alert.Content>
      </Alert.Portal>
    </Alert.Root>
  );
}

/** `3 chats and 1 view tab`, leaving out a kind there are none of. */
function counts(chats: number, views: number, between: string): string {
  return [counted(chats, "chat"), counted(views, "view tab")].filter(Boolean).join(between);
}

/** `3 chats`, `1 view tab`, or nothing at all for none. */
function counted(howMany: number, what: string): string {
  if (howMany === 0) return "";
  return `${howMany} ${what}${howMany === 1 ? "" : "s"}`;
}
