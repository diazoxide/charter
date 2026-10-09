import { describe, expect, it } from "vitest";
import { catalogue, type Now } from "../actions";
import { noTabs } from "../tabs";
import { OPEN_EXTENSIONS, openVault, PICK_VAULT, showPersona } from "./links";

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
