import { $, expect } from "@wdio/globals";

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
