import { existsSync, rmSync, writeFileSync } from "node:fs";
import { THE_RUNS_DIALOG_ANSWER } from "./harness.js";

/**
 * **What the next native dialog answers** (#1680): the seam a scenario spec uses in place of a
 * dialog no WebDriver reaches.
 *
 * A folder picker is the operating system's window. A spec that clicks Locate… would wait on it
 * for good, and the page cannot stand in for it: the IPC goes neither through the page's
 * `fetch` nor through an `invoke` a spec can replace. So the app built with the `e2e` feature
 * reads the answer from a file instead (`scenario_answer` in `src-tauri/src/opener.rs`), once,
 * and removes it; the dialog after that is the system's again. Every other build reads nothing.
 *
 * The file is `THE_RUNS_DIALOG_ANSWER`, which every launcher names in the app's environment
 * (`theRunsEnvironment`), so any spec in any config can answer a dialog the same way.
 */

/** A kind of native dialog, as the app names it. A file pick joins this list when one exists. */
export type Dialog = "folder";

/** Has the next `dialog` the app opens answer `picked`, or be cancelled when it is `null`. */
export function theNextDialogAnswers(dialog: Dialog, picked: string | null): void {
  writeFileSync(THE_RUNS_DIALOG_ANSWER, JSON.stringify({ dialog, picked }));
}

/** Has the next folder pick answer `folder`, as a person picking it would. */
export function theNextFolderPickAnswers(folder: string | null): void {
  theNextDialogAnswers("folder", folder);
}

/** Whether an answer is still waiting: the app has not opened the dialog it was meant for. */
export function anAnswerIsWaiting(): boolean {
  return existsSync(THE_RUNS_DIALOG_ANSWER);
}

/** Takes back an answer nothing used, so it cannot answer another spec's dialog. */
export function noAnswerWaits(): void {
  rmSync(THE_RUNS_DIALOG_ANSWER, { force: true });
}
