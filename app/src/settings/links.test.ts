import { describe, expect, it } from "vitest";
import { catalogue, type Now } from "../actions";
import { noTabs } from "../tabs";
import type { EntryReferrer } from "./driver";
import { OPEN_EXTENSIONS, openVault, PICK_VAULT, referrerElsewhere, showPersona } from "./links";

/**
 * **A link out of Settings names a row of the catalogue** (#1387, #1388): the ids Settings
 * sends are the ones `actions.ts` gives its rows, so a link never names a row that is not
 * there, and runs what the palette runs.
 */
function now(over: Partial<Now> = {}): Now {
  return {
    tabs: noTabs(),
    workspaces: [],
    needsYou: [],
    nameOf: (session) => String(session),
    ...over,
  };
}

describe("a link out of Settings", () => {
  it("names rows the catalogue holds, each doing what the link says", () => {
    const offers = catalogue(now({ plane: "/plane", personas: ["steward"], vaults: ["forge"] }));
    const does = (id: string) => offers.find((one) => one.id === id)?.does;

    expect(does(OPEN_EXTENSIONS)).toEqual({ verb: "showExtensions" });
    expect(does(PICK_VAULT)).toEqual({ verb: "pickVault" });
    expect(does(openVault("forge"))).toMatchObject({
      verb: "openView",
      view: { from: null, view: "vault", key: "forge" },
    });
    expect(does(showPersona("steward"))).toMatchObject({
      verb: "openView",
      view: { from: null, view: "persona", key: "steward" },
    });
  });
});

describe("a referrer changed at another level (#1241)", () => {
  const at = (over: Partial<EntryReferrer>): EntryReferrer => ({
    what: "It uses it.",
    group: null,
    follows: false,
    ...over,
  });

  it("is followed to its persona's tab, through the catalogue's row", () => {
    expect(referrerElsewhere(at({ level: "persona", target: "devops" }))).toEqual({
      action: showPersona("devops"),
      label: "Show devops",
    });
  });

  it("is followed to its group at a workspace's level or the You level", () => {
    expect(
      referrerElsewhere(at({ level: "workspace", target: "alpha", group: "workspace.general" })),
    ).toEqual({ link: { group: "workspace.general", workspace: "alpha" } });
    expect(referrerElsewhere(at({ level: "you", group: "you.editor" }))).toEqual({
      link: { group: "you.editor" },
    });
  });

  it("is not elsewhere at the entry's own level, as a referrer from before it was", () => {
    expect(referrerElsewhere(at({ group: "project.harness" }))).toBeUndefined();
    expect(
      referrerElsewhere(at({ group: "project.harness", level: null, target: null })),
    ).toBeUndefined();
  });

  it("draws no link where it says no place to follow", () => {
    expect(referrerElsewhere(at({ level: "persona" }))).toBeNull();
    expect(referrerElsewhere(at({ level: "workspace", group: "workspace.general" }))).toBeNull();
    expect(referrerElsewhere(at({ level: "you" }))).toBeNull();
  });
});
