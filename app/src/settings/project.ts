import { useCallback, useEffect, useRef, useState } from "react";
import { useProjectThemeAnswers } from "../projectTheme";
import {
  commands,
  type HarnessPlugins,
  type PlaneId,
  type ProjectExtensions,
  type ProjectTheme,
  type SandboxState,
  type SettingsChange,
  type SettingsEdit,
  type SettingsFile,
  type SettingsStep,
  type SettingsWhich,
} from "../bindings";
import {
  answered,
  entries,
  EXTENSIONS,
  forgeBlocks,
  harnessPluginGroups,
  key,
  listAt,
  LOCAL,
  NO_EXTENSIONS,
  planeGroup,
  reposGroup,
  SHARED,
  shown,
  textAt,
  themeGroup,
  valueAt,
  type Control,
  type Entries,
  type Saving,
  type Shown,
} from "./fileControls";
import {
  asked,
  fileSetting,
  useSettingsDriver,
  type Driven,
  type EntryOp,
  type EntryWrote,
  type Wrote,
} from "./driver";
import type { Collection, FileSetting, SettingsFileId, SettingsGroup } from "./groups";
import { settled } from "../PlaneEdits";

/**
 * **The Project level** (SE-17, #1167; V89b, V89e, V89h): a project's settings in the Settings
 * tab, grouped as General · Saving · Harness & profiles · Sandbox · Forges · Extensions ·
 * Appearance · Plugins. "Saving" is the old page's Plane and Repos together.
 *
 * **The groups are data**, declared once in {@link projectGroups} with their stable ids, and
 * **their settings are the old page's controls** (`fileControls.ts`), each with the file it is
 * kept in; SE-19 retired that page, and its raw view is the level's Edit as TOML. A group's settings depend on what the files and the core say (a forge block, an
 * extension, a repo), so the declaration is a function of what was read; a group with no
 * setting is hidden.
 *
 * **One driver for the tab** ({@link useProjectLevel}, on `driver.ts`'s, which the Workspace
 * level shares): each change is written as its own keys through the core (`save_project_settings`, `charter_core::settings::save`, which keeps every
 * other key and comment and refuses what the next read would refuse), one write at a time and
 * each against the file as it now stands; the last change can be undone; and a refused write is
 * kept by the setting it was for, while the files are read again so what is shown is what is on
 * disk.
 */

/** What the core said about the project, which the groups are declared from. */
export type ProjectRead = {
  shared: SettingsFile;
  local: SettingsFile;
  extensions: ProjectExtensions;
  harnesses: readonly HarnessPlugins[];
  theme: ProjectTheme | undefined;
  saving: Saving;
  sandbox: SandboxState | undefined;
  /** What the project has for a picker to name (ST-1): each list once the core has answered. */
  entries: Partial<Entries>;
};

/** The sentence a setting's help ends on: where it is kept, and who sees it. */
export const KEPT: Record<SettingsWhich, string> = {
  shared: "Kept in charter.toml, which your team sees.",
  local: "Kept in charter.local.toml, on this machine only.",
};

/**
 * The tables both files' readers read (`profiles`' rule for `charter.local.toml`): a key in one
 * of them may be kept in either file. `[plane] worktrees` is the Shared file's alone, and
 * `[harness]`, `[chat_env]` the Local file's: those are never offered the other.
 */
const EITHER = new Set(["plane", "repos", "extensions", "theme", "harness_plugins"]);

/** Whether a key in `charter.toml` may be moved to `charter.local.toml` and back. */
function eitherFile(path: readonly SettingsStep[]): boolean {
  const [table, name] = path;
  return (
    table?.key !== undefined &&
    EITHER.has(table.key) &&
    !(table.key === "plane" && name?.key === "worktrees")
  );
}

/**
 * One of the old page's controls as a setting of `group`, kept in `file`. A key either file may
 * hold is `movable` (SE-18): its row's file choice says where it is kept, so its help does not.
 */
function setting(group: string, file: SettingsWhich, control: Control): FileSetting {
  const one = fileSetting(group, file, control, KEPT[file]);
  return file === "shared" && eitherFile(one.key)
    ? { ...fileSetting(group, file, control, ""), movable: true }
    : one;
}

/**
 * **A picker's choices and the core's word on its value** (ST-1, #1225): what the project has,
 * once the core has listed it, and the sentence among the file's standing refusals that is about
 * this key — the core says a default naming nothing as `[persona] default = "ghost" …`, as it
 * says `[harness] default`'s.
 */
function picking(one: FileSetting, file: Shown, entries: Partial<Entries>): FileSetting {
  if (one.names === undefined) return one;
  const [table, name] = one.key;
  const about = `[${table?.key ?? ""}] ${name?.key ?? ""} = `;
  return {
    ...one,
    choices: entries[one.names],
    standing: file.refusals.find((why) => why.startsWith(about)),
  };
}

/** `[plane] assisted_by` and `[repos.<name>] assisted_by` (V67): how an agent's commit says so. */
function assistedBy(path: string[]): Control {
  return textAt(key(...path), "Assisted-by trailer", {
    kind: "choice",
    choices: ["full", "llm"],
    hint: "How a commit an agent made names it: full is the harness and its model, llm the bare LLM.",
  });
}

/** What each kind's own host is, when a block names none — `forge::Kind::default_host`. */
const KIND_HOST: Readonly<Record<string, string>> = {
  gitlab: "gitlab.com",
  github: "github.com",
};

/**
 * **Forges as a collection** (ST-3): each `[[forge]]` block an entry over its own rows, and the
 * Add form's four fields, written by `charter_core::settings::forges` (`add_project_forge`,
 * `remove_project_forge`).
 */
function forgesCollection(shared: Shown, settings: readonly FileSetting[]): Collection {
  return {
    name: "forges",
    noun: "forge",
    entries: forgeBlocks(shared).map((index) => {
      const at = (key: string) =>
        shown(valueAt(shared, [{ key: "forge" }, { index }, { key }])).trim();
      const kind = at("kind") || "gitlab";
      const owner = at("group") || at("owner");
      const host = at("host") || KIND_HOST[kind] || kind;
      return {
        label: `Forge ${index + 1}: ${kind}${owner ? ` ${owner}` : ""} at ${host}`,
        index,
        settings: settings
          .filter((one) => one.key[0]?.key === "forge" && one.key[1]?.index === index)
          .map((one) => one.id),
      };
    }),
    fields: [
      {
        field: "kind",
        label: "Kind",
        help: "Which forge it is.",
        kind: "choice",
        choices: ["gitlab", "github"],
        initial: "gitlab",
      },
      {
        field: "owner",
        label: "Owner",
        help: "The GitLab group or GitHub org whose repos discover lists.",
        kind: "text",
      },
      {
        field: "host",
        label: "Host",
        help: "A bare host, with a port if it needs one. Empty is the kind's own: gitlab.com or github.com.",
        kind: "text",
      },
      {
        field: "exclude",
        label: "Repos never listed",
        help: "One repo name per line.",
        kind: "lines",
      },
    ],
  };
}

/** `[sandbox] mode`, which this tab turns on and never back off (D-SE17g). */
const SANDBOX_MODE = key("sandbox", "mode");

/** Whether two keys are the same key. */
function same(a: readonly SettingsStep[], b: readonly SettingsStep[]): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Whether writing `back` would leave the sandbox less confining than it is: `mode` taken out,
 * or set to anything but `on` — both read as no sandbox, or refused (ADR 0067). An Undo that
 * would do that is never offered, whichever setting's change it undoes.
 */
function loosensTheSandbox(back: readonly SettingsEdit[]): boolean {
  return back.some(
    (edit) =>
      same(edit.path, SANDBOX_MODE) && !(edit.value?.kind === "text" && edit.value.value === "on"),
  );
}

/** What `[sandbox] egress` is when no file sets it: every preset (ADR 0067 §3). */
const ALL_EGRESS = ["model-providers", "forge", "toolchains"];

/**
 * **The sandbox** (ADR 0067): `mode` may only be turned on here. A committed file may turn it on
 * and never off, and taking `mode` out — the only way back to "not set" — is left to the file
 * itself, so this tab adds no way to loosen what a chat is confined to. `egress` shows the
 * presets in force, and an empty box writes `[]`, which reaches no host, rather than taking the
 * key out, which would reach all three.
 */
function sandboxSettings(shared: Shown, sandbox: SandboxState | undefined): Control[] {
  const set = valueAt(shared, SANDBOX_MODE) !== undefined;
  // The opt-out count is this machine's, and is never sent (ADR 0067 §7, V78 d).
  const said = sandbox?.on
    ? `On: every chat charter starts here runs in a sandbox.${sandbox.said ? ` ${sandbox.said}. Counted on this machine only, and never sent.` : ""}`
    : "Not set: chats here run without a sandbox. On runs every chat sandboxed.";
  const egress = key("sandbox", "egress");
  return [
    {
      ...textAt(SANDBOX_MODE, "Sandbox mode", {
        kind: "choice",
        choices: ["on"],
        hint: said,
      }),
      unset: set ? undefined : "not set",
    },
    {
      ...listAt(
        egress,
        "Hosts it may reach",
        "One preset per line: model-providers, forge, toolchains. Empty reaches no host.",
      ),
      read: (file) => {
        const value = valueAt(file, egress);
        return value === undefined ? ALL_EGRESS.join("\n") : shown(value);
      },
      edits: (draft) => [
        {
          path: egress,
          value: { kind: "list", value: entries(draft) },
        },
      ],
    },
  ];
}

/**
 * **The eight groups**, in V89h's order, each with its stable id. Everything a project's two
 * files hold that charter reads is here: what the old page's forms covered, and the keys only its
 * raw view reached — `[plane] assisted_by`, `[repos.<name>] assisted_by`, `[sandbox]`,
 * `[theme] icons` and `[chat_env] pass`. A key the file holds that nothing reads (`[frame]`) and
 * the project's id, minted once (`[project] id`), are not settings.
 *
 * A file that is not TOML has no settings to show until it is mended: its standing refusals say
 * why, above the groups.
 */
export function projectGroups(read: ProjectRead): SettingsGroup[] {
  const shared: Shown = read.shared;
  const local: Shown = read.local;
  const sharedOk = shared.parsed;
  const localOk = local.parsed;
  const [general, forges] = SHARED;
  const [harness, profiles] = LOCAL;
  const fromShared = (group: string, controls: Control[]) =>
    sharedOk
      ? controls.map((one) => picking(setting(group, "shared", one), shared, read.entries))
      : [];
  const fromLocal = (group: string, controls: Control[]) =>
    localOk
      ? controls.map((one) => picking(setting(group, "local", one), local, read.entries))
      : [];

  const sharedGeneral = asked(general, shared, read).controls;
  const isDefaultHarness = (one: Control) => one.id === JSON.stringify(key("harness", "default"));

  const plane = asked(planeGroup("either"), shared, read);
  const repos = asked(reposGroup("either"), shared, read);
  const repoNames = (answered(read.saving)?.repos ?? []).map((repo) => repo.name);
  const savingControls = [
    ...plane.controls,
    assistedBy(["plane", "assisted_by"]),
    ...repoNames.flatMap((name) => [
      ...repos.controls.filter((one) => one.label.startsWith(`${name}: `)),
      { ...assistedBy(["repos", name, "assisted_by"]), label: `${name}: assisted-by trailer` },
    ]),
  ];

  const extensions = asked(EXTENSIONS, shared, read);
  const theme = asked(themeGroup("not set — the window's own theme"), shared, read);
  const plugins = harnessPluginGroups(read.harnesses, "project").map((group) =>
    asked(group, shared, read),
  );
  const forgeSettings = fromShared("project.forges", asked(forges, shared, read).controls);

  return [
    {
      id: "project.general",
      label: "General",
      help: "What a chat starts on when nothing else names it, the update channel, the version lock and where worktrees go.",
      settings: fromShared(
        "project.general",
        sharedGeneral.filter((one) => !isDefaultHarness(one)),
      ),
    },
    {
      id: "project.saving",
      label: "Saving",
      help: "How far a save of the project goes, and of each workspace repo.",
      settings: fromShared("project.saving", savingControls),
      notes: [...new Set([...plane.notes, ...repos.notes])],
    },
    {
      id: "project.harness",
      label: "Harness & profiles",
      help: "The harness a chat starts on, and this machine's profiles and the environment its chats are given.",
      settings: [
        ...fromShared("project.harness", sharedGeneral.filter(isDefaultHarness)),
        ...fromLocal("project.harness", [
          ...asked(harness, local, read).controls,
          ...asked(profiles, local, read).controls,
          listAt(
            key("chat_env", "pass"),
            "Environment passed to chats",
            "More of this machine's environment every chat here is given: a variable's name per line, or a prefix ending in *. A credential's name passes only whole.",
          ),
        ]),
      ],
    },
    {
      id: "project.sandbox",
      label: "Sandbox",
      help: "Whether the chats charter starts here run in a sandbox, and which hosts it lets them reach.",
      settings: fromShared("project.sandbox", sandboxSettings(shared, read.sandbox)).map((one) =>
        same(one.key, SANDBOX_MODE) ? { ...one, oneWay: true } : one,
      ),
    },
    {
      id: "project.forges",
      label: "Forges",
      help: "Where the project's repos are found: one block per [[forge]] in charter.toml.",
      settings: forgeSettings,
      ...(sharedOk ? { collection: forgesCollection(shared, forgeSettings) } : {}),
    },
    {
      id: "project.extensions",
      label: "Extensions",
      help: "Which of this machine's extensions are on in this project, and what each is set to.",
      settings: fromShared("project.extensions", extensions.controls),
      notes: extensions.notes,
    },
    {
      id: "project.appearance",
      label: "Appearance",
      help: "The theme and the icons the window draws while this project is in front.",
      settings: fromShared("project.appearance", [
        ...theme.controls,
        textAt(key("theme", "icons"), "Icons", {
          hint: "The file trees' icons: charter-icons, or an extension's as <extension>/<icon theme>. Empty is charter-icons.",
        }),
      ]),
      notes: theme.notes,
    },
    {
      id: "project.plugins",
      label: "Plugins",
      help: "Which harness plugins the chats charter starts here have on.",
      settings: fromShared(
        "project.plugins",
        plugins.flatMap((one) => one.controls),
      ),
      notes: plugins.flatMap((one) => one.notes),
    },
  ];
}

/** What the driver answers at the Project level: what was read, and what to do with a setting. */
export type ProjectLevel =
  | { state: "reading" }
  | { state: "trouble"; trouble: string }
  | (Driven<ProjectFiles> & {
      read: ProjectRead;
      /** Lists what the pickers name again: once something has been made for one (ST-1). */
      readEntries: () => Promise<void>;
    });

/** The project's two files. */
type ProjectFiles = { shared: SettingsFile; local: SettingsFile };

/**
 * **The Project level's driver** (`driver.ts`): reads the two files and what the core says is
 * in force, and writes one setting at a time through `save_project_settings`.
 */
export function useProjectLevel(plane: PlaneId): ProjectLevel {
  const [extensions, setExtensions] = useState<ProjectExtensions>(NO_EXTENSIONS);
  const [harnesses, setHarnesses] = useState<HarnessPlugins[]>([]);
  const [theme, setTheme] = useState<ProjectTheme>();
  const [saving, setSaving] = useState<Saving>();
  const [sandbox, setSandbox] = useState<SandboxState>();
  const [entries, setEntries] = useState<Partial<Entries>>({});
  /** The newest asking of what is in force: an answer to an older one is dropped. */
  const asking = useRef(0);

  const readTheme = useCallback(() => {
    void commands
      .projectTheme(plane, null)
      .then((said) => setTheme(said.status === "ok" ? (said.data ?? undefined) : undefined))
      .catch(() => setTheme(undefined));
  }, [plane]);

  /**
   * **What the pickers name** (ST-1): the profiles a chat can start on and the personas, as the
   * new-chat picker reads them (`start_options`), and the workspaces, as the sidebar does. A list
   * the core did not answer is left as it was: a picker without one names nothing as missing.
   */
  const readEntries = useCallback(
    () =>
      Promise.all([
        settled(commands.startOptions(plane)).then((said) => {
          // A whole-window test answers a command it does not care about with nothing.
          const options = said.status === "ok" ? said.data : undefined;
          if (options) {
            const profile = options.profiles.map((one) => one.name);
            setEntries((was) => ({ ...was, profile, persona: options.personas }));
          }
        }),
        settled(commands.planeSidebar(plane)).then((said) => {
          const sidebar = said.status === "ok" ? said.data : undefined;
          if (sidebar) {
            const workspace = sidebar.workspaces.map((one) => one.name);
            setEntries((was) => ({ ...was, workspace }));
          }
        }),
      ]).then(() => undefined),
    [plane],
  );

  /** What is in force, read again after every read and write so each sentence says what it now is. */
  const readInForce = useCallback(() => {
    const mine = ++asking.current;
    const newest = () => asking.current === mine;
    void commands
      .projectExtensions(plane, null)
      .then((said) => {
        if (newest())
          setExtensions(said.status === "ok" ? (said.data ?? NO_EXTENSIONS) : NO_EXTENSIONS);
      })
      .catch(() => newest() && setExtensions(NO_EXTENSIONS));
    void commands
      .projectHarnessPlugins(plane, null)
      .then((said) => {
        if (newest()) setHarnesses(said.status === "ok" ? (said.data ?? []) : []);
      })
      .catch(() => newest() && setHarnesses([]));
    void commands
      .projectSavingInForce(plane)
      .then((said) => {
        if (newest())
          setSaving(said.status === "ok" ? (said.data ?? undefined) : { trouble: said.error });
      })
      .catch((err: unknown) => newest() && setSaving({ trouble: String(err) }));
    void commands
      .sandboxState(plane)
      .then((said) => {
        if (newest()) setSandbox(said.status === "ok" ? (said.data ?? undefined) : undefined);
      })
      .catch(() => newest() && setSandbox(undefined));
    readTheme();
    void readEntries();
  }, [plane, readTheme, readEntries]);

  // An approval or a removal in the Extensions dialog changes what the theme draws.
  const answers = useProjectThemeAnswers(plane);
  useEffect(() => {
    if (answers > 0) readTheme();
  }, [answers, readTheme]);

  /** Writes one of the two files through the core: a setting's keys, or its whole text. */
  const saveFile = (
    which: SettingsFileId,
    base: string | null,
    change: SettingsChange,
    both: ProjectFiles,
  ): Promise<Wrote<ProjectFiles>> =>
    commands
      .saveProjectSettings(plane, which as SettingsWhich, base, change)
      .then((said) =>
        said.status === "error"
          ? { refused: [said.error] }
          : said.data.kind === "refused"
            ? { refused: said.data.reasons }
            : { saved: { ...both, [which]: said.data.file } },
      );

  /** An add or a remove in one of the level's collections, through the core's function for it. */
  const entry = (op: EntryOp, both: ProjectFiles): Promise<EntryWrote<ProjectFiles>> => {
    const base = both.shared.exists ? both.shared.text : null;
    if (op.collection !== "forges")
      return Promise.resolve({
        refused: {
          fields: {},
          referrers: [],
          reasons: [`No collection is called ${op.collection}.`],
        },
      });
    const asked =
      "add" in op
        ? commands.addProjectForge(plane, base, {
            kind: op.add.kind ?? "",
            owner: op.add.owner ?? "",
            host: op.add.host ?? "",
            exclude: entries(op.add.exclude ?? ""),
          })
        : commands.removeProjectForge(plane, base, op.remove);
    return asked.then((said): EntryWrote<ProjectFiles> => {
      if (said.status === "error")
        return { refused: { fields: {}, referrers: [], reasons: [said.error] } };
      if (said.data.kind === "saved")
        return { saved: { ...both, shared: said.data.file }, file: "shared" };
      const fields: Record<string, string[]> = {};
      for (const one of said.data.fields) (fields[one.field] ??= []).push(one.why);
      return {
        refused: { fields, referrers: said.data.referrers, reasons: said.data.reasons },
      };
    });
  };

  const driver = useSettingsDriver<ProjectFiles>(plane, {
    plane,
    read: () =>
      commands
        .projectSettings(plane)
        .then((said) => (said.status === "ok" ? { ok: said.data } : { trouble: said.error })),
    files: (both) => both,
    save: (which, base, edits, both) => saveFile(which, base, { kind: "edits", edits }, both),
    saveRaw: (which, base, text, both) => saveFile(which, base, { kind: "raw", text }, both),
    inForce: readInForce,
    entry,
    // Nothing here takes the sandbox back off (D-SE17g): not an Undo, whichever setting's change
    // it undoes, nor a reset, nor a move out of charter.toml (D-SE18e).
    mayUndo: (back) => !loosensTheSandbox(back),
    move: (to, paths, both) =>
      commands
        .moveProjectSettings(
          plane,
          to,
          both.shared.exists ? both.shared.text : null,
          both.local.exists ? both.local.text : null,
          paths,
        )
        .then((said) =>
          said.status === "error"
            ? { refused: [said.error] }
            : said.data.kind === "refused"
              ? { refused: said.data.reasons }
              : { saved: said.data.settings },
        ),
  });

  if (driver.state !== "read") return driver;
  return {
    ...driver,
    read: { ...driver.now, extensions, harnesses, theme, saving, sandbox, entries },
    readEntries,
  };
}
