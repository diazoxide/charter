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
  envAt,
  EXTENSIONS,
  harnessPluginGroups,
  key,
  KINDS,
  listAt,
  LOCAL,
  NO_EXTENSIONS,
  planeGroup,
  reposGroup,
  SHARED,
  textAt,
  themeGroup,
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
import { GRANTED, grantedGroup } from "./GrantedList";
import { dispatchGroup } from "./dispatch";
import { onAMac } from "../tabKeys";
import { sandboxCommandReturned } from "../sandboxAsked";
import {
  SANDBOX_MODE,
  lockedRow,
  sandboxControls,
  sandboxNotes,
  sandboxReasons,
  sandboxSentence,
} from "./sandbox";

/**
 * **The Project level** (SE-17, #1167; V89b, V89e, V89h): a project's settings in the Settings
 * tab, grouped as General · Saving · Harness & profiles · Sandbox · Dispatch · Forges · Extensions ·
 * Appearance · Plugins. "Saving" is the old page's Plane and Repos together.
 *
 * **The groups are data**, declared once in {@link projectGroups} with their stable ids, and
 * **their settings are the old page's controls** (`fileControls.ts`), each with the file it is
 * kept in; SE-19 retired that page, and its raw view is the level's Edit as TOML. A group's settings depend on what the files and the core say (a forge block, an
 * extension, a repo), so the declaration is a function of what was read; a group with no
 * setting is hidden.
 *
 * **One driver for the tab** ({@link useProjectLevel}, on `driver.ts`'s, which the Workspace
 * level shares): each change is written as its own keys through the core (`save_project_settings`, `purlis_core::settings::save`, which keeps every
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
  /** The project itself, for the Granted list (#1348), which reads the core on its own. */
  plane?: PlaneId;
};

/**
 * The sentence a setting's help ends on: where it is kept, and who sees it — named as the file
 * the project uses, `charter.toml` or `purlis.toml` (#1340).
 */
export function kept(file: Pick<SettingsFile, "which" | "file">): string {
  return file.which === "shared"
    ? `Kept in ${file.file}, which your team sees.`
    : `Kept in ${file.file}, on this machine only.`;
}

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

/** The names of the project's two files, as the core read them: `charter.toml` or `purlis.toml`. */
function namesOf(read: ProjectRead): Record<SettingsWhich, string> {
  return { shared: read.shared.file, local: read.local.file };
}

/**
 * One of the old page's controls as a setting of `group`, kept in `file`. A key either file may
 * hold is `movable` (SE-18): its row's file choice says where it is kept, so its help does not.
 */
function setting(
  group: string,
  file: SettingsWhich,
  control: Control,
  names: Readonly<Record<SettingsWhich, string>>,
): FileSetting {
  const one = fileSetting(group, file, control, kept({ which: file, file: names[file] }));
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

/**
 * **Forges as a collection** (ST-3): each `[[forge]]` block the core lists in `charter.toml` an
 * entry over its own rows, and the Add form's four fields, written by
 * `purlis_core::settings::forges` (`add_project_forge`, `remove_project_forge`).
 */
function forgesCollection(shared: SettingsFile, settings: readonly FileSetting[]): Collection {
  const under = (keys: readonly SettingsStep[], key: readonly SettingsStep[]) =>
    keys.every((step, at) => JSON.stringify(step) === JSON.stringify(key[at]));
  const collection: Collection = {
    name: "forges",
    noun: "forge",
    base: shared.exists ? shared.text : null,
    entries: (shared.entries ?? [])
      .filter((one) => one.collection === "forges")
      .map((one) => ({
        id: one.id,
        label: one.label,
        settings: settings
          .filter((setting) => under(one.keys, setting.key))
          .map((setting) => setting.id),
      })),
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
  return collection;
}

/** The address of the page of your own hosts (#1341): a sub-page of Sandbox. */
export const MY_HOSTS = "project.sandbox.mine";

/**
 * **A sandbox's hosts as a collection** (#1341): each entry of `[sandbox] hosts` the core lists
 * in `file`, with Remove, and an Add form of one field the core checks
 * (`purlis_core::settings::hosts`, `add_sandbox_host`, `remove_sandbox_host`): the project's in
 * `charter.toml` (`hosts`), which every teammate follows, or yours in `charter.local.toml`
 * (`myHosts`), on this machine only. A host the core refuses is said under the field, with why.
 */
function hostsCollection(file: SettingsFile, name: "hosts" | "myHosts"): Collection {
  return {
    name,
    noun: "host",
    base: file.exists ? file.text : null,
    entries: (file.entries ?? [])
      .filter((one) => one.collection === name)
      .map((one) => ({
        id: one.id,
        label: one.label,
        settings: [],
        confirm: one.values.some((value) => value.field === "confirmed" && value.value === "no"),
      })),
    fields: [
      {
        field: "host",
        label: "Host",
        help: "A domain such as api.example.com, *.example.com for every name under it, or an IP address such as 10.0.0.5. Add :port to reach a port other than HTTPS's.",
        kind: "text",
      },
    ],
  };
}

/** The address of the profile `name`'s own page (ST-4): a sub-page of Harness & profiles. */
export function profilePage(name: string): string {
  return `project.profile.${name}`;
}

/**
 * **Harness profiles as a collection** (ST-4, #1236): each `[harness.<name>]` table the core lists
 * in `charter.local.toml` is an entry with a page of its own, and the Add form asks for its name,
 * kind and command, written by `purlis_core::settings::harness_profiles` (`add_project_profile`,
 * `remove_project_profile`, `rename_project_profile`). On Harness & profiles each entry is a
 * heading that opens its page, with Remove; its page holds it alone, with its rows, Rename and
 * Remove.
 */
function profilesCollection(
  local: SettingsFile,
  only?: { id: string; settings: readonly FileSetting[] },
): Collection {
  const listed = (local.entries ?? []).filter((one) => one.collection === "profiles");
  const nameOf = (one: (typeof listed)[number]) =>
    one.values.find((value) => value.field === "name")?.value ?? one.label;
  return {
    name: "profiles",
    noun: "profile",
    base: local.exists ? local.text : null,
    entries: listed
      .filter((one) => only === undefined || one.id === only.id)
      .map((one) => ({
        id: one.id,
        label: one.label,
        name: nameOf(one),
        settings: only === undefined ? [] : only.settings.map((setting) => setting.id),
        page: only === undefined ? profilePage(nameOf(one)) : undefined,
      })),
    adds: only === undefined,
    home: "project.harness",
    renames: only !== undefined,
    pageOf: profilePage,
    fields: [
      {
        field: "name",
        label: "Name",
        help: "What the profile is called in the new-chat picker: letters, digits, '_' and '-'.",
        kind: "text",
      },
      {
        field: "kind",
        label: "Kind",
        help: "Which harness it runs.",
        kind: "choice",
        choices: KINDS,
        initial: KINDS[0],
      },
      {
        field: "command",
        label: "Command",
        help: "One argument per line, program first. No shell runs it, and its first run asks you to approve it.",
        kind: "lines",
      },
    ],
  };
}

/**
 * **One page per profile** (ST-4, V91e): the profile's kind, command and environment, under its
 * name in the nav, beneath Harness & profiles. A name the core does not list as an entry (a file
 * that is not TOML) has no page.
 */
function profilePages(read: ProjectRead): SettingsGroup[] {
  const local: Shown = read.local;
  if (!local.parsed) return [];
  return (read.local.entries ?? [])
    .filter((one) => one.collection === "profiles")
    .map((one) => {
      const name = one.values.find((value) => value.field === "name")?.value ?? one.label;
      const settings = [
        textAt(key("harness", name, "kind"), `${name}: kind`, { kind: "choice", choices: KINDS }),
        listAt(
          key("harness", name, "command"),
          `${name}: command`,
          "One argument per line, program first. No shell runs it.",
        ),
        { ...envAt(name), label: `${name}: environment` },
      ].map((control) => setting(profilePage(name), "local", control, namesOf(read)));
      return {
        id: profilePage(name),
        label: one.label,
        help: `The profile ${one.label}: what it runs and with what environment, on this machine only. A changed command asks you to approve it before its next run.`,
        settings,
        collection: profilesCollection(read.local, { id: one.id, settings }),
        sub: true,
      };
    });
}

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
  const names = namesOf(read);
  // The Sandbox's pages are written with the names in use, and quote hosts a person wrote:
  // nothing in them is renamed (#1340).
  return declaredGroups(read).map((group) =>
    WRITTEN_NAMED.has(group.id) ? group : inFiles(group, names),
  );
}

/** The groups whose text already names the files in use. */
const WRITTEN_NAMED = new Set(["project.sandbox", MY_HOSTS, GRANTED]);

/**
 * `text`, naming the project's two files as the project names them (#1340): the level's
 * sentences are written with `charter.toml` and `charter.local.toml`, and a project renamed to
 * purlis's names reads `purlis.toml` and `purlis.local.toml` in each of them.
 */
export function namedIn(text: string, names: Readonly<Record<SettingsWhich, string>>): string {
  // The local name first: the committed name is not part of it.
  return wholeName(
    wholeName(text, "charter.local.toml", names.local),
    "charter.toml",
    names.shared,
  );
}

/**
 * `text` with each `old` that stands as a whole file name replaced by `now`: one no name
 * character runs into on either side, so a host or a path that only holds it
 * (`charter.toml.example.com`, `mycharter.toml`) is quoted as written. A sentence's full stop
 * after it still ends the name, and a folder before it is the file's own. As the core's
 * `settings::named_as` does.
 */
function wholeName(text: string, old: string, now: string): string {
  const part = /[A-Za-z0-9_-]/;
  return text.split(old).reduce((out, piece, at, pieces) => {
    if (at === 0) return piece;
    const before = pieces[at - 1];
    const led = before.length > 0 && /[A-Za-z0-9_.-]/.test(before[before.length - 1]);
    const runsOn =
      piece.length > 0 &&
      (piece[0] === "." ? piece.length > 1 && part.test(piece[1]) : part.test(piece[0]));
    return `${out}${led || runsOn ? old : now}${piece}`;
  }, "");
}

/** `group`, with every sentence it shows naming the files as the project names them. */
function inFiles(
  group: SettingsGroup,
  names: Readonly<Record<SettingsWhich, string>>,
): SettingsGroup {
  const said = (text: string) => namedIn(text, names);
  return {
    ...group,
    help: said(group.help),
    notes: group.notes?.map(said),
    settings: group.settings.map((one) => ({
      ...one,
      label: said(one.label),
      help: said(one.help),
      ...("unset" in one && one.unset !== undefined ? { unset: said(one.unset) } : {}),
    })),
    after: group.after?.map((one) => ({ ...one, help: said(one.help) })),
    ...(group.collection
      ? {
          collection: {
            ...group.collection,
            fields: group.collection.fields.map((one) => ({ ...one, help: said(one.help) })),
          },
        }
      : {}),
  };
}

/** The level's groups, as declared: {@link projectGroups} names the files in them. */
function declaredGroups(read: ProjectRead): SettingsGroup[] {
  const shared: Shown = read.shared;
  const local: Shown = read.local;
  const sharedOk = shared.parsed;
  const localOk = local.parsed;
  const [general, forges] = SHARED;
  const [harness] = LOCAL;
  const fromShared = (group: string, controls: Control[]) =>
    sharedOk
      ? controls.map((one) =>
          picking(setting(group, "shared", one, namesOf(read)), shared, read.entries),
        )
      : [];
  const fromLocal = (group: string, controls: Control[]) =>
    localOk
      ? controls.map((one) =>
          picking(setting(group, "local", one, namesOf(read)), local, read.entries),
        )
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
          listAt(
            key("chat_env", "pass"),
            "Environment passed to chats",
            "More of this machine's environment every chat here is given: a variable's name per line, or a prefix ending in *. A credential's name passes only whole.",
          ),
        ]),
      ],
      ...(localOk ? { collection: profilesCollection(read.local) } : {}),
    },
    ...profilePages(read),
    {
      id: "project.sandbox",
      label: "Sandbox",
      // The sentence at the top (#1340): what a chat here can do, as the files stand now.
      help: sandboxSentence(read.shared, read.sandbox, onAMac()),
      notes: sandboxNotes(read.shared, read.sandbox),
      // The mode, and any value an administrator's policy locks (#1343), is a status line:
      // nothing here takes it back, moves it or resets it.
      settings: fromShared("project.sandbox", sandboxControls(read.sandbox)).map((one) =>
        same(one.key, SANDBOX_MODE) || one.kind === "status" ? { ...one, oneWay: true } : one,
      ),
      ...(sharedOk ? { collection: hostsCollection(read.shared, "hosts") } : {}),
      after: sharedOk ? sandboxReasons(read.shared, read.local, read.sandbox) : [],
    },
    ...(localOk
      ? [
          read.sandbox?.policy?.personal_hosts === true
            ? {
                // Locked by policy (#1343): said, with who set it, and no Add or Remove.
                id: MY_HOSTS,
                label: "Your hosts",
                help: `Hosts your chats here would reach besides the project's, on this machine only.`,
                settings: [
                  lockedRow(
                    `${MY_HOSTS}.locked`,
                    "Your hosts",
                    `Policy forbids hosts of your own, so none reaches a chat here. ${read.sandbox.policy.locked_by}`,
                  ),
                ],
                sub: true,
              }
            : {
                id: MY_HOSTS,
                label: "Your hosts",
                help: `Hosts your chats here reach besides the project's, on this machine only. Kept in ${read.local.file}.`,
                settings: [],
                collection: hostsCollection(read.local, "myHosts"),
                sub: true,
              },
        ]
      : []),
    ...(read.plane !== undefined
      ? [
          grantedGroup(
            read.plane,
            read.shared.file,
            read.sandbox?.policy?.write_grants === true
              ? `Policy forbids allowing a chat to write a folder. ${read.sandbox.policy.locked_by}`
              : null,
          ),
        ]
      : []),
    // Dispatch (#1439, #1437): the limits, the grants and the policy's locks. It reads the core
    // on its own, as the Granted list does.
    ...(read.plane !== undefined ? [dispatchGroup(read.plane, read.shared.file)] : []),
    {
      id: "project.forges",
      label: "Forges",
      help: "Where the project's repos are found: one block per [[forge]] in charter.toml.",
      settings: forgeSettings,
      ...(sharedOk ? { collection: forgesCollection(read.shared, forgeSettings) } : {}),
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
      help: "Which harness plugins the chats purlis starts here have on.",
      settings: fromShared(
        "project.plugins",
        plugins.flatMap((one) => one.controls),
      ),
      notes: plugins.flatMap((one) => one.notes),
    },
  ];
}

/** Each collection of the level: the file it is kept in, and what one entry is called. */
const HOMES: Readonly<Record<string, { file: SettingsWhich; noun: string }>> = {
  forges: { file: "shared", noun: "forge" },
  profiles: { file: "local", noun: "profile" },
  hosts: { file: "shared", noun: "host" },
  myHosts: { file: "local", noun: "host" },
};

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
  const [picked, setPicked] = useState<Partial<Entries>>({});
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
            setPicked((was) => ({ ...was, profile, persona: options.personas }));
          }
        }),
        settled(commands.planeSidebar(plane)).then((said) => {
          const sidebar = said.status === "ok" ? said.data : undefined;
          if (sidebar) {
            const workspace = sidebar.workspaces.map((one) => one.name);
            setPicked((was) => ({ ...was, workspace }));
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

  /**
   * An add, a remove or a rename in one of the level's collections, through the core's function
   * for it, against the text the entries were drawn from (`op.base`). A written one answers what
   * it did, in the core's label, and what undoes it (D-ST3-i, as amended): a remove of what was
   * added, the text a remove was made against written back exactly, and a rename back (ST-4).
   */
  const entry = (op: EntryOp, both: ProjectFiles): Promise<EntryWrote<ProjectFiles>> => {
    const refused = (reasons: string[]) => ({ refused: { fields: {}, referrers: [], reasons } });
    const home = HOMES[op.collection];
    if (home === undefined)
      return Promise.resolve(refused([`No collection is called ${op.collection}.`]));
    const hosts = op.collection === "hosts" || op.collection === "myHosts";
    const asked = hosts
      ? "add" in op
        ? commands.addSandboxHost(plane, home.file, op.base, op.add.host ?? "")
        : "remove" in op
          ? commands.removeSandboxHost(plane, home.file, op.base, op.remove)
          : "confirm" in op
            ? commands.confirmSandboxHost(plane, op.base, op.confirm)
            : undefined
      : op.collection === "forges"
        ? "add" in op
          ? commands.addProjectForge(plane, op.base, {
              kind: op.add.kind ?? "",
              owner: op.add.owner ?? "",
              host: op.add.host ?? "",
              exclude: entries(op.add.exclude ?? ""),
            })
          : "remove" in op
            ? commands.removeProjectForge(plane, op.base, op.remove)
            : undefined
        : "add" in op
          ? commands.addProjectProfile(plane, op.base, {
              name: op.add.name ?? "",
              kind: op.add.kind ?? "",
              command: entries(op.add.command ?? ""),
            })
          : "remove" in op
            ? commands.removeProjectProfile(plane, op.base, op.remove)
            : "rename" in op
              ? commands.renameProjectProfile(plane, op.base, op.rename, op.to)
              : undefined;
    if (asked === undefined)
      return Promise.resolve(refused([`A ${home.noun} is not changed that way here.`]));
    return asked.then((said): EntryWrote<ProjectFiles> => {
      // A host confirmed as yours is kept in the project's state folder, which no watcher
      // reports: the Notice for chats left on an older sandbox asks again (#1428).
      if (hosts) sandboxCommandReturned();
      if (said.status === "error") return refused([said.error]);
      const answer = said.data;
      if (answer.kind === "refused") {
        const fields: Record<string, string[]> = {};
        for (const one of answer.fields) (fields[one.field] ??= []).push(one.why);
        return {
          refused: { fields, referrers: answer.referrers, reasons: answer.reasons },
        };
      }
      const file = answer.file;
      const after = file.exists ? file.text : null;
      const which = home.file;
      const saved = { ...both, [which]: file };
      const entryIn = (id: string, in_: SettingsFile) =>
        (in_.entries ?? []).find((one) => one.id === id);
      const labelOf = (id: string, in_: SettingsFile) =>
        entryIn(id, in_)?.label ?? `the ${home.noun}`;
      if ("add" in op)
        return {
          saved,
          file: which,
          said: `Added ${labelOf(answer.added ?? "", file)}.`,
          undo: { collection: op.collection, base: after, remove: answer.added ?? "" },
        };
      if ("confirm" in op)
        return {
          saved,
          file: which,
          said: `Confirmed ${labelOf(answer.added ?? "", file)}.`,
          undo: undefined,
        };
      if ("rename" in op) {
        const was = entryIn(op.rename, both[which]);
        const name = was?.values.find((one) => one.field === "name")?.value ?? "";
        return {
          saved,
          file: which,
          said: `Renamed ${was?.label ?? `the ${home.noun}`} to ${labelOf(answer.added ?? "", file)}.`,
          undo: { collection: op.collection, base: after, rename: answer.added ?? "", to: name },
        };
      }
      return {
        saved,
        file: which,
        said: `Removed ${labelOf("remove" in op ? op.remove : "", both[which])}.`,
        // Exact: the text the entry was removed from, against the text the remove left.
        undo: { restore: { file: which, base: after, text: op.base ?? "" } },
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
    read: { ...driver.now, extensions, harnesses, theme, saving, sandbox, entries: picked, plane },
    readEntries,
  };
}
