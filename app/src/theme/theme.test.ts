import { describe, expect, it } from "vitest";
import {
  BUILT_IN,
  DEFAULT_THEME,
  TOKENS,
  apply,
  cssVariables,
  inForce,
  isColour,
  load,
  property,
  drawIn,
  xtermTheme,
} from "./theme";
import dark from "./charter-dark.json";
import light from "./charter-light.json";

/** A theme file as one arrives from disk: parsed JSON and nothing more. */
function file(over: Record<string, unknown> = {}): unknown {
  return { name: "probe", appearance: "dark", tokens: {}, ...over };
}

describe("a theme is a complete set of semantic tokens", () => {
  it.each(Object.keys(BUILT_IN))("%s gives a value for every token", (name) => {
    const theme = BUILT_IN[name];
    for (const token of TOKENS) expect(isColour(theme.values[token]), token).toBe(true);
  });

  it("ships more than one theme, so the contract has been tried twice", () => {
    // A token that only ever has one value is a token nobody has proved is a token.
    expect(Object.keys(BUILT_IN).length).toBeGreaterThan(1);
  });

  it("the two built-ins differ everywhere a theme is allowed to", () => {
    // If a token held the same value in both, the theme file would not be what decides it.
    const same = TOKENS.filter(
      (token) => BUILT_IN["charter-dark"].values[token] === BUILT_IN["charter-light"].values[token],
    );
    expect(same).toEqual(["needs-you.text"]);
  });

  it("names a token for what it means, not for what colour it is", () => {
    // `needs-you.base` and `danger.base` were one value — `--stop`, `#c05c5c` — because they
    // are both "charter's red". Splitting them by meaning immediately paid for itself:
    // `contrast.test.ts` measured white on `#c05c5c` at 4.26:1, under AA, and the badge that
    // fails is the count of chats waiting for the operator. Darkening it to `#b85050` fixed
    // that one badge without touching the mark on an answer that cannot be undone, which is a
    // move a shared palette token cannot make.
    const dark = BUILT_IN["charter-dark"].values;
    expect(dark["needs-you.base"]).not.toEqual(dark["danger.base"]);
    expect(TOKENS).toContain("needs-you.base");
    expect(TOKENS).toContain("danger.base");
  });

  it("holds nothing but hex, because a value is written into CSS and into xterm", () => {
    for (const name of Object.keys(BUILT_IN)) {
      for (const value of Object.values(BUILT_IN[name].values)) {
        expect(value, `${name}: ${value}`).toMatch(/^#[0-9a-f]{3,8}$/);
      }
    }
  });
});

describe("what a value may be", () => {
  it.each(["#abc", "#abcd", "#aabbcc", "#aabbccdd", "#ABC", "#AABBCC"])("takes %s", (value) => {
    expect(isColour(value)).toBe(true);
  });

  it.each([
    "rgb(1 2 3)",
    "red",
    "var(--x)",
    "#ab",
    "#abcde",
    "#abcdefg",
    "#aabbcc;}",
    "#aabbcc; } body { display: none",
    " #aabbcc",
    "#aabbcc ",
    "",
  ])("refuses %s", (value) => {
    // The last few are the point: a value goes straight into a CSS declaration, so anything
    // that could close one is a stylesheet somebody else wrote.
    expect(isColour(value)).toBe(false);
  });

  it.each([null, undefined, 16711680, {}, []])("refuses the non-string %s", (value) => {
    expect(isColour(value)).toBe(false);
  });
});

describe("loading a theme that is wrong never leaves the window without one", () => {
  it("answers with the default when the file is not an object", () => {
    for (const raw of [null, undefined, 3, "charter-dark", ["charter-dark"]]) {
      const { theme, complaints } = load(raw);
      expect(theme).toBe(DEFAULT_THEME);
      expect(complaints.map((c) => c.said)).toEqual(["a theme is a JSON object"]);
    }
  });

  it("falls back token by token, so one typo does not cost the other fifty-three", () => {
    const { theme, complaints } = load(
      file({ tokens: { "surface.base": "#123456", "text.primary": "rgb(1 2 3)" } }),
    );
    expect(theme.values["surface.base"]).toBe("#123456");
    expect(theme.values["text.primary"]).toBe(DEFAULT_THEME.values["text.primary"]);
    expect(complaints).toHaveLength(1);
    expect(complaints[0].token).toBe("text.primary");
    expect(complaints[0].said).toContain("rgb(1 2 3)");
  });

  it("says nothing about a token the file simply did not mention", () => {
    // Not naming a token is how a theme says "the built-in one is right", and is the normal
    // way to write a small theme. Only a value that is *there* and unusable is a complaint.
    const { theme, complaints } = load(file({ tokens: { "surface.base": "#123456" } }));
    expect(complaints).toEqual([]);
    expect(theme.values["state.failed"]).toBe(DEFAULT_THEME.values["state.failed"]);
  });

  it("names a token charter does not have rather than ignoring it", () => {
    const { complaints } = load(file({ tokens: { "gray.800": "#123456" } }));
    expect(complaints).toEqual([{ said: "gray.800 is not a charter token", token: "gray.800" }]);
  });

  it("falls back to the LIGHT built-in when the theme says it is light", () => {
    // The appearance picks which complete theme the holes are filled from. Filling a light
    // theme's holes from the dark one is how a theme ends up unreadable in patches.
    const { theme } = load(file({ appearance: "light", tokens: {} }));
    expect(theme.values["text.primary"]).toBe(BUILT_IN["charter-light"].values["text.primary"]);
    expect(theme.appearance).toBe("light");
  });

  it("reads an unknown appearance as dark, and says so", () => {
    const { theme, complaints } = load(file({ appearance: "solarized" }));
    expect(theme.appearance).toBe("dark");
    expect(complaints.map((c) => c.said)).toContain(
      `appearance is "dark" or "light"; read this one as dark`,
    );
  });

  it("gives a nameless theme the built-in's name rather than an empty tab", () => {
    for (const name of [undefined, "", "   ", 7]) {
      const { theme, complaints } = load(file({ name }));
      expect(theme.name).toBe(DEFAULT_THEME.name);
      expect(complaints.map((c) => c.said)).toContain(
        `a theme has a name; called this one ${DEFAULT_THEME.name}`,
      );
    }
  });

  it("complains about tokens that is not an object, and still gives a whole theme", () => {
    const { theme, complaints } = load(file({ tokens: "charter-dark" }));
    expect(complaints.map((c) => c.said)).toContain(
      "tokens is an object of token names to colours",
    );
    for (const token of TOKENS) expect(isColour(theme.values[token]), token).toBe(true);
  });

  it("takes a complete theme without a word", () => {
    const { theme, complaints } = load(light);
    expect(complaints).toEqual([]);
    expect(theme.values).toEqual(BUILT_IN["charter-light"].values);
  });
});

describe("one theme, two consumers", () => {
  it("writes every token as a custom property", () => {
    const written = cssVariables(BUILT_IN["charter-dark"]);
    expect(Object.keys(written)).toHaveLength(TOKENS.length);
    expect(written["--surface-base"]).toBe(dark.tokens["surface.base"]);
    expect(written["--terminal-ansi-bright-white"]).toBe(dark.tokens["terminal.ansi.bright-white"]);
  });

  it("turns a token name into a property name", () => {
    expect(property("surface.base")).toBe("--surface-base");
    expect(property("terminal.ansi.bright-black")).toBe("--terminal-ansi-bright-black");
  });

  it("builds xterm's object with xterm's own key names", () => {
    const object = xtermTheme(BUILT_IN["charter-dark"]);
    expect(object.background).toBe(dark.tokens["terminal.background"]);
    expect(object.foreground).toBe(dark.tokens["terminal.foreground"]);
    expect(object.cursorAccent).toBe(dark.tokens["terminal.cursor-accent"]);
    expect(object.selectionBackground).toBe(dark.tokens["terminal.selection"]);
    expect(object.brightMagenta).toBe(dark.tokens["terminal.ansi.bright-magenta"]);
    // Sixteen ANSI colours plus four, and no key xterm does not know.
    expect(Object.keys(object)).toHaveLength(21);
  });

  it("gives the terminal and the window the same theme, which is the whole point", () => {
    // The defect this layer removes: `SessionPane` used to say `#181818`, `App.css` used to
    // say `--paper: #181818`, and nothing on earth made the two agree.
    const theme = BUILT_IN["charter-light"];
    expect(xtermTheme(theme).background).toBe(cssVariables(theme)["--terminal-background"]);
    expect(xtermTheme(theme).foreground).toBe(cssVariables(theme)["--terminal-foreground"]);
  });
});

describe("putting a theme on the document", () => {
  it("sets every custom property, the colour scheme and the name", () => {
    const element = document.createElement("div");
    apply(BUILT_IN["charter-light"], element);
    expect(element.style.getPropertyValue("--surface-base")).toBe(light.tokens["surface.base"]);
    expect(element.style.getPropertyValue("--state-failed")).toBe(light.tokens["state.failed"]);
    // `color-scheme` is what makes the platform's own scrollbars and form controls match, and
    // no custom property can do it.
    expect(element.style.colorScheme).toBe("light");
    expect(element.dataset.theme).toBe("charter-light");
  });

  it("leaves nothing of the theme before it behind", () => {
    const element = document.createElement("div");
    apply(BUILT_IN["charter-light"], element);
    apply(BUILT_IN["charter-dark"], element);
    for (const token of TOKENS) {
      expect(element.style.getPropertyValue(property(token)), token).toBe(
        BUILT_IN["charter-dark"].values[token],
      );
    }
    expect(element.style.colorScheme).toBe("dark");
  });

  it("makes the theme in force the one a pane opens its terminal in", () => {
    // `SessionPane` reads `inForce()` when it constructs a terminal, so this is the hand-off
    // between the document's colours and the grid's.
    const element = document.createElement("div");
    try {
      drawIn(BUILT_IN["charter-light"], element);
      expect(inForce()).toBe(BUILT_IN["charter-light"]);
      expect(xtermTheme(inForce()).background).toBe(light.tokens["terminal.background"]);
    } finally {
      drawIn(DEFAULT_THEME, element);
    }
  });
});
