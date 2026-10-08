import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ScrollText } from "lucide-react";
import { commands, type BriefOf, type PlaneId, type TaskBrief } from "./bindings";

/**
 * **The brief a task was sent, read back** (#1494, V100-45).
 *
 * A task starts on a brief: what its asking chat wrote, or what the person wrote from a chat's
 * tab. **Brief** shows it as it was sent, with who sent it, when, to which persona and where
 * it works. It is on a task's row menu in the Chats list, on a finished task's row, and on the
 * breadcrumb line while a tab shows a task. All three open this one panel.
 *
 * **Read-only, and a dialog, not a view tab.** It asks nothing, so Escape, Close and a press
 * outside it all close it, and the keyboard goes back to where it was.
 *
 * **The brief is a chat's words, never purlis's, and never markup here.** It is one text node
 * in a `<pre>`: no link in it is live, no image is loaded, no tag is read. Where it holds a
 * character that draws as nothing or turns the words around it, the core hands it over written
 * out as well (`inert`), that is what is drawn, and the panel says so. **Copy gives the brief
 * as it was sent**, byte for byte, whatever was drawn.
 *
 * **Long briefs are whole**: the box scrolls, and nothing is cut by the panel. A brief the
 * record itself cut, and a record that holds none, are said in plain words.
 */

/** Which task's brief: an open task by its chat's number, or a dispatch by its record's id
 *  (a finished task's row, a line of the Activity view). `name` is the task's, for the
 *  panel's title before the core has answered. */
export type BriefAsk = { name: string } & ({ chat: number } | { dispatch: string });

/**
 * **Opens the Brief panel for a task**: the one way in, for the catalogue's `Brief` rows, a
 * finished row, the breadcrumb's button, and whatever else names a task (a tab chip's menu,
 * the Activity view's "dispatched" line). Nothing is read until the panel opens.
 */
export type OpenBrief = (of: BriefAsk) => void;

const Opener = createContext<OpenBrief | undefined>(undefined);

/**
 * Hands everything a project's window draws the way to open a task's brief. It also notes what
 * each context menu is opened on, for the keyboard's way back ({@link useKeyboardBack}).
 */
export function BriefOpener({ value, children }: { value: OpenBrief; children: ReactNode }) {
  useEffect(() => {
    const opened = (event: MouseEvent) => {
      menuOpenedOn = event.target instanceof Element ? event.target : null;
    };
    document.addEventListener("contextmenu", opened, true);
    return () => document.removeEventListener("contextmenu", opened, true);
  }, []);
  return <Opener.Provider value={value}>{children}</Opener.Provider>;
}

/** {@link OpenBrief}, inside a project's window. Outside one there is none, and whatever
 *  would offer Brief offers nothing. */
export function useOpenBrief(): OpenBrief | undefined {
  return useContext(Opener);
}

/** The panel's accessible name, and the name of every control that opens it. */
export function briefTitle(name: string): string {
  return `Brief of ${name}`;
}

/**
 * **Brief, on the breadcrumb line** (`.pane-chips`, beside `PaneCrumbs`): one small icon
 * button, named `Brief of <task>`.
 *
 * **It is an item of the line, after the breadcrumb, and never part of it.** The line's rule
 * is that the breadcrumb's state word is cut last (`App.css`, `.pane-crumbs`): every other
 * item of the line gives way first, and this is one of those. So in a pane too narrow for the
 * whole line it is cut at its edge with the gauge, and the path and its state are what they
 * were without it. The same Brief is on the task's row in the Chats list.
 */
export function BriefButton({ of }: { of: BriefAsk }) {
  const open = useOpenBrief();
  if (open === undefined) return null;
  return (
    <button
      type="button"
      className="pane-brief"
      // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
      tabIndex={0}
      aria-label={briefTitle(of.name)}
      aria-haspopup="dialog"
      title="Brief: what this task was sent"
      onClick={() => open(of)}
    >
      <ScrollText aria-hidden="true" />
    </button>
  );
}

/** When it was sent, in the person's own time and words; the record's stamp where that does
 *  not read as a time. */
function whenSaid(sent: string): string {
  const at = new Date(sent);
  return Number.isNaN(at.getTime()) ? sent : at.toLocaleString();
}

/** The brief's facts, in purlis's words. Every name is isolated (`<bdi>`), so a name in
 *  another direction's script turns nothing around it. */
function Facts({ said }: { said: TaskBrief }) {
  return (
    <p className="where brief-facts">
      Sent by{" "}
      {said.by_person ? (
        <>
          you, from the tab of <bdi>{said.asker}</bdi>
        </>
      ) : (
        <bdi>{said.asker}</bdi>
      )}
      {said.persona !== null && (
        <>
          , to <bdi>{said.persona}</bdi>
        </>
      )}
      , <time dateTime={said.sent}>{whenSaid(said.sent)}</time>. It works in <bdi>{said.place}</bdi>
      {said.folder !== null && said.folder !== "." && (
        <>
          {" "}
          (<bdi>{said.folder}</bdi>)
        </>
      )}
      {said.branch !== null && (
        <>
          , on its own branch <bdi>{said.branch}</bdi>
        </>
      )}
      .
    </p>
  );
}

/**
 * The element the last context menu was opened on: a chat's row, a tab. Kept so a panel opened
 * from a row of that menu can hand the keyboard back to it ({@link useKeyboardBack}).
 */
let menuOpenedOn: Element | null = null;

/**
 * **Where the keyboard was as the panel opened, handed back as it closes** (`useFocusBack`'s
 * rule, and one case more).
 *
 * The panel has no trigger of its own, so the element that had the focus as it opened gets it
 * back: the breadcrumb's button, a finished row's Brief. **Opened from a row of a menu, that
 * element is the menu's row, which goes with its menu.** The keyboard then goes to what the
 * menu was opened on (the task's row in the Chats list), which is where it was before the menu.
 */
function useKeyboardBack() {
  const [had] = useState(() => {
    const at = document.activeElement;
    if (!(at instanceof HTMLElement) || at.closest('[role="menu"]') === null) return at;
    return menuOpenedOn?.closest<HTMLElement>("button, [tabindex]") ?? null;
  });
  return (event: Event) => {
    event.preventDefault();
    if (had instanceof HTMLElement && had.isConnected) had.focus();
  };
}

/** What Copy last did, said to a screen reader too. */
type Copied = "copied" | "refused" | undefined;

/**
 * The panel: the brief of the task `of` names, read from the core as it opens.
 *
 * Mounted while it is open and unmounted when it closes, so each opening reads again and the
 * keyboard goes back to where it was as this one opened ({@link useKeyboardBack}).
 */
export function BriefPanel({
  plane,
  of,
  onClose,
}: {
  plane: PlaneId;
  of: BriefAsk;
  onClose: () => void;
}) {
  const [said, setSaid] = useState<TaskBrief>();
  const [trouble, setTrouble] = useState<string>();
  const [copied, setCopied] = useState<Copied>();
  const close = useRef<HTMLButtonElement>(null);
  const handBack = useKeyboardBack();
  // What is asked, as values: one read an opening, whatever object named the task.
  const chat = "chat" in of ? of.chat : undefined;
  const dispatch = "dispatch" in of ? of.dispatch : undefined;
  useEffect(() => {
    let gone = false;
    const ask: BriefOf = chat !== undefined ? { chat } : { dispatch: dispatch ?? "" };
    void commands
      .taskBrief(plane, ask)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }))
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setTrouble(answer.error);
        else setSaid(answer.data);
      });
    return () => {
      gone = true;
    };
  }, [plane, chat, dispatch]);

  const copy = () => {
    if (said === undefined) return;
    const put = navigator.clipboard?.writeText(said.brief);
    if (put === undefined) {
      setCopied("refused");
      return;
    }
    void put.then(
      () => setCopied("copied"),
      () => setCopied("refused"),
    );
  };

  // The task's own name once the record has said it, the row's until then.
  const title = briefTitle(said?.name ?? of.name);
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          className="warning brief-panel"
          aria-describedby={undefined}
          onOpenAutoFocus={(event) => {
            // Close has the keyboard from the start, whether or not the brief has been read
            // yet: Escape and Enter both close, and Shift+Tab is Copy and the brief's box.
            event.preventDefault();
            close.current?.focus();
          }}
          // The focus goes back to where it was as the panel opened.
          onCloseAutoFocus={handBack}
        >
          <Dialog.Title>
            Brief of <bdi>{said?.name ?? of.name}</bdi>
          </Dialog.Title>
          {trouble !== undefined ? (
            <p className="honest" role="alert">
              {trouble}
            </p>
          ) : said === undefined ? (
            <p className="pending">reading the brief…</p>
          ) : (
            <>
              <Facts said={said} />
              {said.kept === "missing" ? (
                <p className="honest">
                  This task's dispatch record holds no brief, so there is nothing to show.
                </p>
              ) : (
                <>
                  {said.kept === "cut" && (
                    <p className="honest">
                      The brief was longer than a dispatch record keeps. This is its start, as it
                      was sent. The rest is not kept.
                    </p>
                  )}
                  {said.inert !== null && (
                    <p className="honest">
                      This brief holds characters that draw as nothing or change how the text around
                      them reads. Each is written out here as its code, such as{" "}
                      <code>{"\\u202e"}</code>, and a backslash the brief wrote is doubled. Copy
                      gives the brief exactly as it was sent.
                    </p>
                  )}
                  <p className="brief-whose">
                    {said.by_person
                      ? "The brief, as you wrote it."
                      : "The brief, as the chat wrote it. purlis did not write it."}
                  </p>
                  {/* One text node: a brief is a chat's words, and is never markup here. Its
                      box scrolls, so it is a stop of its own for the keyboard. */}
                  <pre
                    className="brief-text"
                    tabIndex={0}
                    role="group"
                    aria-label={`${title}, as it was sent`}
                    dir="auto"
                  >
                    {said.inert ?? said.brief}
                  </pre>
                </>
              )}
            </>
          )}
          <div className="answer">
            <span className="brief-copied" role="status">
              {copied === "copied"
                ? "Copied, as it was sent."
                : copied === "refused"
                  ? "purlis could not put the brief on the clipboard."
                  : ""}
            </span>
            {said !== undefined && said.kept !== "missing" && (
              <button type="button" tabIndex={0} onClick={copy}>
                Copy
              </button>
            )}
            <Dialog.Close asChild>
              <button type="button" tabIndex={0} ref={close}>
                Close
              </button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
