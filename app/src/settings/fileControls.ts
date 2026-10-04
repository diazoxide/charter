import { BUILT_IN, SYSTEM } from "../theme/theme";
import { hueOf, PALETTE } from "../theme/tint";
import type {
  HarnessPlugin,
  HarnessPlugins,
  InForce,
  ProjectExtension,
  ProjectExtensions,
  ProjectTheme,
  SavingInForce,
  SettingsEdit,
  SettingsFile,
  SettingsStep,
  SettingsValue,
} from "../bindings";

/**
 * **What a project's settings files hold, as controls** (charter-app#252): each key a form can
 * write, how it reads its value out of the file, and the edits a new value makes. Declared once
 * and drawn twice until SE-19 retires the old page: by Project settings' two sections
 * (`ProjectSettings.tsx`) and by the Settings tab's Project level (`project.ts`, SE-17), which
 * regroups the same controls into V89h's groups.
 */
/**
 * One control: a label, how it is drawn, how it reads its value out of the file, and the edits a
 * new value makes. Every value a control holds is text while it is being typed; `edits` is where
 * it becomes a key.
 */
/** What a section shows of a file: a project's settings file, or a workspace's manifest. */
export type Shown = Pick<
  SettingsFile,
  "file" | "exists" | "text" | "refusals" | "parsed" | "fields"
>;

export type Control = {
  id: string;
  label: string;
  hint?: string;
  /** `text` is one line, `choice` a closed set, `lines` one entry per line, and `colour` a
   *  closed set whose `custom` pick is a `#rrggbb` of the operator's own (charter-app#281). */
  kind: "text" | "choice" | "lines" | "colour";
  choices?: readonly string[];
  /** What a `choice`'s empty option says. */
  unset?: string;
  /** What a `choice` shows for each of its values, when that is not the value itself. */
  labels?: Readonly<Record<string, string>>;
  read: (file: Shown) => string;
  /** The edits `draft` makes to `file`, the file it was typed over. */
  edits: (draft: string, file: Shown) => SettingsEdit[];
};

/** What `project_extensions` answers before it has answered, or when it could not. */
export const NO_EXTENSIONS: ProjectExtensions = { extensions: [], local_left_out: null };

/** A heading and what is under it. `extensions` is what the core says is in force in this
 *  project, and `theme` what it says the project draws, for the groups that draw them. */
export type Group = {
  title: string;
  note?: string;
  controls: (
    file: Shown,
    extensions: readonly ProjectExtension[],
    theme: ProjectTheme | undefined,
    saving: Saving,
  ) => Control[];
  /** Sentences about this file the group says under its heading. */
  notes?: (
    file: Shown,
    extensions: readonly ProjectExtension[],
    theme: ProjectTheme | undefined,
    saving: Saving,
  ) => string[];
  /** For a group that shows what is in force: why `charter.local.toml` had no say in it, as the
   *  core's answer carries it (charter-app#319), or `null` when it had its say. */
  leftOut?: (
    extensions: ProjectExtensions,
    theme: ProjectTheme | undefined,
    saving: Saving,
  ) => string | null;
  /** What the group says when it has no controls; "None in this file." otherwise. */
  empty?: string | ((saving: Saving) => string);
};

/** What `project_saving_in_force` answered: nothing yet, the answer, or why it could not. */
export type Saving = SavingInForce | { trouble: string } | undefined;

/** The answer in `saving`, when there is one. */
export function answered(saving: Saving): SavingInForce | undefined {
  return saving === undefined || "trouble" in saving ? undefined : saving;
}

export const key = (...keys: string[]): SettingsStep[] => keys.map((one) => ({ key: one }));

function same(a: readonly SettingsStep[], b: readonly SettingsStep[]): boolean {
  return (
    a.length === b.length && a.every((step, at) => JSON.stringify(step) === JSON.stringify(b[at]))
  );
}

export function valueAt(file: Shown, path: readonly SettingsStep[]): SettingsValue | undefined {
  return file.fields.find((field) => same(field.path, path))?.value;
}

/** A value as one line of text: what a text box or a choice shows. */
export function shown(value: SettingsValue | undefined): string {
  if (value === undefined) return "";
  switch (value.kind) {
    case "text":
    case "other":
      return value.value;
    case "list":
      return value.value.join("\n");
    case "bool":
    case "integer":
      return String(value.value);
  }
}

/** One key holding one piece of text: an empty box removes the key. */
export function textAt(
  path: SettingsStep[],
  label: string,
  more: Partial<Pick<Control, "hint" | "kind" | "choices">> = {},
): Control {
  return {
    id: JSON.stringify(path),
    label,
    kind: "text",
    ...more,
    read: (file) => shown(valueAt(file, path)),
    edits: (draft) => [
      {
        path,
        value: draft.trim() === "" ? null : { kind: "text", value: draft.trim() },
      },
    ],
  };
}

/** One key holding a list of text, one entry per line. An empty box removes the key. */
export function listAt(path: SettingsStep[], label: string, hint?: string): Control {
  return {
    id: JSON.stringify(path),
    label,
    hint,
    kind: "lines",
    read: (file) => shown(valueAt(file, path)),
    edits: (draft) => {
      const items = entries(draft);
      return [{ path, value: items.length === 0 ? null : { kind: "list", value: items } }];
    },
  };
}

export function entries(draft: string): string[] {
  return draft
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");
}

/** The places `[[forge]]` blocks are at, and the names `[harness.<name>]` tables have. */
export function forgeBlocks(file: Shown): number[] {
  const at = new Set<number>();
  for (const { path } of file.fields) {
    const [first, second] = path;
    if (first?.key === "forge" && second?.index !== undefined) at.add(second.index);
  }
  return [...at];
}

export function profiles(file: Shown): string[] {
  const names = new Set<string>();
  for (const { path } of file.fields) {
    const [first, second] = path;
    if (first?.key === "harness" && second?.key !== undefined && path.length > 2)
      names.add(second.key);
  }
  return [...names];
}

/** A profile's `env` as `NAME=value` lines — one key per variable, so a line removed is a key
 *  removed and the others are left exactly as they were written. */
export function envAt(name: string): Control {
  const env = key("harness", name, "env");
  const pairs = (file: Shown) =>
    file.fields
      .filter((field) => field.path.length === 4 && same(field.path.slice(0, 3), env))
      .map((field) => [field.path[3].key ?? "", shown(field.value)] as const);
  return {
    id: JSON.stringify(env),
    label: "Environment",
    hint: "NAME=value, one per line. A credential does not belong here: log in inside the harness.",
    kind: "lines",
    read: (file) =>
      pairs(file)
        .map(([n, v]) => `${n}=${v}`)
        .join("\n"),
    edits: (draft, file) => {
      const before = pairs(file);
      const after = new Map(
        entries(draft).map((line) => {
          const cut = line.indexOf("=");
          return cut < 0 ? [line, ""] : [line.slice(0, cut).trim(), line.slice(cut + 1).trim()];
        }),
      );
      const gone: SettingsEdit[] = before
        .filter(([n]) => !after.has(n))
        .map(([n]) => ({ path: [...env, { key: n }], value: null }));
      const set: SettingsEdit[] = [...after]
        .filter(([n, v]) => before.find(([was]) => was === n)?.[1] !== v)
        .map(([n, v]) => ({ path: [...env, { key: n }], value: { kind: "text", value: v } }));
      return [...gone, ...set];
    },
  };
}

/** Where a source is, as the end of a sentence: a file, or this workspace. */
export function where(source: string): string {
  if (source === "local") return "charter.local.toml";
  if (source === "workspace") return "this workspace";
  return "charter.toml";
}

/** What an extension is in this project, and why, in a sentence after its name. */
function standing(it: ProjectExtension): string {
  const file = where(it.source);
  switch (it.state) {
    case "on":
      return it.source === "default"
        ? "on — installed and approved on this machine"
        : `on — enabled in ${file}`;
    case "off":
      return `off — turned off in ${file}`;
    case "needs-approval":
      return `needs approval here — ${it.source === "default" ? "installed" : `enabled in ${file}`}, and this machine has not approved it. Approve it in Extensions.`;
    default:
      return `not installed here — named in ${file}; install it from Extensions to use it`;
  }
}

/** One key holding true or false, as a closed choice: on, off, or not set here. */
export function onOffAt(path: SettingsStep[], label: string, hint: string, unset: string): Control {
  return {
    id: JSON.stringify(path),
    label,
    hint,
    kind: "choice",
    choices: ["on", "off"],
    unset,
    read: (file) => {
      const value = valueAt(file, path);
      if (value?.kind !== "bool") return shown(value);
      return value.value ? "on" : "off";
    },
    edits: (draft) => [
      {
        path,
        value: draft === "" ? null : { kind: "bool", value: draft === "on" },
      },
    ],
  };
}

/** Where a resolved setting came from, as the end of a sentence. */
export function from(source: string): string {
  return source === "default" ? "its default" : `from ${where(source)}`;
}

/**
 * **Extensions, in either section** (charter-app#253, ADR 0048). A project turns an extension
 * this machine has installed on or off, and sets what it declares; Local overrides Shared key by
 * key, and neither overrides this machine's approval. The sentence under each is the core's
 * answer for the project as both files stand — which the toggle above it may be about to change.
 */
export const EXTENSIONS: Group = {
  title: "Extensions",
  empty: "No extension is installed on this machine or named by this project.",
  controls: (_file, extensions) =>
    extensions.flatMap((it) => {
      const at = (...rest: string[]) => key("extensions", it.id, ...rest);
      return [
        onOffAt(
          at("enabled"),
          `${it.name}: enabled`,
          `${it.name}: ${standing(it)}`,
          "not set — inherits",
        ),
        ...it.settings.map((setting): Control => {
          const label = `${it.name}: ${setting.title}`;
          const hint = `In this project: ${setting.kind === "bool" ? (setting.value === "true" ? "on" : "off") : setting.value || "empty"}, ${from(setting.source)}.`;
          const path = at("settings", setting.key);
          if (setting.kind === "bool") return onOffAt(path, label, hint, "not set");
          if (setting.kind === "choice")
            return textAt(path, label, { hint, kind: "choice", choices: setting.choices });
          return textAt(path, label, { hint });
        }),
      ];
    }),
  notes: (file, extensions) =>
    extensions.flatMap((it) =>
      it.ignored.filter((one) => one.file === file.file).map((one) => one.why),
    ),
  leftOut: (extensions) => extensions.local_left_out,
};

/** The file a harness plugin's source is, by its own name: the layer that decided it. */
function pluginFile(source: string): string {
  if (source === "local") return "charter.local.toml";
  if (source === "workspace") return "workspace.json";
  return "charter.toml";
}

/** What a harness plugin is in this project or workspace, and why, in a sentence under its
 *  control. */
function pluginStanding(it: HarnessPlugin, harness: string, scope: Scope): string {
  const file = pluginFile(it.source);
  if (!it.installed)
    return `not installed on this machine — named in ${file}, so no chat is handed it`;
  const where = it.origin === "" ? "" : ` Installed: ${it.origin}.`;
  if (it.state === "not-set") return `not set — ${harness} decides, from its own settings.${where}`;
  return `${it.state} in this ${scope} — from ${file}.${where}`;
}

/** Whose settings a section is: the project's two files, or one workspace's (charter-app#282). */
export type Scope = "project" | "workspace";

/**
 * **Harness plugins, one group per harness, in every section** (charter-app#274, #282, ADR 0050).
 * Local overrides the workspace, which overrides Shared, plugin by plugin; not set leaves a
 * plugin to the harness. A plugin charter fixes is a line and not a control, and a harness whose
 * adapter cannot apply is its "not supported yet" sentence, with what it has installed listed
 * under it. In a workspace's section a toggle writes `settings.harness_plugins.<harness>` of its
 * `workspace.json` — the same path under `settings` as the files' table.
 */
export function harnessPluginGroups(harnesses: readonly HarnessPlugins[], scope: Scope): Group[] {
  return harnesses.map((harness) => {
    const chosen = harness.unsupported === null ? harness.plugins.filter((it) => !it.pinned) : [];
    return {
      title: `Harness plugins: ${harness.title}`,
      note: harness.unsupported ?? undefined,
      empty:
        harness.unsupported === null
          ? `${harness.title} has no plugin installed on this machine.`
          : "Nothing to choose here.",
      controls: () =>
        chosen.map((it) =>
          onOffAt(
            key("harness_plugins", harness.harness, it.id),
            `${harness.title}: ${it.id}`,
            pluginStanding(it, harness.title, scope),
            "not set",
          ),
        ),
      notes: (file) => [
        ...(harness.record === null
          ? []
          : [
              `Listed from ${harness.record}. A profile that points ${harness.title} at another directory is listed against that one when its chat starts.`,
            ]),
        ...(harness.trouble === null ? [] : [harness.trouble]),
        ...harness.plugins.flatMap((it) => (it.pinned === null ? [] : [it.pinned])),
        ...(harness.unsupported === null
          ? []
          : harness.plugins.map((it) => `Installed: ${it.id} (${it.origin})`)),
        ...harness.plugins.flatMap((it) =>
          it.ignored.filter((one) => one.file === file.file).map((one) => one.why),
        ),
      ],
      leftOut: () => harness.local_left_out,
    };
  });
}

/** charter's own picks, for when the core could not be asked what else there is. */
const BUILT_IN_PICKS = [...Object.keys(BUILT_IN), SYSTEM];

/**
 * **Theme, in either section** (charter-app#273, ADR 0048): `[theme] use`. Local's pick wins over
 * Shared's; an extension's theme is drawn only while the project has that extension on and this
 * machine approved it, and the core's sentence says why when the pick in force is not drawn —
 * under the section whose file made it.
 */
export function themeGroup(unset: string, here: "project" | "workspace" = "project"): Group {
  const path = key("theme", "use");
  return {
    title: "Theme",
    controls: (_file, _extensions, theme) => {
      const options = theme?.options ?? BUILT_IN_PICKS.map((value) => ({ value, label: value }));
      const labels = Object.fromEntries(options.map((one) => [one.value, one.label]));
      const drawn =
        theme?.draws == null
          ? `the window's own theme — your theme.json, else the first theme from an extension this ${here} has on, else charter-dark`
          : (labels[theme.draws] ?? theme.draws);
      // In a workspace, which of the three files picked the theme drawn here: its own, or the
      // project's that it did not override (charter-app#281). Not when the pick fell back: the
      // file picked something else, and the sentence under the group says what and why.
      const from =
        here === "workspace" && theme?.file != null && theme.why === null
          ? `, picked in ${theme.file}`
          : "";
      return [
        {
          ...textAt(path, "Theme", {
            kind: "choice",
            choices: options.map((one) => one.value),
            hint: `Drawn in this ${here}: ${drawn}${from}. The terminal follows the window.`,
          }),
          unset,
          labels,
        },
        ...(here === "workspace" ? [COLOUR] : []),
      ];
    },
    notes: (file, _extensions, theme) => {
      if (theme === undefined) return [];
      // A workspace's tab is one section, and what is drawn in the workspace is its answer
      // whichever file made the pick; a project's says it under the file that made it.
      const why = here === "workspace" || theme.file === file.file ? theme.why : null;
      return [
        ...(why === null ? [] : [why]),
        ...theme.ignored.filter((one) => one.file === file.file).map((one) => one.why),
        ...(here === "workspace" && theme.colour !== null && hueOf(theme.colour) === undefined
          ? [
              `${theme.colour} is a grey, which has no hue to tint with — so this workspace is drawn without a colour. Pick one of the eight, or a custom colour that is not grey.`,
            ]
          : []),
      ];
    },
    leftOut: (_extensions, theme) => theme?.local_left_out ?? null,
  };
}

/** Which of the project's two files a section is. */
export type Which = "shared" | "local";

/** The modes `planesave::Mode` reads, in the ladder's order. */
const MODES = ["off", "commit", "push", "pr", "pr-merge"] as const;

/** One save key the Plane or Repos group has a control for: where it is, what it is called,
 *  what it does, and what no value means when no file sets one. */
type SaveKey = {
  key: string;
  label: string;
  kind: "mode" | "text" | "bool";
  hint: string;
  /** What `null` in force means — no file set it, and none is the answer. */
  none?: string;
};

/**
 * What the project has for one save key, and which file decided it — the marker ADR 0048's
 * overlay asks of every place that shows a value — as the end of the control's hint. In the
 * Shared section, a value this file holds that `charter.local.toml` overrides says so.
 */
function decided(
  it: InForce,
  one: SaveKey,
  file: Shown,
  path: SettingsStep[],
  section: Which,
  fromShare = false,
): string {
  if (it.value === null && one.kind === "mode") return `In this project: ${one.none}.`;
  const value = it.value ?? one.none ?? "not set";
  const origin = fromShare
    ? "from [memory] share in charter.toml, the deprecated alias"
    : from(it.source);
  const overridden =
    section === "shared" && it.source === "local" && valueAt(file, path) !== undefined
      ? " — overriding the value here"
      : "";
  return `In this project: ${value}, ${origin}${overridden}.`;
}

/** One save key's control, writing to `path` in the section it is in. */
function saveControl(
  one: SaveKey,
  path: SettingsStep[],
  label: string,
  section: Which,
  marker: string | null,
): Control {
  const hint = marker === null ? one.hint : `${one.hint} ${marker}`;
  const unset = section === "local" ? "not set — charter.toml's" : "not set";
  if (one.kind === "bool") return onOffAt(path, label, hint, unset);
  if (one.kind === "mode")
    return { ...textAt(path, label, { hint, kind: "choice", choices: MODES }), unset };
  return textAt(path, label, { hint });
}

const QUIET_HINT = "A whole number of seconds or minutes, like 30s or 2m.";

/** `[plane]`'s save keys, in the order ADR 0051 and `docs/plane-format.md` list them. */
const PLANE_KEYS: readonly SaveKey[] = [
  {
    key: "mode",
    label: "Mode",
    kind: "mode",
    hint: "How far a save of the project goes: off, commit, push, pr (push to the save branch and keep one request open), or pr-merge (and set that request to auto-merge).",
    none: "not set — the Saving view asks once, before anything is pushed",
  },
  {
    key: "branch",
    label: "Target branch",
    kind: "text",
    hint: "The branch a save is meant to end up on.",
    none: "the branch the project has checked out",
  },
  {
    key: "save_branch",
    label: "Save branch",
    kind: "text",
    hint: "The one branch per machine that pr and pr-merge push to.",
    none: "charter/save/<this machine's name>",
  },
  { key: "sign", label: "Sign commits", kind: "bool", hint: "Sign the commits a save makes." },
  {
    key: "autosave",
    label: "Auto-save",
    kind: "bool",
    hint: "Save by itself: after a quiet period, when a session ends, and when the app quits.",
  },
  { key: "autosave_after", label: "Auto-save after", kind: "text", hint: QUIET_HINT },
];

/** `[repos.<name>]`'s keys: `[plane]`'s but `save_branch`, with a repo's defaults. */
const REPO_KEYS: readonly SaveKey[] = [
  {
    key: "mode",
    label: "mode",
    kind: "mode",
    hint: "How far a save of this repo goes: off, commit, push, pr or pr-merge.",
  },
  {
    key: "branch",
    label: "branch",
    kind: "text",
    hint: "The branch a request goes into.",
    none: "the repo's default branch",
  },
  { key: "sign", label: "sign", kind: "bool", hint: "Sign the commits a save makes." },
  { key: "autosave", label: "auto-save", kind: "bool", hint: "Save this repo by itself." },
  { key: "autosave_after", label: "auto-save after", kind: "text", hint: QUIET_HINT },
];

/**
 * **Plane, in either section** (charter-app#300, ADR 0051): `[plane]`'s save keys, each with what
 * the project has and the file that decided it. Shared's also holds `[memory] share`, the
 * deprecated alias of Mode that `docs/plane-format.md` still documents — the only file it is read
 * from.
 */
export function planeGroup(section: Which): Group {
  return {
    title: "Plane",
    note: "How the project is saved. charter.local.toml overrides charter.toml key by key. [plane] worktrees is under General.",
    controls: (file, _extensions, _theme, asked) => {
      const saving = answered(asked);
      return [
        ...PLANE_KEYS.map((one) => {
          const path = key("plane", one.key);
          const it = saving?.plane[one.key as keyof typeof saving.plane];
          const marker =
            it === undefined || typeof it === "boolean"
              ? null
              : decided(
                  it,
                  one,
                  file,
                  path,
                  section,
                  one.key === "mode" && saving?.plane.from_share === true,
                );
          return saveControl(one, path, one.label, section, marker);
        }),
        ...(section === "shared"
          ? [
              textAt(key("memory", "share"), "[memory] share (deprecated)", {
                kind: "choice",
                choices: ["local", "commit", "push"],
                hint: [SHARE_HINT, shareMarker(file, saving)].filter(Boolean).join(" "),
              }),
            ]
          : []),
      ];
    },
    notes: (_file, _extensions, _theme, saving) =>
      saving !== undefined && "trouble" in saving
        ? [`What this project uses could not be read: ${saving.trouble}`]
        : [],
    leftOut: (_extensions, _theme, saving) => answered(saving)?.plane_left_out ?? null,
  };
}

const SHARE_HINT =
  "Deprecated: read as Mode — commit and push carry over, local says nothing — only while neither file sets Mode. Set Mode instead.";

/**
 * Whether `[memory] share` is what the plane's mode is, as the end of its hint: ADR 0051's
 * marker for a Shared value something else overrides. Only a share that says something —
 * `commit` or `push` — can be in force or overridden; `local` says nothing either way.
 */
function shareMarker(file: Shown, saving: SavingInForce | undefined): string {
  if (saving === undefined) return "";
  if (saving.plane.from_share) return "In force as Mode.";
  const share = shown(valueAt(file, key("memory", "share")));
  if ((share === "commit" || share === "push") && saving.plane.mode.value !== null)
    return `Not in force — Mode from ${where(saving.plane.mode.source)} wins.`;
  return "";
}

/**
 * **Repos, in either section** (charter-app#300, ADR 0051): a row per repo — every one
 * `inventory/repos.json` catalogues, then every one only a file's `[repos]` names — with
 * `[repos.<name>]`'s keys, each marked as the Plane group's are.
 */
export function reposGroup(section: Which): Group {
  return {
    title: "Repos",
    note: "How each workspace repo is saved, by its name in inventory/repos.json. A repo's defaults are mode off and auto-save off: charter saves no repo until its mode says how.",
    empty: (saving) =>
      saving !== undefined && "trouble" in saving
        ? `The repos and how each is saved could not be read: ${saving.trouble}`
        : "No repo is catalogued in inventory/repos.json or named by either file.",
    controls: (file, _extensions, _theme, saving) =>
      (answered(saving)?.repos ?? []).flatMap((repo) =>
        REPO_KEYS.map((one) => {
          const path = key("repos", repo.name, one.key);
          const it = repo[one.key as keyof typeof repo];
          const marker = typeof it === "string" ? null : decided(it, one, file, path, section);
          return saveControl(one, path, `${repo.name}: ${one.label}`, section, marker);
        }),
      ),
    leftOut: (_extensions, _theme, saving) => answered(saving)?.repos_left_out ?? null,
  };
}

/** The harness kinds a profile may name — `profiles::KINDS`, in the registry's order. */
export const KINDS = ["claude", "opencode", "codex"] as const;

/** `charter.toml`, as `docs/plane-format.md` documents it. `[frame]` is the tmux frame's, which
 *  this charter does not have and nothing reads (the doctor says so): the raw view has it. */
export const SHARED: Group[] = [
  {
    title: "General",
    controls: () => [
      textAt(key("workspace", "default"), "Default workspace"),
      textAt(key("persona", "default"), "Default persona", {
        hint: "The persona a chat starts as when nothing else names one.",
      }),
      textAt(key("harness", "default"), "Default harness", {
        hint: "claude, opencode, codex, or a profile charter.local.toml declares.",
      }),
      textAt(key("update", "channel"), "Update channel", {
        kind: "choice",
        choices: ["stable", "dev"],
      }),
      textAt(key("charter", "version"), "Version lock", {
        hint: "The charter version this project is pinned to, as 1.2.3. Empty pins nothing.",
      }),
      textAt(key("plane", "worktrees"), "Worktrees folder", {
        hint: "Under the project or one folder beside it, such as ../charter.worktrees.",
      }),
    ],
  },
  {
    title: "Forges",
    note: "One block per [[forge]] in the file. Add or remove a block in the raw view.",
    controls: (file) =>
      forgeBlocks(file).flatMap((at) => {
        const block = (name: string): SettingsStep[] => [
          { key: "forge" },
          { index: at },
          { key: name },
        ];
        // `group` wins over `owner` when a block has both; edit the one the block uses.
        const owner = valueAt(file, block("group")) !== undefined ? "group" : "owner";
        const n = at + 1;
        return [
          textAt(block("kind"), `Forge ${n}: kind`, {
            kind: "choice",
            choices: ["gitlab", "github"],
          }),
          textAt(block(owner), `Forge ${n}: ${owner}`),
          textAt(block("host"), `Forge ${n}: host`, {
            hint: "A bare host, with a port if it needs one. Empty is the kind's own.",
          }),
          listAt(block("exclude"), `Forge ${n}: repos never listed`, "One repo name per line."),
        ];
      }),
  },
  planeGroup("shared"),
  reposGroup("shared"),
  EXTENSIONS,
  themeGroup("not set — the window's own theme"),
];

/** The pick that says a workspace's colour is its own `#rrggbb` rather than a palette name. */
const CUSTOM = "custom";

/**
 * **A workspace's colour** (charter-app#281): `settings.theme.colour`, one of the eight the core
 * reads (`theme/tint.ts`'s `PALETTE`, held to the core's by a test) or a custom `#rrggbb`, whose
 * hue is taken. A select, and a colour well beside it while the pick is custom.
 */
const COLOUR: Control = {
  ...textAt(key("theme", "colour"), "Colour", {
    kind: "colour",
    choices: [...Object.keys(PALETTE), CUSTOM],
    hint: "Tints this workspace's tab, its chat strip, the title bar's mark and the accent while it is in front. Text and the terminal keep the theme's own colours.",
  }),
  unset: "none — the theme as it is",
  labels: {
    ...Object.fromEntries(
      Object.keys(PALETTE).map((name) => [name, name[0].toUpperCase() + name.slice(1)]),
    ),
    [CUSTOM]: "Custom…",
  },
};

/** A workspace's `settings` (charter-app#280): what a workspace can set — its extensions, and
 *  its theme and colour (charter-app#281). */
export const WORKSPACE: Group[] = [
  EXTENSIONS,
  themeGroup("not set — the project's pick", "workspace"),
];

/** `charter.local.toml`: `[harness]`, `[plane]`'s save keys, `[repos]`, `[extensions]` and
 *  `[theme]`, which is all its readers read there. */
export const LOCAL: Group[] = [
  {
    title: "Harness",
    controls: () => [
      textAt(key("harness", "default"), "Default profile", {
        hint: "The profile the new-chat picker starts on. Wins over charter.toml's.",
      }),
    ],
  },
  {
    title: "Profiles",
    note: "One per [harness.<name>] table. Add or remove one in the raw view.",
    controls: (file) =>
      profiles(file).flatMap((name) => [
        textAt(key("harness", name, "kind"), `${name}: kind`, { kind: "choice", choices: KINDS }),
        listAt(
          key("harness", name, "command"),
          `${name}: command`,
          "One argument per line, program first. No shell runs it.",
        ),
        { ...envAt(name), label: `${name}: environment` },
      ]),
  },
  planeGroup("local"),
  reposGroup("local"),
  EXTENSIONS,
  themeGroup("not set — charter.toml's pick"),
];
