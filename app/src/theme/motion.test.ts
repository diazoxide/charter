import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DURATIONS,
  EASINGS,
  isDuration,
  isEasing,
  LONGEST,
  MOTION_TOKENS,
  motionProperty,
  motionVariables,
  REDUCE,
} from "./motion";
import { apply, BUILT_IN, DEFAULT_THEME, load } from "./theme";

/** The milliseconds a written duration says, read as a number so no test here spells out a
 *  CSS time — `literals.test.ts` holds this directory's tests to nothing, but the habit is the
 *  point. */
const ms = (written: string) => Number.parseFloat(written);

describe("motion is a vocabulary, like colour", () => {
  it("gives every built-in theme every duration and every easing", () => {
    for (const theme of Object.values(BUILT_IN)) {
      for (const token of DURATIONS) expect(isDuration(theme.motion.durations[token])).toBe(true);
      for (const token of EASINGS) expect(isEasing(theme.motion.easings[token])).toBe(true);
    }
  });

  it("names a token for what the motion is for, not for how long it is", () => {
    // `duration.enter`, never `duration.200`: a theme speeding arrivals up must not also
    // speed up whatever happened to share a rung of a scale with them.
    for (const token of MOTION_TOKENS) expect(token).not.toMatch(/\d/);
  });

  it("writes each token to its own property, apart from the colours", () => {
    expect(motionProperty("duration.enter")).toBe("--motion-duration-enter");
    expect(motionProperty("easing.steady")).toBe("--motion-easing-steady");
  });
});

describe("what a motion value may be", () => {
  it.each([0, 1, 160, LONGEST])("takes %s milliseconds", (value) => {
    expect(isDuration(value)).toBe(true);
  });

  it.each([
    ["a string with a unit", "150ms"],
    ["a negative", -1],
    ["a fraction", 12.5],
    ["past the ceiling", LONGEST + 1],
    ["not a number", Number.NaN],
    ["an injection", "0ms; } body { display: none"],
  ])("refuses %s as a duration", (_, value) => {
    expect(isDuration(value)).toBe(false);
  });

  it.each([[[0, 0, 1, 1]], [[0.2, 0, 0, 1]], [[0.34, 1.56, 0.64, 1]]])(
    "takes %j as an easing",
    (value) => {
      expect(isEasing(value)).toBe(true);
    },
  );

  it.each([
    ["an x below 0, which CSS drops whole", [-0.1, 0, 1, 1]],
    ["an x past 1, which CSS drops whole", [0, 0, 1.2, 1]],
    ["a y that flings the surface off the window", [0, 5, 1, 1]],
    ["three numbers", [0, 0, 1]],
    ["strings", ["0", "0", "1", "1"]],
    ["infinity", [0, Number.POSITIVE_INFINITY, 1, 1]],
    ["a CSS keyword", "ease-out"],
  ])("refuses %s as an easing", (_, value) => {
    expect(isEasing(value)).toBe(false);
  });
});

describe("loading a theme's motion never leaves the window without any", () => {
  it("moves a theme that says nothing about motion the way its built-in does, silently", () => {
    const { theme, complaints } = load({ name: "quiet", appearance: "light", tokens: {} });
    expect(theme.motion).toEqual(BUILT_IN["charter-light"].motion);
    expect(complaints).toEqual([]);
  });

  it("takes what it is given and fills the rest, token by token", () => {
    const { theme, complaints } = load({
      name: "brisk",
      appearance: "dark",
      tokens: {},
      motion: { "duration.enter": 90, "easing.enter": [0, 0, 0.1, 1] },
    });
    expect(complaints).toEqual([]);
    expect(theme.motion.durations["duration.enter"]).toBe(90);
    expect(theme.motion.easings["easing.enter"]).toEqual([0, 0, 0.1, 1]);
    expect(theme.motion.durations["duration.spin"]).toBe(
      BUILT_IN["charter-dark"].motion.durations["duration.spin"],
    );
  });

  it("lets a theme turn motion off by saying so", () => {
    const off = Object.fromEntries(DURATIONS.map((token) => [token, 0]));
    const { theme, complaints } = load({ name: "still", appearance: "dark", motion: off });
    expect(complaints.filter((c) => c.said.includes("motion"))).toEqual([]);
    for (const token of DURATIONS) expect(theme.motion.durations[token]).toBe(0);
  });

  it("falls back from a bad value and says what it used instead", () => {
    const { theme, complaints } = load({
      name: "typo",
      appearance: "dark",
      tokens: {},
      motion: { "duration.enter": "160ms", "easing.enter": [2, 0, 0, 1] },
    });
    const base = BUILT_IN["charter-dark"].motion;
    expect(theme.motion.durations["duration.enter"]).toBe(base.durations["duration.enter"]);
    expect(theme.motion.easings["easing.enter"]).toEqual(base.easings["easing.enter"]);
    expect(complaints.map((c) => c.said)).toEqual([
      expect.stringMatching(/^duration\.enter is "160ms", which is not a whole number/),
      expect.stringMatching(/^easing\.enter is \[2,0,0,1\], which is not \[x1, y1, x2, y2\]/),
    ]);
  });

  it("names a motion token charter does not have rather than ignoring it", () => {
    const { complaints } = load({ name: "x", appearance: "dark", motion: { "duration.200": 200 } });
    expect(complaints.map((c) => c.said)).toContain("duration.200 is not a charter motion token");
  });

  it("complains about motion that is not an object, and still moves", () => {
    const { theme, complaints } = load({ name: "x", appearance: "dark", motion: [1, 2] });
    expect(theme.motion).toEqual(BUILT_IN["charter-dark"].motion);
    expect(complaints.map((c) => c.said)).toContain(
      "motion is an object of motion token names to values",
    );
  });

  it("never copies a theme's own text into the stylesheet", () => {
    // Parse and re-emit (ADR 0041, property 4): what reaches a declaration is written from a
    // number, so a string in the file has nowhere to go.
    const { theme } = load({
      name: "hostile",
      appearance: "dark",
      motion: {
        "duration.enter": "1ms; } body { display: none",
        "easing.enter": ["0); } body { display: none", 0, 1, 1],
      },
    });
    const written = Object.values(motionVariables(theme.motion, false)).join(" ");
    expect(written).not.toContain("display");
    expect(written).not.toContain("}");
  });
});

describe("reduced motion is the layer's, and handled once", () => {
  it("writes the theme's durations and easings as custom properties", () => {
    const written = motionVariables(DEFAULT_THEME.motion, false);
    expect(Object.keys(written).sort()).toEqual(MOTION_TOKENS.map(motionProperty).sort());
    for (const token of DURATIONS) {
      expect(ms(written[motionProperty(token)])).toBe(DEFAULT_THEME.motion.durations[token]);
    }
    expect(written["--motion-easing-enter"]).toBe(
      `cubic-bezier(${DEFAULT_THEME.motion.easings["easing.enter"].join(", ")})`,
    );
  });

  it("collapses every duration when the operator asked for less motion", () => {
    // Which is what a component written tomorrow gets for free: it reads a token, and the token
    // is zero for anyone who asked.
    const written = motionVariables(DEFAULT_THEME.motion, true);
    for (const token of DURATIONS) expect(ms(written[motionProperty(token)])).toBe(0);
    // The easings stay, so the set of properties on the document is the same either way.
    expect(written["--motion-easing-enter"]).toMatch(/^cubic-bezier\(/);
  });

  it("puts the motion on the document with the colours", () => {
    const element = document.createElement("div");
    apply(DEFAULT_THEME, element, false);
    expect(ms(element.style.getPropertyValue("--motion-duration-enter"))).toBe(
      DEFAULT_THEME.motion.durations["duration.enter"],
    );
    apply(DEFAULT_THEME, element, true);
    expect(ms(element.style.getPropertyValue("--motion-duration-enter"))).toBe(0);
  });
});

describe("the window follows the setting while it is up", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("reads the setting when the theme is drawn, and redraws when it changes", async () => {
    // A fresh copy of the module, because the listener is installed once per window.
    vi.resetModules();
    let reduced = true;
    const listeners: ((event: { matches: boolean }) => void)[] = [];
    const matchMedia = vi.fn((query: string) => ({
      get matches() {
        return query === REDUCE && reduced;
      },
      addEventListener: (_: string, listener: (event: { matches: boolean }) => void) =>
        listeners.push(listener),
    }));
    vi.stubGlobal("matchMedia", matchMedia);
    const { drawIn, DEFAULT_THEME: fresh } = await import("./theme");

    const element = document.createElement("div");
    drawIn(fresh, element);
    expect(ms(element.style.getPropertyValue("--motion-duration-spin"))).toBe(0);
    expect(matchMedia).toHaveBeenCalledWith(REDUCE);

    // Drawn again — an extension's theme arriving after the first frame — and still one
    // listener, not one per draw.
    drawIn(fresh, element);
    expect(listeners).toHaveLength(1);

    reduced = false;
    for (const listener of listeners) listener({ matches: false });
    expect(ms(element.style.getPropertyValue("--motion-duration-spin"))).toBe(
      fresh.motion.durations["duration.spin"],
    );
  });
});
