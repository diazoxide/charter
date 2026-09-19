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
