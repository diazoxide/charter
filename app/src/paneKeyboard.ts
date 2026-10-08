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
 * **The keyboard went somewhere by the person's own hand** while a chat was owed it: a press in
 * a field, a dialog that opened. That is where they are now, so nothing is owed any more. The
 * body is not somewhere: it is where the focus falls when the terminal that had it is taken
 * away, which is the very moment a switch is under way.
 */
function movedOn(event: FocusEvent) {
  if (event.target === document.body || event.target === document.documentElement) return;
  forgive();
}

function owe(key: string) {
  owed = key;
  document.addEventListener("focusin", movedOn);
}

function forgive() {
  owed = undefined;
  document.removeEventListener("focusin", movedOn);
}

/** Whether a dialog has the keyboard: a question the person is answering keeps it, whatever a
 *  pane behind it is drawn as. */
function inADialog(): boolean {
  const at = document.activeElement;
  return at instanceof Element && at.closest('[role="dialog"], [role="alertdialog"]') !== null;
}

/**
 * A pane says chat `session`'s terminal is drawn, and how to put the keyboard in it. Answers
 * what to call when that terminal is gone.
 */
export function paneDrawn(plane: string, session: number, take: () => void): () => void {
  const key = chatKey(plane, session);
  drawn.set(key, take);
  if (owed === key) {
    // The terminal's own focus is a focus like any other: it must not read as the person
    // having moved on, in the turn a pane is drawn twice.
    document.removeEventListener("focusin", movedOn);
    if (!inADialog()) take();
    // Let go of after this turn and not before: a pane drawn twice in one turn, as React draws
    // one in development, is owed the keyboard by its second terminal, the one that stays.
    queueMicrotask(() => {
      if (owed === key) forgive();
    });
  }
  return () => {
    if (drawn.get(key) === take) drawn.delete(key);
  };
}

/** Chat `session`'s terminal takes the keyboard: now when it is on screen, and when it is
 *  drawn otherwise. One chat is owed it at a time, and only until the person puts the keyboard
 *  somewhere themselves. */
export function giveKeyboardTo(plane: string, session: number): void {
  const key = chatKey(plane, session);
  const take = drawn.get(key);
  if (take === undefined) {
    owe(key);
    return;
  }
  forgive();
  if (!inADialog()) take();
}

/** Nothing is owed and nothing is drawn: what a test starts from. */
export function forgetKeyboard(): void {
  drawn.clear();
  forgive();
}
