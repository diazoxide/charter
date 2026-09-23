import { StrictMode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render as renderBare, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Extensions } from "./Extensions";
import { BUILT_IN, DEFAULT_THEME, drawIn, inForce } from "./theme/theme";

/**
 * A colour for a theme an extension contributes, taken out of charter's own light theme
 * rather than written here.
 *
 * `literals.test.ts` refuses a hex literal anywhere but `src/theme/`, and it is right to: the
 * rule is what keeps a second palette from growing back. A fixture is not an exception to it —
 * any value works, so the one that already exists is the one to use. It differs from the dark
 * theme's, which is what these tests need it to do.
 */
const OTHER = BUILT_IN["charter-light"].values["surface.base"];

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
  drawIn(DEFAULT_THEME);
});

/** The question charter asks, as the core builds it. The two sentences are the core's own. */
const RUNS_AS_YOU =
  "charter does not confine an extension. It runs as you do, with your files, your network " +
  "and your ability to start programs — nothing charter has stops one reading your vaults, " +
  "writing charter's own settings, or changing what your next launch runs. What is listed " +
  "above is what this extension DECLARES, not what it is LIMITED to.";
const FINGERPRINTED =
  "charter has read every file in this extension's directory — not only the ones it " +
  "declares — and will ask again if any of them changes, or if one is added or taken away. " +
  "That catches an extension that changed under you. It is not a defence against one written " +
  "to deceive you, and it is not a boundary.";
/** `charter_core::extension::state_note("cache")`, for an extension that declares one. */
const STATE_NOTE =
  "charter does not read 'cache/'. That is this extension's state directory: the one place " +
  "it may write without charter asking again. charter refuses to load the extension if that " +
  "directory holds a link or a program, so what is in there is data — but charter cannot stop " +
  "a program it has already read from treating its own data as code.";

const ASK = {
  id: "solarized",
  name: "Solarized",
  path: "/home/dev/ext/solarized",
  declares: ["a theme, “Solarized Dark”"],
  fingerprint: "a".repeat(64),
  first: true,
  runs_as_you: RUNS_AS_YOU,
  fingerprint_note: FINGERPRINTED,
  state_note: null as string | null,
};

type Row = {
  id: string;
  name: string;
  path: string;
  standing: string;
  themes_in_force: string[];
  declares: string[];
  refused: string | null;
  ask: typeof ASK | null;
};

function core({
  rows = [] as Row[],
  themes = [] as { extension: string; name: string; text: string }[],
  unreadable = null as string | null,
  dropped = [] as string[],
  picks = null as string | null,
}) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "installed_extensions")
      return {
        built_in_themes: ["charter-dark", "charter-light"],
        extensions: rows,
        unreadable,
        dropped,
      };
    if (cmd === "extension_themes") return themes;
    if (cmd === "pick_extension") return picks;
    if (cmd === "install_extension") return ASK;
    if (cmd === "approve_extension") return null;
    if (cmd === "forget_extension") return null;
    throw new Error(`the window asked for ${cmd}, which this test did not expect`);
  });
  return asked;
}

const newRow: Row = {
  id: "solarized",
  name: "Solarized",
  path: "/home/dev/ext/solarized",
  standing: "new",
  themes_in_force: [],
  declares: ASK.declares,
  refused: null,
  ask: ASK,
};

describe("the extension registry", () => {
  it("names charter's own themes as well as the installed ones", async () => {
    core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);

    expect(await screen.findByText("charter-dark")).toBeInTheDocument();
    expect(screen.getByText("charter-light")).toBeInTheDocument();
    expect(screen.getByText("Solarized")).toBeInTheDocument();
  });

  it("says that an unapproved extension is contributing nothing", async () => {
    core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);

    expect(await screen.findByText(/not approved — contributing nothing/)).toBeInTheDocument();
  });

  it("says that a changed extension is contributing nothing, rather than going quiet", async () => {
    core({ rows: [{ ...newRow, standing: "changed" }] });
    render(<Extensions onClose={() => undefined} />);

    expect(
      await screen.findByText(/changed since you approved it — contributing nothing/),
    ).toBeInTheDocument();
  });

  it("says why an extension charter could not read is contributing nothing", async () => {
    core({
      rows: [
        {
          ...newRow,
          ask: null,
          refused: "'/home/dev/ext/solarized/charter-extension.json' is not there",
        },
      ],
    });
    render(<Extensions onClose={() => undefined} />);

    expect(await screen.findByText(/is not there/)).toBeInTheDocument();
  });

  it("says that an unreadable record puts nothing in force", async () => {
    core({ rows: [], unreadable: "'/home/o/.config/charter/extensions.json' is not JSON" });
    render(<Extensions onClose={() => undefined} />);

    expect(
      await screen.findByText(/nothing an extension declares is in force/),
    ).toBeInTheDocument();
  });
});

describe("the consent surface", () => {
  it("says that an extension runs with the operator's own access", async () => {
    // The whole reason the dialog exists. The operator ruled to ship with no sandbox, so the
    // list of contributions is charter's conduct and not a cage — and a prompt that implied a
    // cage would be worse than no prompt.
    core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));

    const said = await screen.findByText(/does not confine an extension/);
    expect(said).toHaveTextContent("runs as you do");
    expect(said).toHaveTextContent("reading your vaults");
    expect(said).toHaveTextContent("DECLARES");
    expect(said).toHaveTextContent("not what it is LIMITED to");
  });

  it("says that the fingerprint is not a boundary", async () => {
    core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));

    expect(await screen.findByText(/it is not a boundary/)).toBeInTheDocument();
  });

  it("renders the core's sentences as given rather than words of its own", async () => {
    // A dialog that composed its own reassurance would drift kinder than the truth one edit at
    // a time. These two strings come out of `charter_core::extension` and are pinned there.
    core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));

    expect(await screen.findByText(RUNS_AS_YOU)).toBeInTheDocument();
    expect(screen.getByText(FINGERPRINTED)).toBeInTheDocument();
  });

  it("names the one directory charter does not read, when there is one", async () => {
    // charter-app#152. The fingerprint note says charter read every file in the directory; an
    // extension with a state directory has one exception to that, and the exception belongs on
    // the screen where the operator says yes.
    core({ rows: [{ ...newRow, ask: { ...ASK, state_note: STATE_NOTE } }] });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));

    expect(await screen.findByText(STATE_NOTE)).toBeInTheDocument();
  });

  it("says nothing about a state directory for an extension that declares none", async () => {
    // The ordinary case. A dialog that mentioned an exception every extension does not have
    // would teach the operator to skip the paragraph that matters when one does.
    core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));

    await screen.findByText(FINGERPRINTED);
    expect(screen.queryByText(/does not read/)).not.toBeInTheDocument();
  });

  it("names every contribution it was given", async () => {
    core({
      rows: [
        {
          ...newRow,
          declares: [
            "a theme, “Midnight”",
            "a program, bin/x — charter starts it only when you open one of this extension's views",
          ],
          ask: {
            ...ASK,
            declares: [
              "a theme, “Midnight”",
              "a program, bin/x — charter starts it only when you open one of this extension's views",
            ],
          },
        },
      ],
    });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));

    expect(await screen.findByText(/Midnight/)).toBeInTheDocument();
    expect(screen.getByText(/starts it only when you open/)).toBeInTheDocument();
  });

  it("carries back the fingerprint that was shown, and not one fetched again", async () => {
    // charter-app#123's shape: what was drawn and what is recorded are one read.
    const asked = core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));
    await userEvent.click(await screen.findByRole("button", { name: "Trust it" }));

    const approved = asked.find((call) => call.cmd === "approve_extension");
    expect(approved?.args).toMatchObject({
      id: "solarized",
      path: "/home/dev/ext/solarized",
      fingerprint: ASK.fingerprint,
    });
  });

  it("approves nothing when it is cancelled", async () => {
    const asked = core({ rows: [newRow] });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Review" }));
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(asked.some((call) => call.cmd === "approve_extension")).toBe(false);
  });

  it("asks before it approves, and never the other way round", async () => {
    // Installing is not consenting. The picker and the yes are two clicks because they are two
    // questions, and collapsing them approves something before it is drawn.
    const asked = core({ rows: [], picks: "/home/dev/ext/solarized" });
    render(<Extensions onClose={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "Add an extension…" }));

    expect(await screen.findByText(RUNS_AS_YOU)).toBeInTheDocument();
    expect(asked.some((call) => call.cmd === "approve_extension")).toBe(false);
  });
});

describe("a theme an extension contributes", () => {
  it("is drawn when it is in force", async () => {
    core({
      rows: [{ ...newRow, standing: "approved", themes_in_force: ["Solarized Dark"], ask: null }],
      themes: [
        {
          extension: "solarized",
          name: "Solarized Dark",
          text: JSON.stringify({
            name: "Solarized Dark",
            appearance: "dark",
            tokens: { "surface.base": OTHER },
          }),
        },
      ],
    });
    render(<Extensions onClose={() => undefined} />);

    await screen.findByText("Solarized");
    await expect.poll(() => inForce().values["surface.base"]).toBe(OTHER);
    expect(inForce().name).toBe("Solarized Dark");
  });

  it("does not take the window down when it is not JSON at all", async () => {
    core({
      rows: [{ ...newRow, standing: "approved", themes_in_force: ["Broken"], ask: null }],
      themes: [{ extension: "solarized", name: "Broken", text: "}{ not json" }],
    });
    render(<Extensions onClose={() => undefined} />);

    await screen.findByText("Solarized");
    expect(inForce()).toBe(DEFAULT_THEME);
  });

  it("falls back token by token rather than being refused whole", async () => {
    // `theme.load`'s rule, reached through the registry: a window that will not start because a
    // colour was spelled wrong is worse than every possible wrong colour.
    core({
      rows: [{ ...newRow, standing: "approved", themes_in_force: ["Half"], ask: null }],
      themes: [
        {
          extension: "solarized",
          name: "Half",
          text: JSON.stringify({
            name: "Half",
            appearance: "dark",
            tokens: { "surface.base": OTHER, "text.primary": "red; } body { display: none" },
          }),
        },
      ],
    });
    render(<Extensions onClose={() => undefined} />);

    await screen.findByText("Solarized");
    await expect.poll(() => inForce().name).toBe("Half");
    expect(inForce().values["surface.base"]).toBe(OTHER);
    expect(
      inForce().values["text.primary"],
      "a value that is not a colour reached the document",
    ).toBe(DEFAULT_THEME.values["text.primary"]);
  });
});
