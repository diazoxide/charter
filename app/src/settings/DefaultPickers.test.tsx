import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import type { SettingsEdit, SettingsField, SettingsFile, SettingsWhich } from "../bindings";

/**
 * **The default persona, workspace and harness are pickers with "New…"** (ST-1, #1225; the
 * spec on #1221, V91r): each lists what the project has — its personas, its workspaces, the
 * harness profiles a chat can start on — and a New… entry that opens the matching create flow
 * and picks what it made. A value set by hand that names nothing is shown as such, with the
 * core's sentence for it, and is replaced from the picker. The core is the mock: what it lists
 * and refuses is `purlis_core`'s to test.
 */

const PLANE = "/home/dev/plane";

const field = (keys: string[], value: SettingsField["value"]): SettingsField => ({
  path: keys.map((one) => ({ key: one })),
  value,
});

const text = (value: string) => ({ kind: "text" as const, value });

function file(which: SettingsWhich, fields: SettingsField[], refusals: string[] = []) {
  return {
    which,
    file: which === "shared" ? "charter.toml" : "charter.local.toml",
    exists: true,
    text: which === "shared" ? "schema = 1\n" : "",
    refusals,
    parsed: true,
    fields,
  } satisfies SettingsFile;
}

function applied(was: SettingsFile, edits: readonly SettingsEdit[]): SettingsFile {
  let fields = [...was.fields];
  for (const edit of edits) {
    const at = JSON.stringify(edit.path);
    fields = fields.filter((one) => JSON.stringify(one.path) !== at);
    if (edit.value !== null) fields.push({ path: edit.path, value: edit.value });
  }
  // What the core would say once the value names something: no refusal about that key.
  return { ...was, fields, refusals: [], text: `${was.text}# written\n` };
}

const profile = (name: string, source = "built-in") => ({
  name,
  kind: name === "work" ? "claude" : name,
  shown: name,
  source,
  is_default: false,
  approval: null,
  ready_to_type: true,
  harness: null,
  sandbox: null,
});

const GHOST =
  '[persona] default = "ghost" names no persona in this project, so purlis reads it as no default. Pick one that is here, or make it.';

/** The core, as a mock: the two files, and what the project has to pick from. */
function core({
  shared = file("shared", []),
  local = file("local", []),
  personas = ["steward", "scribe"],
  workspaces = ["alpha", "beta"],
} = {}) {
  const files: Record<SettingsWhich, SettingsFile> = { shared, local };
  const sent: { which: SettingsWhich; edits: SettingsEdit[] }[] = [];
  const made: { cmd: string; args: Record<string, unknown> }[] = [];
  const has = { personas: [...personas], workspaces: [...workspaces] };
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
        case "sandbox_state":
          return {
            on: false,
            offer: false,
            said: null,
            never: [],
            hosts_changed: null,
            presets: [],
            persona_hosts: [],
            besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
            policy: null,
          };
        case "start_options":
          return {
            profiles: [profile("claude"), profile("codex"), profile("work", "charter.local.toml")],
            refused: [],
            personas: has.personas,
            persona: null,
            ignore_fix: null,
            declares_none: false,
          };
        case "plane_sidebar":
          return {
            root: PLANE,
            workspaces: has.workspaces.map((name) => ({
              name,
              path: `${PLANE}/workspaces/${name}`,
              vision: "",
              todos: [],
              chats: [],
              colour: null,
              live: false,
            })),
            personas: has.personas,
            persona: null,
            unfiled: [],
          };
        case "persona_create":
          made.push({ cmd, args: given });
          has.personas.push(given.name as string);
          return [`Created persona ${given.name as string}.`];
        case "workspace_create":
          made.push({ cmd, args: given });
          has.workspaces.push(given.name as string);
          return [`Created workspace ${given.name as string}.`];
        case "save_project_settings": {
          const which = given.which as SettingsWhich;
          const change = given.change as { kind: "edits"; edits: SettingsEdit[] };
          sent.push({ which, edits: change.edits });
          files[which] = applied(files[which], change.edits);
          return { kind: "saved", file: files[which] };
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { sent, made };
}

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: {
      path: "/home/dev/.config/charter/layout.json",
      found: false,
      document: null,
      trouble: null,
    },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const open = (name: string) => userEvent.click(within(nav()).getByRole("button", { name }));
const picker = (label: string) => screen.getByRole("combobox", { name: label });
const offered = (label: string) =>
  within(picker(label))
    .getAllByRole("option")
    .map((one) => one.textContent);
const rowOf = (label: string) => picker(label).closest(".ui-setting-row") as HTMLElement;

async function atProject() {
  render(<SettingsTab plane={PLANE} level="project" />);
  await screen.findByRole("button", { name: "General" });
}

describe("the default persona, workspace and harness", () => {
  it("are pickers over what the project has, each with a New… entry", async () => {
    core();
    await atProject();

    await waitFor(() =>
      expect(offered("Default persona")).toEqual(["not set", "steward", "scribe", "New persona…"]),
    );
    expect(offered("Default workspace")).toEqual(["not set", "alpha", "beta", "New workspace…"]);

    await open("Harness & profiles");
    await waitFor(() =>
      expect(offered("Default harness")).toEqual([
        "not set",
        "claude",
        "codex",
        "work",
        "New profile…",
      ]),
    );
    expect(offered("Default profile")).toEqual([
      "not set",
      "claude",
      "codex",
      "work",
      "New profile…",
    ]);
  });

  it("write the one picked as the key's value", async () => {
    const { sent } = core();
    await atProject();
    await waitFor(() => expect(offered("Default persona")).toContain("scribe"));

    await userEvent.selectOptions(picker("Default persona"), "scribe");

    await waitFor(() =>
      expect(sent).toEqual([
        {
          which: "shared",
          edits: [{ path: [{ key: "persona" }, { key: "default" }], value: text("scribe") }],
        },
      ]),
    );
    expect(picker("Default persona")).toHaveValue("scribe");
  });

  it("show a value that names nothing as such, with the core's sentence, until it is replaced", async () => {
    const { sent } = core({
      shared: file("shared", [field(["persona", "default"], text("ghost"))], [GHOST]),
    });
    await atProject();
    await waitFor(() => expect(offered("Default persona")).toContain("steward"));

    expect(picker("Default persona")).toHaveValue("ghost");
    expect(offered("Default persona")).toEqual([
      "not set",
      "ghost — names nothing here",
      "steward",
      "scribe",
      "New persona…",
    ]);
    expect(within(rowOf("Default persona")).getByRole("alert")).toHaveTextContent(GHOST);

    await userEvent.selectOptions(picker("Default persona"), "steward");

    await waitFor(() => expect(picker("Default persona")).toHaveValue("steward"));
    expect(sent.at(-1)?.edits).toEqual([
      { path: [{ key: "persona" }, { key: "default" }], value: text("steward") },
    ]);
    await waitFor(() => expect(within(rowOf("Default persona")).queryByRole("alert")).toBeNull());
    expect(offered("Default persona")).not.toContain("ghost — names nothing here");
  });

  it("say a value names nothing only where the core says so, not where the list lacks it", async () => {
    // A workspace declared before it is made is one charter makes where it is first used, and
    // a persona the new-chat picker does not list (`_shared`) is still the core's to judge: no
    // sentence from the core, so each is shown plainly, as held (D-ST1-1, amended; V91l).
    core({
      shared: file("shared", [
        field(["workspace", "default"], text("later")),
        field(["persona", "default"], text("_shared")),
      ]),
    });
    await atProject();
    await waitFor(() => expect(offered("Default workspace")).toContain("alpha"));
    await waitFor(() => expect(offered("Default persona")).toContain("steward"));

    expect(picker("Default workspace")).toHaveValue("later");
    expect(offered("Default workspace")).toEqual([
      "not set",
      "later",
      "alpha",
      "beta",
      "New workspace…",
    ]);
    expect(within(rowOf("Default workspace")).queryByRole("alert")).toBeNull();
    expect(picker("Default persona")).toHaveValue("_shared");
    expect(offered("Default persona")).toEqual([
      "not set",
      "_shared",
      "steward",
      "scribe",
      "New persona…",
    ]);
    expect(within(rowOf("Default persona")).queryByRole("alert")).toBeNull();
  });

  it("New persona… makes one through persona_create and picks it", async () => {
    const { sent, made } = core();
    await atProject();
    await waitFor(() => expect(offered("Default persona")).toContain("New persona…"));

    await userEvent.selectOptions(picker("Default persona"), "New persona…");
    const dialog = await screen.findByRole("dialog", { name: "New persona" });
    // Nothing is written by opening it.
    expect(sent).toEqual([]);
    await userEvent.type(within(dialog).getByLabelText("Name"), "reviewer");
    await userEvent.type(within(dialog).getByLabelText("Delegate when"), "a change wants review");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create persona" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(made).toEqual([
      {
        cmd: "persona_create",
        args: expect.objectContaining({ name: "reviewer", delegateWhen: "a change wants review" }),
      },
    ]);
    await waitFor(() => expect(picker("Default persona")).toHaveValue("reviewer"));
    expect(sent).toEqual([
      {
        which: "shared",
        edits: [{ path: [{ key: "persona" }, { key: "default" }], value: text("reviewer") }],
      },
    ]);
    await waitFor(() => expect(offered("Default persona")).toContain("reviewer"));
  });

  it("New workspace… makes one through workspace_create and picks it; Cancel picks nothing", async () => {
    const { sent, made } = core();
    await atProject();
    await waitFor(() => expect(offered("Default workspace")).toContain("New workspace…"));

    await userEvent.selectOptions(picker("Default workspace"), "New workspace…");
    let dialog = await screen.findByRole("dialog", { name: /New workspace/ });
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(made).toEqual([]);
    expect(sent).toEqual([]);

    await userEvent.selectOptions(picker("Default workspace"), "New workspace…");
    dialog = await screen.findByRole("dialog", { name: /New workspace/ });
    await userEvent.type(within(dialog).getByLabelText("Name"), "gamma");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

    await waitFor(() => expect(picker("Default workspace")).toHaveValue("gamma"));
    expect(made).toEqual([
      { cmd: "workspace_create", args: expect.objectContaining({ name: "gamma" }) },
    ]);
    expect(sent.at(-1)?.edits).toEqual([
      { path: [{ key: "workspace" }, { key: "default" }], value: text("gamma") },
    ]);
  });
});
