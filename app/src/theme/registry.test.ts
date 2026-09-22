/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { BUILT_IN } from "./theme";

/**
 * The registry's answer to "what has contributed what to this window" has to include the themes
 * charter itself ships, and those live in two places that cannot import each other: the window's
 * bundle (`BUILT_IN` here) and `charter_core::extension::BUILT_IN_THEMES`, which is what the
 * survey hands back.
 *
 * Two lists that must agree, with the agreement tested rather than remembered. Without this, a
 * third built-in theme would ship, the registry would go on naming two, and the one surface that
 * is supposed to be the whole answer would quietly stop being it — which is the failure mode ADR
 * 0041 builds the registry to prevent, arrived at from the inside.
 */
/// From `app`, which is vitest's working directory here — the same anchor
/// `literals.test.ts` uses to read the real source tree rather than a fixture.
const CORE = join(process.cwd(), "..", "crates", "charter-core", "src", "extension.rs");

describe("charter's own themes", () => {
  it("are the same two the core's registry names", () => {
    const source = readFileSync(CORE, "utf8");
    expect(source.length, `${CORE} read as empty`).toBeGreaterThan(0);
    const declared = /BUILT_IN_THEMES:\s*\[&str;\s*\d+\]\s*=\s*\[([^\]]*)\]/.exec(source);
    expect(declared, `BUILT_IN_THEMES was not found in ${CORE}`).not.toBeNull();

    const named = [...(declared?.[1] ?? "").matchAll(/"([^"]+)"/g)].map((hit) => hit[1]);
    expect(named.sort()).toEqual(Object.keys(BUILT_IN).sort());
  });
});
