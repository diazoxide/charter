// The bench's cleanup of a throwaway profile (#954). Run with `node --test tools/`.
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { removeProfile } from "./cleanup.mjs";

test("a profile something is still writing into is removed once the writes stop", async () => {
  const home = mkdtempSync(join(tmpdir(), "charter-cleanup-"));
  mkdirSync(join(home, "cache"));
  let n = 0;
  // A late writer, the way a WebKit helper outliving the app keeps creating files.
  const writer = setInterval(() => {
    try {
      mkdirSync(join(home, "cache"), { recursive: true });
      writeFileSync(join(home, "cache", `late-${n++}`), "x");
    } catch {
      // The directory went away under it; that is the point.
    }
  }, 1);
  setTimeout(() => clearInterval(writer), 150);
  const removed = await removeProfile(home, { retries: 40, delayMs: 25 });
  clearInterval(writer);
  assert.equal(removed, true);
  assert.equal(existsSync(home), false);
});

test("a remove that meets ENOTEMPTY while the app's helper finishes is tried again", async () => {
  let calls = 0;
  const busy = () => {
    calls++;
    if (calls < 3) throw Object.assign(new Error("Directory not empty"), { code: "ENOTEMPTY" });
  };
  const removed = await removeProfile("/tmp/charter-bench-profile-x", { delayMs: 1, remove: busy });
  assert.equal(removed, true);
  assert.equal(calls, 3);
});

test("a profile that cannot be removed is reported, not thrown", async () => {
  const said = [];
  const removed = await removeProfile("/definitely/not/a/dir/\0bad", {
    retries: 2,
    delayMs: 1,
    warn: (line) => said.push(line),
  });
  assert.equal(removed, false);
  assert.equal(said.length, 1);
  assert.match(said[0], /could not remove the throwaway profile/);
});
