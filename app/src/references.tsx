/**
 * **Files into chats** (FM-9, #1103 F7): a file, a folder, a range of lines or a search hit,
 * handed to a chat as a reference in its harness's own syntax, typed and never sent.
 *
 * The window only ever names WHAT to reference — a branch, a path inside it, and the lines —
 * never the text. The core places the path (inside the branch, no link, not `.git`), builds the
 * reference with the chat's harness adapter, and types it as one bracketed paste with nothing
 * after it, only while the chat is at its prompt (`references.rs`). Where it cannot be typed —
 * opencode, a chat mid-turn — the core puts it on the clipboard and says why.
 *
 * Three ways in, all through here:
 * - **a drag** (`dragReference`) from a file row, a search hit or the preview's selection, onto
 *   a chat's tab or its pane (`droppedReference`);
 * - **"Ask a chat about this"** and **"Add to a chat's context"** in the preview, which pick a
 *   chat (`PickAChat`);
 * - **"Start a chat here"** on a file or folder row (`actions.ts`, `startChatHere`).
 */
import { createContext, useContext, useState, type DragEvent } from "react";
import { MessageSquarePlus } from "lucide-react";
import { commands, type Handed, type PlaneId } from "./bindings";
import type { Ran } from "./actions";

/** The drag's own type: only charter's rows carry it, so a file dragged in from the desktop or
 *  text from another app is never taken for a reference. */
export const REFERENCE_TYPE = "application/x-charter-reference";

/** What is referenced: a branch by its names, a path inside it, and the lines of a file. */
export type Referenced = {
  /** The project the branch is in: a hit from another project cannot be handed to this one's
   *  chats, whose branches are named in this project. */
  plane: PlaneId;
  workspace: string;
  repo: string;
  piece: string | null;
  path: string;
  folder: boolean;
  lines?: { first: number; last: number };
};

/** How a reference reads in a sentence and a drag's text: `src/main.rs:10-20`. */
export function referenceSaid(r: Referenced): string {
  if (r.lines === undefined) return r.folder ? `${r.path}/` : r.path;
  const { first, last } = r.lines;
  return first === last ? `${r.path}:${first}` : `${r.path}:${first}-${last}`;
}

/** Starts a drag carrying `r`. A copy, never a move: nothing in the branch changes. */
export function dragReference(event: DragEvent, r: Referenced): void {
  event.dataTransfer.setData(REFERENCE_TYPE, JSON.stringify(r));
  event.dataTransfer.setData("text/plain", referenceSaid(r));
  event.dataTransfer.effectAllowed = "copy";
}

/** Whether a drag over a chat carries a reference: what makes the chat a drop target. */
export function carriesReference(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes(REFERENCE_TYPE);
}

/** A drag over a chat: taken as a copy when it carries a reference, and left alone otherwise. */
export function overChat(event: DragEvent): void {
  if (!carriesReference(event)) return;
  event.preventDefault();
  event.dataTransfer.dropEffect = "copy";
}

/** The reference a drop carries, or none for anything else — read whole, and refused unless it
 *  has every field a reference has. */
export function droppedReference(event: DragEvent): Referenced | undefined {
  const raw = event.dataTransfer?.getData(REFERENCE_TYPE);
  if (!raw) return undefined;
  try {
    const r = JSON.parse(raw) as Partial<Referenced>;
    const lines = r.lines;
    const linesOk =
      lines === undefined ||
      (typeof lines === "object" &&
        lines !== null &&
        Number.isInteger(lines.first) &&
        Number.isInteger(lines.last));
    if (
      typeof r.plane !== "string" ||
      typeof r.workspace !== "string" ||
      typeof r.repo !== "string" ||
      (r.piece !== null && typeof r.piece !== "string") ||
      typeof r.path !== "string" ||
      typeof r.folder !== "boolean" ||
      !linesOk
    )
      return undefined;
    return r as Referenced;
  } catch {
    return undefined;
  }
}

/** What the window says once a reference reached a chat, or did not. */
export function handedSaid(handed: Handed, chat: string): string {
  if (handed.kind === "typed") return `Typed ${handed.text} into ${chat}. Nothing was sent.`;
  const why = handed.why.charAt(0).toUpperCase() + handed.why.slice(1);
  return `${why}: ${handed.text}`;
}

/** Hands `r` to chat `session` of project `plane`, named `chat` in what is said. */
export async function handReference(
  plane: PlaneId,
  session: number,
  chat: string,
  r: Referenced,
): Promise<Ran> {
  if (r.plane !== plane)
    return {
      ok: false,
      refused: `${r.path} is in another project, so it was not handed to ${chat}.`,
    };
  const answer = await commands
    .referenceIntoChat(plane, r.workspace, r.repo, r.piece, r.path, r.lines ?? null, session)
    .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
  if (answer.status === "error") return { ok: false, refused: answer.error };
  return { ok: true, said: handedSaid(answer.data, chat) };
}

/** One chat a reference can be handed to, as the picker lists it. */
export type ChatHere = { session: number; name: string };

/** What the project's window lends the views it holds, to hand a reference to a chat. */
export type ChatsForReferences = {
  plane: PlaneId;
  /** Every chat open in the project, in strip order. */
  chats: readonly ChatHere[];
  /** Hand `r` to chat `session`; `ask` also brings the chat to the front. */
  hand: (r: Referenced, session: number, how: "ask" | "add") => void;
};

/** Lent by `PlaneView`; none in a test or a window with no chats to lend. */
export const ReferenceChats = createContext<ChatsForReferences | undefined>(undefined);

/** The chats a view can hand a reference to, where its window lends them. */
export function useReferenceChats(): ChatsForReferences | undefined {
  return useContext(ReferenceChats);
}

/**
 * **"Ask a chat about this" or "Add to a chat's context"**: a button that opens a short list of
 * the project's chats, and hands `referenced` to the one picked. Asking brings the chat to the
 * front, so the operator types their question beside the reference; adding leaves them here.
 */
export function PickAChat({ referenced, how }: { referenced: Referenced; how: "ask" | "add" }) {
  const lent = useReferenceChats();
  const [open, setOpen] = useState(false);
  const label = how === "ask" ? "Ask a chat about this" : "Add to a chat's context";
  if (lent === undefined) return null;
  const chats = lent.chats;
  return (
    <span className="pick-a-chat">
      <button
        type="button"
        tabIndex={0}
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((was) => !was)}
      >
        <MessageSquarePlus className="node-icon" aria-hidden="true" />
        {label}
      </button>
      {open &&
        (chats.length === 0 ? (
          <span className="pick-a-chat-none" role="status">
            No chat is open in this project. Start one from the file's row with Start a chat here.
          </span>
        ) : (
          <span className="pick-a-chat-list" role="menu" aria-label={label}>
            {chats.map((chat) => (
              <button
                key={chat.session}
                type="button"
                role="menuitem"
                tabIndex={0}
                onClick={() => {
                  setOpen(false);
                  lent.hand(referenced, chat.session, how);
                }}
              >
                {chat.name}
              </button>
            ))}
          </span>
        ))}
    </span>
  );
}

/**
 * The preview's own handle on what it shows: dragged onto a chat, it carries the file — or the
 * lines selected in it — as a reference.
 */
export function DragHandle({ referenced }: { referenced: Referenced }) {
  return (
    <span
      className="reference-handle"
      draggable
      role="note"
      title="Drag onto a chat to type a reference to this into it"
      data-testid="reference-handle"
      onDragStart={(event) => dragReference(event, referenced)}
    >
      {referenceSaid(referenced)}
    </span>
  );
}
