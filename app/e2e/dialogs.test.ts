import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import { anAnswerIsWaiting, noAnswerWaits, theNextFolderPickAnswers } from "./dialogs.js";
import {
  DIALOG_ANSWER_VARIABLE,
  THE_RUNS_DIALOG_ANSWER,
  THE_RUNS_TREE,
  theRunsEnvironment,
} from "./harness.js";

/**
 * The scenario's half of the dialog seam (#1680), checked here because the specs that use it
 * first run in CI: a spelling or a format the app does not read would leave a spec waiting on a
 * dialog nobody can close.
 */

const here = dirname(fileURLToPath(import.meta.url));
const opener = () => readFileSync(join(here, "..", "src-tauri", "src", "opener.rs"), "utf8");

afterEach(noAnswerWaits);

describe("the answer to the next native dialog", () => {
  it("is named by the variable the e2e build reads, in every launcher's environment", () => {
    expect(opener()).toContain(`const DIALOG_ANSWER: &str = "${DIALOG_ANSWER_VARIABLE}";`);
    expect(theRunsEnvironment(join(THE_RUNS_TREE, "plane"))[DIALOG_ANSWER_VARIABLE]).toBe(
      THE_RUNS_DIALOG_ANSWER,
    );
  });

  it("is written in the shape the app's own tests read", () => {
    theNextFolderPickAnswers("/where/it/went");
    const written = readFileSync(THE_RUNS_DIALOG_ANSWER, "utf8");
    expect(JSON.parse(written)).toEqual({ dialog: "folder", picked: "/where/it/went" });
    // The Rust test reads this very text, so the two halves cannot drift apart.
    expect(opener()).toContain(`r#"${written}"#`);
  });

  it("can be a cancel, and is taken back when nothing used it", () => {
    theNextFolderPickAnswers(null);
    expect(JSON.parse(readFileSync(THE_RUNS_DIALOG_ANSWER, "utf8"))).toEqual({
      dialog: "folder",
      picked: null,
    });
    noAnswerWaits();
    expect(anAnswerIsWaiting()).toBe(false);
  });
});
