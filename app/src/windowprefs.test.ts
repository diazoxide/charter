import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { drawWhatIsInForce } from "./Extensions";
import { BUILT_IN, DEFAULT_THEME, drawIn, inForce } from "./theme/theme";
import {
  aboutThisMachine,
  atCreation,
  GLOBAL,
  sayAboutThisMachine,
  theirTheme,
  type Reading,
} from "./windowprefs";

const PATH = "/home/op/.config/charter/theme.json";
/** A colour that is a value, taken from a theme so that no colour is written here. */
const RAISED = DEFAULT_THEME.values["surface.raised"];
const NONE: Reading = { path: "", found: false, document: null, trouble: null };

const handed = (theme: Partial<Reading>) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: NONE,
    theme: { path: PATH, found: true, document: null, trouble: null, ...theme },
  };
};

beforeEach(() => {
  Reflect.deleteProperty(globalThis, GLOBAL);
  sayAboutThisMachine("theme", undefined);
  drawIn(DEFAULT_THEME);
});
afterEach(() => {
  clearMocks();
  drawIn(DEFAULT_THEME);
});

describe("what the window was handed as it was created", () => {
  it("is nothing at all when it was handed nothing", () => {
    expect(atCreation()).toEqual({ layout: NONE, theme: NONE });
  });

  it("costs the preference and never the window when it is not what the Rust side writes", () => {
    for (const given of [7, "layout", { layout: 3, theme: { found: "yes", trouble: 4 } }]) {
      (globalThis as Record<string, unknown>)[GLOBAL] = given;

      const { layout, theme } = atCreation();
      expect(layout).toEqual(NONE);
      expect(theme.found).toBe(false);
      expect(theme.trouble).toBeNull();
    }
  });
});

describe("the operator's own theme", () => {
  it("is the theme the window is drawn in, token by token", () => {
    handed({
      document: {
        name: "Mine",
        appearance: "light",
        tokens: { "surface.base": RAISED },
      },
    });

    const theirs = theirTheme();

    expect(theirs?.name).toBe("Mine");
    expect(theirs?.values["surface.base"]).toBe(RAISED);
    // A token it did not name is the built-in of its appearance's.
    expect(theirs?.values["text.primary"]).toBe(BUILT_IN["charter-light"].values["text.primary"]);
    expect(aboutThisMachine()).toEqual([]);
  });

  it("says in the drawer what it had to put right, and where the file is", () => {
    handed({
      document: {
        name: "Mine",
        appearance: "dark",
        tokens: { "surface.base": "red; } body { display: none", "surface.bass": RAISED },
      },
    });

    const theirs = theirTheme();

    expect(theirs?.values["surface.base"]).toBe(DEFAULT_THEME.values["surface.base"]);
    const [said] = aboutThisMachine();
    expect(said.subject).toBe("theme");
    expect(said.detail).toContain(PATH);
    expect(said.detail).toContain("surface.bass is not a charter token");
    expect(said.detail).toContain("surface.base is");
  });

  it("that could not be read leaves the built-in, and the drawer says why", () => {
    handed({ trouble: `${PATH} is not JSON: trailing comma` });

    expect(theirTheme()).toBeUndefined();
    expect(aboutThisMachine()[0].detail).toContain("is not JSON");
    expect(aboutThisMachine()[0].remedy).toContain(PATH);
  });

  it("is not there on a machine with no file, and nothing is said", () => {
    handed({ found: false });

    expect(theirTheme()).toBeUndefined();
    expect(aboutThisMachine()).toEqual([]);
  });

  it("is not repainted over by an extension's theme", async () => {
    handed({ document: { name: "Mine", appearance: "dark", tokens: {} } });
    const theirs = theirTheme();
    if (theirs === undefined) throw new Error("the operator's theme was not read");
    drawIn(theirs);
    mockIPC((cmd) =>
      cmd === "extension_themes"
        ? [{ extension: "solarized", name: "Solarized", text: '{"name":"Solarized"}' }]
        : null,
    );

    await drawWhatIsInForce();

    expect(inForce()).toBe(theirs);
  });
});
