import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import type { SettingsChange, SettingsFile, SettingsWhich } from "../bindings";

/**
 * **Edit as TOML** (SE-19, #1169; the spec on #558, V89d): one link per file at the foot of the
 * Project level's nav opens that file's whole text in the right column, with an explicit Save
 * and Discard. Whether a text is refused, and in which words, is the core's
 * (`charter_core::settings`'s tests); the core is the mock here, and what is held is what the
 * person sees and what the window sends.
 */

const PLANE = "/home/dev/plane";

const SHARED_TEXT = `# the team's
[plane]
mode = "push"   # how far a save goes
`;

const LOCAL_TEXT = `[harness]\ndefault = "claude"\n`;

const file = (which: SettingsWhich, text: string, exists = true): SettingsFile => ({
  which,
  file: which === "shared" ? "charter.toml" : "charter.local.toml",
  exists,
  text,
  refusals: [],
  parsed: true,
  fields: [],
});

type Sent = { which: SettingsWhich; base: string | null; change: SettingsChange };

/**
 * The core, as a mock: `refuse` answers every save with these reasons instead of writing. Like
 * the core, a save made against text the file no longer holds is refused. `held`, when given,
 * is awaited before a save answers.
 */
function core({
  refuse = undefined as string[] | undefined,
  local = file("local", LOCAL_TEXT),
  held = undefined as (() => Promise<void>) | undefined,
} = {}) {
  const files: Record<SettingsWhich, SettingsFile> = {
    shared: file("shared", SHARED_TEXT),
    local,
  };
  const sent: Sent[] = [];
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "project_settings":
          return { shared: files.shared, local: files.local };
        case "project_extensions":
          return { extensions: [], local_left_out: null };
        case "project_harness_plugins":
          return [];
        case "extensions_on":
          return [];
        case "save_project_settings": {
          const which = given.which as SettingsWhich;
          const change = given.change as SettingsChange;
          const base = given.base as string | null;
          sent.push({ which, base, change });
          return (held ?? (() => Promise.resolve()))().then(() => {
            if (refuse) return { kind: "refused", reasons: refuse };
            const was = files[which];
            if ((was.exists ? was.text : null) !== base)
              return {
                kind: "refused",
                reasons: [
                  `${was.file} changed on disk since this tab read it, so nothing was saved.`,
                ],
              };
            if (change.kind === "raw")
              files[which] = { ...files[which], exists: true, text: change.text };
            else {
              // A form's change: each key set or gone, and a text that differs.
              let fields = [...files[which].fields];
              for (const edit of change.edits) {
                const at = JSON.stringify(edit.path);
                fields = fields.filter((one) => JSON.stringify(one.path) !== at);
                if (edit.value !== null) fields.push({ path: edit.path, value: edit.value });
              }
              files[which] = { ...files[which], fields, text: `${files[which].text}# form\n` };
            }
            return { kind: "saved", file: files[which] };
          });
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { sent, files };
}

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/dev/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

async function atProject() {
  render(<SettingsTab plane={PLANE} level="project" />);
  await screen.findByRole("button", { name: "General" });
}

/** A file's link, by the name it shows, in the links at the foot of the nav. */
const link = (name: string) =>
  within(screen.getByRole("group", { name: "Edit as TOML" })).getByRole("button", { name });
const editAsToml = (name: string) => userEvent.click(link(name));
const raw = (name: string) => screen.getByRole("textbox", { name: `${name}, as TOML` });

/** Settles the plane-changed event a write outside the tab sends. */
const changedOnDisk = () =>
  act(async () => {
    await emit("plane-changed", { plane: PLANE, changes: [], answers: [{ answer: "settings" }] });
  });

describe("Edit as TOML", () => {
  it("is one link per file at the foot of the nav, not a group of its own", async () => {
    core();
    await atProject();

    const links = screen.getByRole("group", { name: "Edit as TOML" });
    expect(
      within(links)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["charter.toml", "charter.local.toml"]);
    const nav = screen.getByRole("navigation", { name: "Groups" });
    expect(within(nav).queryByRole("button", { name: /TOML/ })).toBeNull();
  });

  it("opens the file's whole text in the right column, in place of the group", async () => {
    core();
    await atProject();

    await editAsToml("charter.toml");

    expect(raw("charter.toml")).toHaveValue(SHARED_TEXT);
    expect(screen.queryByRole("region", { name: "General" })).toBeNull();
    expect(link("charter.toml")).toHaveAttribute("aria-current", "true");
    expect(screen.getByRole("button", { name: "Save charter.toml" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Discard" })).toBeDisabled();
  });

  it("writes nothing as it is typed, and Save writes the whole text against what was read", async () => {
    const { sent } = core();
    await atProject();
    await editAsToml("charter.toml");

    // `[[` is user-event's way of typing one `[`.
    await userEvent.type(raw("charter.toml"), '[[update]\nchannel = "dev"\n');
    expect(sent).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({
      which: "shared",
      base: SHARED_TEXT,
      change: { kind: "raw", text: `${SHARED_TEXT}[update]\nchannel = "dev"\n` },
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Save charter.toml" })).toBeDisabled(),
    );
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("Discard drops the edit and shows the file as it is", async () => {
    const { sent } = core();
    await atProject();
    await editAsToml("charter.local.toml");

    await userEvent.type(raw("charter.local.toml"), "oops");
    await userEvent.click(screen.getByRole("button", { name: "Discard" }));

    expect(raw("charter.local.toml")).toHaveValue(LOCAL_TEXT);
    expect(screen.getByRole("button", { name: "Save charter.local.toml" })).toBeDisabled();
    expect(sent).toHaveLength(0);
  });

  it("says a text the core refuses in its own words, writes nothing and keeps what was typed", async () => {
    const why =
      "charter.toml is not valid TOML: TOML parse error at line 4, column 8 — invalid table header";
    const { files } = core({ refuse: [why] });
    await atProject();
    await editAsToml("charter.toml");

    await userEvent.type(raw("charter.toml"), "[[broken");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Nothing was saved:");
    expect(alert).toHaveTextContent(why);
    expect(raw("charter.toml")).toHaveValue(`${SHARED_TEXT}[broken`);
    expect(files.shared.text).toBe(SHARED_TEXT);
  });

  it("saves a Local file that is not there yet as new, and says the save creates it", async () => {
    const { sent } = core({ local: file("local", "", false) });
    await atProject();
    await editAsToml("charter.local.toml");

    expect(screen.getByText(/Not created yet: the first save creates it\./)).toBeInTheDocument();
    await userEvent.type(raw("charter.local.toml"), "[[harness]");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({ which: "local", base: null });
  });

  it("follows a file changed outside the tab while nothing is typed", async () => {
    const { files } = core();
    await atProject();
    await editAsToml("charter.toml");

    files.shared = { ...files.shared, text: `${SHARED_TEXT}# edited by hand\n` };
    await changedOnDisk();

    await waitFor(() =>
      expect(raw("charter.toml")).toHaveValue(`${SHARED_TEXT}# edited by hand\n`),
    );
  });

  it("keeps an edit when the file changes outside the tab, says so, and saves against what it read", async () => {
    const { files, sent } = core();
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# mine\n");

    files.shared = { ...files.shared, text: `${SHARED_TEXT}# edited by hand\n` };
    await changedOnDisk();

    expect(
      await screen.findByText(/charter\.toml changed on disk since this edit began/),
    ).toBeInTheDocument();
    expect(raw("charter.toml")).toHaveValue(`${SHARED_TEXT}# mine\n`);
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    // The core refuses a save made against text the file no longer holds, so the outside edit is
    // never overwritten unseen.
    expect(sent[0].base).toBe(SHARED_TEXT);

    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(raw("charter.toml")).toHaveValue(files.shared.text);
  });

  it("keeps what was typed when a group is opened and the file's link is pressed again", async () => {
    core();
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# mine\n");

    await userEvent.click(
      within(screen.getByRole("navigation", { name: "Groups" })).getByRole("button", {
        name: "Saving",
      }),
    );
    expect(screen.getByRole("region", { name: "Saving" })).toBeInTheDocument();
    await editAsToml("charter.toml");

    expect(raw("charter.toml")).toHaveValue(`${SHARED_TEXT}# mine\n`);
  });

  it("is where a file that is not TOML is mended: its link is there while its groups are not", async () => {
    const { files } = core();
    files.shared = {
      ...files.shared,
      text: "[plane\n",
      parsed: false,
      refusals: ["charter.toml is not valid TOML"],
    };
    render(<SettingsTab plane={PLANE} level="project" />);
    await screen.findByRole("button", { name: "Harness & profiles" });
    expect(screen.queryByRole("button", { name: "General" })).toBeNull();

    await editAsToml("charter.toml");

    expect(raw("charter.toml")).toHaveValue("[plane\n");
  });

  it("never saves over an outside change, even once the edit was typed back to what it read", async () => {
    const { files, sent } = core();
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "#");
    files.shared = { ...files.shared, text: `${SHARED_TEXT}# edited by hand\n` };
    await changedOnDisk();

    // Typed back to exactly the text it read, then typed again.
    await userEvent.type(raw("charter.toml"), "{Backspace}");
    expect(
      await screen.findByText(/charter\.toml changed on disk since this edit began/),
    ).toBeInTheDocument();
    await userEvent.type(raw("charter.toml"), "# mine\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].base).toBe(SHARED_TEXT);
  });

  it("refuses the save, and keeps the outside change, after an edit was typed back, the file changed, and typing went on", async () => {
    const { files, sent } = core();
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# mine");
    await userEvent.type(raw("charter.toml"), "{Backspace}".repeat("# mine".length));
    const outside = `${SHARED_TEXT}# edited by hand\n`;
    files.shared = { ...files.shared, text: outside };
    await changedOnDisk();
    await screen.findByText(/charter\.toml changed on disk since this edit began/);

    await userEvent.type(raw("charter.toml"), "# then\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/changed on disk/);
    expect(sent.map((one) => one.base)).toEqual([SHARED_TEXT]);
    expect(files.shared.text).toBe(outside);
    // What was typed is kept, and Discard takes up the outside change.
    expect(raw("charter.toml")).toHaveValue(`${SHARED_TEXT}# then\n`);
    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(raw("charter.toml")).toHaveValue(outside);
  });

  it("writes an edit typed straight after a save, against what that save wrote", async () => {
    const { files } = core();
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# one\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(files.shared.text).toBe(`${SHARED_TEXT}# one\n`));

    await userEvent.type(raw("charter.toml"), "# two\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));

    await waitFor(() => expect(files.shared.text).toBe(`${SHARED_TEXT}# one\n# two\n`));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(/changed on disk/)).toBeNull();
  });

  it("loses nothing typed while a save is on its way", async () => {
    let letGo = () => {};
    let holding = true;
    const { files } = core({
      held: () =>
        holding
          ? new Promise<void>((done) => {
              holding = false;
              letGo = done;
            })
          : Promise.resolve(),
    });
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# one\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));
    await screen.findByRole("button", { name: "Saving…" });

    await userEvent.type(raw("charter.toml"), "# two\n");
    await act(async () => letGo());
    await waitFor(() => expect(files.shared.text).toBe(`${SHARED_TEXT}# one\n`));

    // What was typed during the save is still there, as an edit of what the save wrote.
    expect(raw("charter.toml")).toHaveValue(`${SHARED_TEXT}# one\n# two\n`);
    expect(screen.queryByText(/changed on disk/)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(files.shared.text).toBe(`${SHARED_TEXT}# one\n# two\n`));
  });

  it("saves the next edit against what the last save wrote", async () => {
    const { sent } = core();
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# one\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(sent).toHaveLength(1));

    await userEvent.type(raw("charter.toml"), "# two\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));

    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].base).toBe(`${SHARED_TEXT}# one\n`);
  });

  it("takes away a form change's Undo once the file is saved as TOML", async () => {
    core();
    await atProject();
    await userEvent.click(
      within(screen.getByRole("navigation", { name: "Groups" })).getByRole("button", {
        name: "Saving",
      }),
    );
    await userEvent.selectOptions(screen.getByLabelText("Mode"), "commit");
    expect(await screen.findByRole("button", { name: "Undo" })).toBeInTheDocument();

    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# mine\n");
    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Save charter.toml" })).toBeDisabled(),
    );
    await userEvent.click(
      within(screen.getByRole("navigation", { name: "Groups" })).getByRole("button", {
        name: "Saving",
      }),
    );

    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
  });

  it("gives the focus back to the text after Save, Discard and a refusal", async () => {
    core({ refuse: ["charter.toml is not valid TOML"] });
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "[[broken");

    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));
    await screen.findByRole("alert");
    expect(raw("charter.toml")).toHaveFocus();

    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(raw("charter.toml")).toHaveFocus();
  });

  it("gives the focus back to the text after a save that is written", async () => {
    core();
    await atProject();
    await editAsToml("charter.toml");
    await userEvent.type(raw("charter.toml"), "# mine\n");

    await userEvent.click(screen.getByRole("button", { name: "Save charter.toml" }));

    await waitFor(() => expect(raw("charter.toml")).toHaveFocus());
  });

  it("says, under what charter refuses, where a file that is not TOML is mended", async () => {
    const { files } = core();
    files.shared = {
      ...files.shared,
      text: "[plane\n",
      parsed: false,
      refusals: ["charter.toml is not valid TOML"],
    };
    render(<SettingsTab plane={PLANE} level="project" />);

    expect(
      await screen.findByText("Open charter.toml under Edit as TOML to mend it."),
    ).toBeInTheDocument();
  });
});
