import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import { forgetGroups } from "./links";
import type {
  EntryReferrer,
  EntryWritten,
  ProfileEntry,
  SettingsEdit,
  SettingsField,
  SettingsFile,
} from "../bindings";

/**
 * **Settings › Harness & profiles, a page per profile** (ST-4, #1236): each `[harness.<name>]`
 * profile in `charter.local.toml` has a page of its own under Harness & profiles in the nav, with
 * its rows, Rename and Remove; Add (name, kind, command) is on Harness & profiles; a refused
 * Remove or Rename names what uses the profile, with a link; each write has the one-level Undo;
 * and New profile… in the default pickers opens the Add form and picks what it added.
 *
 * The core is a model of the window's contract with it: the profiles, the text they are written
 * as, an identity per profile that changes once it is renamed or changed, and a write refused
 * when the text it is sent against is not the text on disk. What it checks beyond that is
 * `purlis_core::settings::harness_profiles`'s tests.
 */

const PLANE = "/home/dev/plane";

type Profile = { name: string; kind: string; command: string[] };

const text = (profiles: readonly Profile[]) =>
  profiles
    .map(
      (one) =>
        `[harness.${one.name}]\nkind = "${one.kind}"\ncommand = [${one.command.map((w) => `"${w}"`).join(", ")}]\n`,
    )
    .join("\n");

const idOf = (one: Profile) => `profile:${one.name}:${one.kind}:${one.command.join(" ")}`;

const field = (keys: string[], value: SettingsField["value"]): SettingsField => ({
  path: keys.map((one) => ({ key: one })),
  value,
});

/** `charter.local.toml` as the core answers it for `profiles`. */
function fileOf(profiles: readonly Profile[]): SettingsFile {
  return {
    which: "local",
    file: "charter.local.toml",
    exists: profiles.length > 0,
    text: text(profiles),
    refusals: [],
    parsed: true,
    fields: profiles.flatMap((one) => [
      field(["harness", one.name, "kind"], { kind: "text", value: one.kind }),
      field(["harness", one.name, "command"], { kind: "list", value: one.command }),
    ]),
    entries: profiles.map((one) => ({
      collection: "profiles",
      id: idOf(one),
      label: one.name,
      keys: [{ key: "harness" }, { key: one.name }],
      values: [
        { field: "name", value: one.name },
        { field: "kind", value: one.kind },
        { field: "command", value: one.command.join("\n") },
      ],
    })),
  };
}

const SHARED: SettingsFile = {
  which: "shared",
  file: "charter.toml",
  exists: true,
  text: "schema = 1\n",
  refusals: [],
  parsed: true,
  fields: [field(["schema"], { kind: "integer", value: 1 })],
};

const WORK: Profile = { name: "work", kind: "claude", command: ["claude"] };
const ALT: Profile = { name: "alt", kind: "codex", command: ["codex", "--alt"] };

const USED: EntryReferrer = {
  what: '[harness] default = "work" in charter.toml starts new chats on it.',
  group: "project.harness",
};

type Asked =
  | { cmd: "add"; base: string | null; entry: ProfileEntry }
  | { cmd: "remove"; base: string | null; id: string }
  | { cmd: "rename"; base: string | null; id: string; to: string }
  | { cmd: "raw"; base: string | null; text: string }
  | { cmd: "edits"; edits: SettingsEdit[] };

const MOVED = "charter.local.toml changed on disk since this tab read it, so nothing was saved.";

const refusal = (
  over: Partial<{
    fields: { field: string; why: string }[];
    referrers: EntryReferrer[];
    reasons: string[];
  }>,
): EntryWritten => ({ kind: "refused", fields: [], referrers: [], reasons: [], ...over });

const option = (name: string, source = "built-in") => ({
  name,
  kind: name,
  shown: name,
  source,
  is_default: false,
  approval: null,
  ready_to_type: true,
  harness: null,
  sandbox: null,
});

/** The core, as a model of the window's contract with it. `used` names who uses a profile. */
function core({
  profiles: start = [WORK, ALT],
  used = () => [],
  refuseAdd,
}: {
  profiles?: Profile[];
  used?: (name: string) => EntryReferrer[];
  refuseAdd?: (entry: ProfileEntry) => EntryWritten | undefined;
} = {}) {
  let profiles = [...start];
  let shared = SHARED;
  /** Every text the file has been, and the profiles it was: what a whole-text write puts back. */
  const seen = new Map<string, Profile[]>([[text(profiles), profiles]]);
  const asked: Asked[] = [];
  const now = () => fileOf(profiles);
  const saved = (): EntryWritten => ({ kind: "saved", file: now(), added: null, removed: null });
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      const base = given.base as string | null;
      const moved = () => base !== (profiles.length > 0 ? text(profiles) : null);
      switch (cmd) {
        case "project_settings":
          return { shared, local: now() };
        case "project_extensions":
          return { extensions: [], local_left_out: null };
        case "project_harness_plugins":
          return [];
        case "sandbox_state":
          return { on: false, offer: false, said: null, never: [], hosts_changed: null };
        case "start_options":
          return {
            profiles: [
              option("claude"),
              option("codex"),
              ...profiles.map((one) => option(one.name, "charter.local.toml")),
            ],
            refused: [],
            personas: [],
            persona: null,
            ignore_fix: null,
            declares_none: false,
          };
        case "add_project_profile": {
          const entry = given.entry as ProfileEntry;
          asked.push({ cmd: "add", base, entry });
          if (moved()) return refusal({ reasons: [MOVED] });
          const answer = refuseAdd?.(entry);
          if (answer) return answer;
          const made = { name: entry.name, kind: entry.kind, command: entry.command };
          profiles = [...profiles, made];
          seen.set(text(profiles), profiles);
          return { ...saved(), added: idOf(made) };
        }
        case "remove_project_profile": {
          const id = given.id as string;
          asked.push({ cmd: "remove", base, id });
          if (moved()) return refusal({ reasons: [MOVED] });
          const took = profiles.find((one) => idOf(one) === id);
          if (!took)
            return refusal({ reasons: ["That profile is not in the file as it was shown"] });
          const users = used(took.name);
          if (users.length > 0) return refusal({ referrers: users });
          profiles = profiles.filter((one) => one !== took);
          seen.set(text(profiles), profiles);
          return {
            ...saved(),
            removed: [
              { field: "name", value: took.name },
              { field: "kind", value: took.kind },
              { field: "command", value: took.command.join("\n") },
            ],
          };
        }
        case "rename_project_profile": {
          const id = given.id as string;
          const to = given.to as string;
          asked.push({ cmd: "rename", base, id, to });
          if (moved()) return refusal({ reasons: [MOVED] });
          const was = profiles.find((one) => idOf(one) === id);
          if (!was)
            return refusal({ reasons: ["That profile is not in the file as it was shown"] });
          if (to.includes("."))
            return refusal({
              fields: [{ field: "name", why: `profile '${to}' is not a name purlis accepts` }],
            });
          const users = used(was.name);
          if (users.length > 0) return refusal({ referrers: users });
          const renamed = { ...was, name: to };
          profiles = profiles.map((one) => (one === was ? renamed : one));
          seen.set(text(profiles), profiles);
          return { ...saved(), added: idOf(renamed) };
        }
        case "save_project_settings": {
          const change = given.change as
            { kind: "raw"; text: string } | { kind: "edits"; edits: SettingsEdit[] };
          if (change.kind === "edits") {
            asked.push({ cmd: "edits", edits: change.edits });
            shared = { ...shared, text: `${shared.text}# written\n` };
            return { kind: "saved", file: given.which === "shared" ? shared : now() };
          }
          asked.push({ cmd: "raw", base, text: change.text });
          if (moved()) return { kind: "refused", reasons: [MOVED] };
          profiles = seen.get(change.text) ?? profiles;
          return { kind: "saved", file: now() };
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { asked, names: () => profiles.map((one) => one.name) };
}

beforeEach(() => {
  forgetThisLaunch();
  forgetGroups();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/dev/layout.json", found: false, document: null, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const pages = () =>
  within(nav())
    .getAllByRole("button")
    .map((one) => one.textContent);
const open = async (name: string) =>
  userEvent.click(await within(nav()).findByRole("button", { name }));
const shown = () => screen.getByRole("region");

async function atProject() {
  render(<SettingsTab plane={PLANE} level="project" />);
  await screen.findByRole("button", { name: "General" });
}

describe("a page per harness profile", () => {
  it("is in the nav for each profile, under Harness & profiles, set in as its page", async () => {
    core();
    await atProject();

    await waitFor(() => expect(pages()).toContain("work"));
    const at = pages().indexOf("Harness & profiles");
    expect(pages().slice(at, at + 3)).toEqual(["Harness & profiles", "work", "alt"]);
    expect(within(nav()).getByRole("button", { name: "work" })).toHaveClass("ui-settings-sub");
    expect(within(nav()).getByRole("button", { name: "Harness & profiles" })).not.toHaveClass(
      "ui-settings-sub",
    );
  });

  it("holds the profile's kind, command and environment, with Rename and Remove and no Add", async () => {
    core();
    await atProject();
    await open("alt");

    expect(shown()).toHaveAccessibleName("alt");
    expect(screen.getByLabelText("alt: kind")).toHaveValue("codex");
    expect(screen.getByLabelText("alt: command")).toHaveValue("codex\n--alt");
    expect(screen.getByLabelText("alt: environment")).toBeInTheDocument();
    expect(screen.queryByLabelText("work: kind")).toBeNull();
    expect(screen.getByRole("button", { name: "Rename alt" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove alt" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add profile" })).toBeNull();
  });

  it("is listed on Harness & profiles, each opening its page", async () => {
    core();
    await atProject();
    await open("Harness & profiles");

    await userEvent.click(screen.getByRole("button", { name: "Open alt" }));

    expect(shown()).toHaveAccessibleName("alt");
  });
});

describe("Add profile", () => {
  it("asks for the name, kind and command, and the new profile has its page", async () => {
    const { asked } = core();
    await atProject();
    await open("Harness & profiles");

    await userEvent.click(screen.getByRole("button", { name: "Add profile" }));
    const form = screen.getByRole("form", { name: "New profile" });
    expect(within(form).getByLabelText("Name")).toHaveFocus();
    await userEvent.type(within(form).getByLabelText("Name"), "cx");
    await userEvent.selectOptions(within(form).getByLabelText("Kind"), "codex");
    await userEvent.type(within(form).getByLabelText("Command"), "codex{Enter}--fast");
    await userEvent.click(within(form).getByRole("button", { name: "Add profile" }));

    await waitFor(() => expect(pages()).toContain("cx"));
    expect(asked[0]).toEqual({
      cmd: "add",
      base: text([WORK, ALT]),
      entry: { name: "cx", kind: "codex", command: ["codex", "--fast"] },
    });
    expect(screen.queryByRole("form", { name: "New profile" })).toBeNull();
    expect(screen.getByText("Added cx.")).toBeVisible();
  });

  it("says a refused field under that field, and keeps what was typed", async () => {
    core({
      refuseAdd: () =>
        refusal({
          fields: [{ field: "name", why: "a profile cannot be named 'default'" }],
        }),
    });
    await atProject();
    await open("Harness & profiles");
    await userEvent.click(screen.getByRole("button", { name: "Add profile" }));
    const form = screen.getByRole("form", { name: "New profile" });
    await userEvent.type(within(form).getByLabelText("Name"), "default");
    await userEvent.type(within(form).getByLabelText("Command"), "claude");

    await userEvent.click(within(form).getByRole("button", { name: "Add profile" }));

    expect(await within(form).findByText("a profile cannot be named 'default'")).toBeVisible();
    expect(within(form).getByLabelText("Name")).toHaveValue("default");
    expect(pages()).not.toContain("default");
  });

  it("is undone by removing what it added", async () => {
    const { asked, names } = core();
    await atProject();
    await open("Harness & profiles");
    await userEvent.click(screen.getByRole("button", { name: "Add profile" }));
    const form = screen.getByRole("form", { name: "New profile" });
    await userEvent.type(within(form).getByLabelText("Name"), "cx");
    await userEvent.type(within(form).getByLabelText("Command"), "claude");
    await userEvent.click(within(form).getByRole("button", { name: "Add profile" }));
    await waitFor(() => expect(names()).toContain("cx"));

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));

    await waitFor(() => expect(names()).not.toContain("cx"));
    expect(asked.at(-1)).toMatchObject({ cmd: "remove", id: "profile:cx:claude:claude" });
  });
});

describe("Remove a profile", () => {
  it("takes it and its page out, and Undo writes the file's text back exactly", async () => {
    const { asked, names } = core();
    await atProject();
    await open("alt");

    await userEvent.click(screen.getByRole("button", { name: "Remove alt" }));

    await waitFor(() => expect(pages()).not.toContain("alt"));
    expect(names()).toEqual(["work"]);
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() => expect(names()).toEqual(["work", "alt"]));
    expect(asked.at(-1)).toEqual({ cmd: "raw", base: text([WORK]), text: text([WORK, ALT]) });
  });

  it("is refused while the default harness names it, naming that with a link to fix it", async () => {
    const { names } = core({ used: (name) => (name === "work" ? [USED] : []) });
    await atProject();
    await open("work");

    await userEvent.click(screen.getByRole("button", { name: "Remove work" }));

    const said = await within(screen.getByRole("group", { name: "work" })).findByRole("alert");
    expect(said).toHaveTextContent("This profile is not removed while this uses it:");
    expect(said).toHaveTextContent(USED.what);
    expect(names()).toEqual(["work", "alt"]);
    await userEvent.click(within(said).getByRole("button", { name: "Fix it in Settings" }));
    expect(shown()).toHaveAccessibleName("Harness & profiles");
  });
});

describe("Rename a profile", () => {
  it("renames one nothing uses, goes to its page under the new name, and Undo renames it back", async () => {
    const { asked, names } = core();
    await atProject();
    await open("alt");

    await userEvent.click(screen.getByRole("button", { name: "Rename alt" }));
    const form = screen.getByRole("form", { name: "Rename alt" });
    const name = within(form).getByLabelText("New name");
    expect(name).toHaveValue("alt");
    await userEvent.clear(name);
    await userEvent.type(name, "fast");
    await userEvent.click(within(form).getByRole("button", { name: "Rename" }));

    await waitFor(() => expect(shown()).toHaveAccessibleName("fast"));
    expect(pages()).not.toContain("alt");
    expect(asked.at(-1)).toEqual({
      cmd: "rename",
      base: text([WORK, ALT]),
      id: idOf(ALT),
      to: "fast",
    });
    expect(screen.getByText("Renamed alt to fast.")).toBeVisible();

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(names()).toEqual(["work", "alt"]));
    // The page it was on is gone: the person lands on the collection's group, not on General.
    expect(shown()).toHaveAccessibleName("Harness & profiles");
    expect(pages()).toContain("alt");
    expect(asked.at(-1)).toMatchObject({
      cmd: "rename",
      id: "profile:fast:codex:codex --alt",
      to: "alt",
    });
  });

  it("is refused while something uses the profile, naming it", async () => {
    const { names } = core({ used: (name) => (name === "work" ? [USED] : []) });
    await atProject();
    await open("work");

    await userEvent.click(screen.getByRole("button", { name: "Rename work" }));
    const form = screen.getByRole("form", { name: "Rename work" });
    await userEvent.type(within(form).getByLabelText("New name"), "2");
    await userEvent.click(within(form).getByRole("button", { name: "Rename" }));

    const said = await within(screen.getByRole("group", { name: "work" })).findByRole("alert");
    expect(said).toHaveTextContent("This profile is not renamed while this uses it:");
    expect(said).toHaveTextContent(USED.what);
    expect(names()).toEqual(["work", "alt"]);
  });

  it("says a refused name under its field and keeps the form open", async () => {
    core();
    await atProject();
    await open("alt");
    await userEvent.click(screen.getByRole("button", { name: "Rename alt" }));
    const form = screen.getByRole("form", { name: "Rename alt" });
    await userEvent.type(within(form).getByLabelText("New name"), ".x");

    await userEvent.click(within(form).getByRole("button", { name: "Rename" }));

    expect(
      await within(form).findByText("profile 'alt.x' is not a name purlis accepts"),
    ).toBeVisible();
    expect(screen.queryByText(/is not renamed while/)).toBeNull();
    expect(within(form).getByLabelText("New name")).toHaveValue("alt.x");
  });
});

describe("New profile… in the default pickers", () => {
  it("opens the Add form, and picks the profile once it is added", async () => {
    const { asked } = core({ profiles: [] });
    await atProject();
    await open("Harness & profiles");
    const harness = screen.getByRole("combobox", { name: "Default harness" });
    await waitFor(() =>
      expect(
        within(harness)
          .getAllByRole("option")
          .map((one) => one.textContent),
      ).toContain("New profile…"),
    );

    await userEvent.selectOptions(harness, "New profile…");
    const form = await screen.findByRole("form", { name: "New profile" });
    await waitFor(() => expect(within(form).getByLabelText("Name")).toHaveFocus());
    await userEvent.type(within(form).getByLabelText("Name"), "work");
    await userEvent.type(within(form).getByLabelText("Command"), "claude");
    await userEvent.click(within(form).getByRole("button", { name: "Add profile" }));

    await waitFor(() =>
      expect(asked.at(-1)).toEqual({
        cmd: "edits",
        edits: [
          {
            path: [{ key: "harness" }, { key: "default" }],
            value: { kind: "text", value: "work" },
          },
        ],
      }),
    );
    expect(asked.filter((one) => one.cmd === "add")).toHaveLength(1);
  });
});
