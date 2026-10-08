/**
 * **Which chat's terminal takes the keyboard next** (#1486).
 *
 * A tab switched to another chat draws that chat's terminal in the same pane, and the person
 * who pressed for it types next: the keyboard has to be in that terminal without a click. The
 * pane cannot do it by itself. Its terminal is made after the switch is drawn, and a terminal
 * that took the keyboard every time it was drawn would take it from the strip each time a tab
 * came forward under the arrow keys.
 *
 * So the window says which chat is owed the keyboard ({@link giveKeyboardTo}) and the pane
 * says when its terminal is there ({@link paneDrawn}). Whichever comes second hands it over.
 * Nothing else is held: a chat that is never drawn is owed nothing once another is asked for.
 */

/** The terminals on screen, each by the chat it shows, as the way to put the keyboard in it. */
const drawn = new Map<string, () => void>();

/** The chat whose terminal takes the keyboard when it is drawn. */
let owed: string | undefined;

/** A session number names a chat only inside its project. */
const chatKey = (plane: string, session: number) => `${plane}\n${session}`;

/**
 * A pane says chat `session`'s terminal is drawn, and how to put the keyboard in it. Answers
 * what to call when that terminal is gone.
 */
export function paneDrawn(plane: string, session: number, take: () => void): () => void {
  const key = chatKey(plane, session);
  drawn.set(key, take);
  if (owed === key) {
    take();
    // Let go of after this turn and not before: a pane drawn twice in one turn, as React draws
    // one in development, is owed the keyboard by its second terminal, the one that stays.
    queueMicrotask(() => {
      if (owed === key) owed = undefined;
    });
  }
  return () => {
    if (drawn.get(key) === take) drawn.delete(key);
  };
}

/** Chat `session`'s terminal takes the keyboard: now when it is on screen, and when it is
 *  drawn otherwise. */
export function giveKeyboardTo(plane: string, session: number): void {
  const key = chatKey(plane, session);
  const take = drawn.get(key);
  if (take === undefined) {
    owed = key;
    return;
  }
  owed = undefined;
  take();
}

/** Nothing is owed and nothing is drawn: what a test starts from. */
export function forgetKeyboard(): void {
  drawn.clear();
  owed = undefined;
}
