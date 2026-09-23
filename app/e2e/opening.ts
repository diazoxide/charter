import { $, browser, expect } from "@wdio/globals";

/**
 * Opening a chat, as the operator does it since M1.2: press, pick, start.
 *
 * **No harness starts until somebody picks a profile** (ADR 0022), so every spec that wants
 * a session comes through here. That is also what keeps the rule from being quietly
 * removed: take the picker out of the path and every scenario spec fails at once.
 *
 * The plane these run in declares one profile, approved on its first use by the picker
 * scenario. Whichever spec gets there first pays the approval; the rest see `Start`. Asking
 * for whichever button is present keeps the specs independent of their order, which matters
 * because WebdriverIO's Tauri service keeps ONE app process for the whole run.
 */
export async function pickAndStart(): Promise<void> {
  const dialog = await $('[role="dialog"]');
  await dialog.waitForDisplayed({ timeout: 20_000 });
  const approve = await $("button=Approve and start");
  const start = (await approve.isExisting()) ? approve : await $("button=Start");
  await start.waitForClickable({ timeout: 20_000 });
  await start.click();
  await expect(dialog).not.toBeDisplayed();
}

/**
 * The picker's harness rows, asked for by what they MEAN rather than by what they are made of.
 *
 * **One definition, used by every spec that waits for the picker to be ready.** It was three
 * copies of `input[type="radio"][name="profile"]`, which said "these rows are native inputs" —
 * a fact about the markup and not about the app. The primitives migration made them
 * `button[role="radio"]` and every copy became a selector that could never match, each one
 * costing a spec its full 20-second timeout before it failed.
 *
 * `role` is what a screen reader reads and what the operator gets; it survives whatever draws
 * it. Scoped to the harness group by the heading that names it, because the persona rows are
 * radios too and "the first radio on the page" is a different kind of accident waiting.
 */
export const HARNESS_ROWS = '[aria-labelledby="pick-harness"] [role="radio"]';

/** Waits until the picker has drawn the harness it is offering. */
export async function harnessRowsDrawn(): Promise<void> {
  await $(HARNESS_ROWS).waitForExist({ timeout: 20_000 });
}

/**
 * Where the keyboard is, named the way somebody looking for that control would name it.
 *
 * **A DOM read and not a gesture, which is what makes it usable here at all.** A scenario
 * cannot press a button by keyboard — a synthesised `Enter` carries no activation
 * (charter-app#176) — but where the focus IS after a key is a property of the document, and
 * reading it costs nothing the harness cannot do.
 *
 * It names a control by its role and its accessible words, in the same vocabulary
 * `Modals.keyboard.test.tsx` uses, so the real engine's answer and jsdom's can be read side by
 * side. A radio row and a checkbox have their words in a `<label for>` rather than inside them,
 * which is why that is consulted before the element's own text: without it, half the picker
 * comes back as `checkbox ""`.
 */
export async function whereTheKeyboardIs(): Promise<string> {
  return browser.execute(() => {
    const on = document.activeElement;
    if (!on || on === document.body) return "(nothing)";
    const labelled = on.id ? document.querySelector(`label[for="${CSS.escape(on.id)}"]`) : null;
    const named = on.getAttribute("aria-label") ?? labelled?.textContent ?? on.textContent ?? "";
    const role = on.getAttribute("role") ?? on.tagName.toLowerCase();
    return `${role} ${JSON.stringify(named.trim().slice(0, 40))}`;
  });
}

/** Presses a button by the words on it, or by its accessible name. */
export async function pressOnly(name: string): Promise<void> {
  const labelled = await $(`button[aria-label="${name}"]`);
  if (await labelled.isExisting()) {
    await labelled.click();
    return;
  }
  const button = await $(`button=${name}`);
  await button.waitForClickable({ timeout: 20_000 });
  await button.click();
}

/** Presses a button that starts a chat, and answers the picker it opens. */
export async function pressAndStart(name: string): Promise<void> {
  await pressOnly(name);
  await pickAndStart();
}

/**
 * Ending a chat, as the operator does it since the pane controls landed: press, then answer.
 *
 * **Nothing ends a chat without asking** — the operator's *"closing session should ask
 * confirmation"*, and it is asked in one place (`PlaneView`'s `run`), so a tab's `×`, a
 * pane's `×` and the palette's rows all come through here. That is also what keeps the rule
 * from being quietly removed: take the question out and every spec below hangs waiting for a
 * dialog that never comes.
 *
 * It is `role="alertdialog"` rather than `dialog`: Radix's `AlertDialog`, which is the
 * primitive for a question the operator did not go looking for.
 */
export async function answerTheAsk(name: string): Promise<void> {
  const asking = await $('[role="alertdialog"]');
  await asking.waitForDisplayed({ timeout: 20_000 });
  const answer = await asking.$(`button=${name}`);
  await answer.waitForClickable({ timeout: 20_000 });
  await answer.click();
  await expect(asking).not.toBeDisplayed();
}

/** Ends the chat a control named `name` belongs to, and answers the question it asks. */
export async function endChat(name: string): Promise<void> {
  await pressOnly(name);
  await answerTheAsk(name);
}

/**
 * Ends every chat the strip is drawing, and the ones it is not.
 *
 * **The strip collapses rather than scrolling** (ADR 0039, as amended), so at fifty
 * chats it draws a handful and hides the rest. This terminates because closing a drawn tab
 * gives the strip room for a hidden one — the hidden tabs flow onto the strip as the drawn
 * ones go — and it is bounded so that a close which stops taking is a failure and not a hang.
 */
export async function endEveryChat(most = 200): Promise<void> {
  for (let pressed = 0; pressed < most; pressed++) {
    const closer = await $('button[aria-label^="End chat "]');
    if (!(await closer.isExisting())) return;
    const name = (await closer.getAttribute("aria-label")) ?? "";
    await closer.click();
    await answerTheAsk(name);
  }
  throw new Error(`${most} presses did not end every chat`);
}
