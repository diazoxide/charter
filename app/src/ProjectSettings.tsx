import { LiveDialog } from "./LiveDialog";
import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import * as RadioGroup from "@radix-ui/react-radio-group";
import { LoaderCircle } from "lucide-react";
import { extensionsChanged } from "./extensionsOn";
import { projectThemeChanged, useProjectThemeAnswers } from "./projectTheme";
import { BUILT_IN, DEFAULT_THEME, inForce, SYSTEM } from "./theme/theme";
import { hueOf, PALETTE } from "./theme/tint";
import { WorkspaceRepos } from "./WorkspaceRepos";
import {
  commands,
  type HarnessPlugin,
  type HarnessPlugins,
  type InForce,
  type PlaneId,
  type ProjectExtension,
  type ProjectExtensions,
  type ProjectTheme,
  type ProjectSettings as Both,
  type SavingInForce,
  type SettingsChange,
  type SettingsEdit,
  type SettingsFile,
  type SettingsStep,
  type SettingsValue,
  type WorkspaceSettings as OneWorkspace,
} from "./bindings";

/**
 * **Project settings** (charter-app#252): a plane's two settings files, as forms and as raw TOML,
 * in a view tab of their own (`tabs.SETTINGS_VIEW`).
 *
 * - **Shared** is `charter.toml`, committed: the team sees it.
 * - **Local** is `charter.local.toml`, gitignored: this machine only. Harness profiles live here.
 *   While git would carry it (tracked, or not ignored) charter reads nothing in it
 *   (charter-app#308, ADR 0048): the section still shows the file, and the reason and its fix
 *   are among its standing refusals, in the words the profiles loader says them in.
 *
 * **Nothing here decides what a file may say.** A form sends its changes, the raw view sends its
 * text, and the core checks either with the rules it reads the file with and writes it with
 * `toml_edit` (`charter_core::settings`), so comments and ordering survive. What comes back is
 * the file as it now stands, or every reason nothing was written, in the core's words — which is
 * all this draws.
 *
 * Each section is a list of {@link Group}s, and a group is data: a heading and the controls
 * under it, each reading and writing keys by path. **Extensions** (charter-app#253, ADR 0048) is
 * one group in either section: each extension with what it is in this project and which file
 * decided it — the core's `extension::project::resolve`, asked with `project_extensions` — and
 * a control per key that writes `[extensions.<id>]` in the section it is in. **Theme**
 * (charter-app#273) is one too: `[theme] use`, from charter's own, the system's, or an approved
 * extension's, with the core's sentence when the pick in force cannot be drawn
 * (`extension::project::theme`, asked with `project_theme`).
 *
 * **Harness plugins** (charter-app#274, ADR 0050) is one group per harness charter knows, in
 * either section: each plugin that harness has installed, on, off or not set, writing
 * `[harness_plugins.<harness>]` — the core's `harness_plugin::survey`, asked with
 * `project_harness_plugins`. A harness whose adapter cannot apply says so and has no control.
 *
 * **Plane** and **Repos** (charter-app#300, ADR 0051) are two more, in either section: `[plane]`'s
 * save keys, and `[repos.<name>]`'s for every repo `inventory/repos.json` catalogues or a file
 * names. Beside each control is what the project has — the core's `planesave::Settings`, asked
 * with `project_saving_in_force` — and the file that decided it; in the Shared section a value
 * this file holds that `charter.local.toml` overrides says so.
 *
 * **Each of those groups says it once when Local was left out having set something in it**
 * (charter-app#319): the value in force beside a control is not the one Local set, and the group
 * says why in the sentence the core's answer carries — the ignore check's, the one the Local
 * section says at its head, so there is one wording. The Local section's own groups leave it to
 * that head.
 */
export function ProjectSettings({ plane }: { plane: PlaneId }) {
  const [both, setBoth] = useState<Both | { trouble: string }>();
  const [extensions, setExtensions] = useState<ProjectExtensions>(NO_EXTENSIONS);
  const [harnesses, setHarnesses] = useState<HarnessPlugins[]>([]);
  const [theme, setTheme] = useState<ProjectTheme>();
  const [saving, setSaving] = useState<Saving>();
  /** The newest read out: an answer to an older one — before a save, or for another plane — is
   *  dropped rather than drawn over what came after it. */
  const reading = useRef(0);

  /** The newest theme read out, kept apart from {@link reading}: the theme is also read on its
   *  own, when the window's answer for this project changes. */
  const themeReading = useRef(0);
  // The theme, as the files are; a refusal leaves the Theme group with charter's own picks only.
  const readTheme = useCallback(() => {
    const mine = ++themeReading.current;
    const newest = () => themeReading.current === mine;
    void commands
      .projectTheme(plane, null)
      .then((said) => {
        if (newest()) setTheme(said.status === "ok" ? (said.data ?? undefined) : undefined);
      })
      .catch(() => {
        if (newest()) setTheme(undefined);
      });
  }, [plane]);

  const read = useCallback(() => {
    const mine = ++reading.current;
    const newest = () => reading.current === mine;
    void commands
      .projectSettings(plane)
      .then((said) => {
        if (newest()) setBoth(said.status === "ok" ? said.data : { trouble: said.error });
      })
      .catch((err: unknown) => {
        if (newest()) setBoth({ trouble: String(err) });
      });
    // What is in force is read with the files, so a save shows its effect. A refusal leaves the
    // list empty: the group then says there is nothing, and the files' forms still work.
    void commands
      .projectExtensions(plane, null)
      .then((said) => {
        if (newest())
          setExtensions(said.status === "ok" ? (said.data ?? NO_EXTENSIONS) : NO_EXTENSIONS);
      })
      .catch(() => {
        if (newest()) setExtensions(NO_EXTENSIONS);
      });
    void commands
      .projectHarnessPlugins(plane, null)
      .then((said) => {
        if (newest()) setHarnesses(said.status === "ok" ? (said.data ?? []) : []);
      })
      .catch(() => {
        if (newest()) setHarnesses([]);
      });
    // How far a save goes, as the files decide it. A refusal leaves each control without the
    // sentence beside it, and the Plane and Repos groups say why; the files' forms still work.
    void commands
      .projectSavingInForce(plane)
      .then((said) => {
        if (newest())
          setSaving(said.status === "ok" ? (said.data ?? undefined) : { trouble: said.error });
      })
      .catch((err: unknown) => {
        if (newest()) setSaving({ trouble: String(err) });
      });
    readTheme();
  }, [plane, readTheme]);

  useEffect(read, [read]);
  // What the window draws for this project was asked again — by this tab's save, or by an
  // approval or a removal in the Extensions dialog, which tells the window and not this tab —
  // so the tab's sentence about the theme is asked again with it, and the two never disagree.
  const answers = useProjectThemeAnswers(plane);
  useEffect(() => {
    if (answers > 0) readTheme();
  }, [answers, readTheme]);

  const saved = useCallback(() => {
    read();
    // The window keeps of its surveyed panels, views and themes what this project has on.
    extensionsChanged(plane);
    // And the theme it draws while it is in front (charter-app#273).
    projectThemeChanged(plane);
  }, [plane, read]);

  if (both === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the settings…
      </p>
    );
  }
  if ("trouble" in both) {
    return (
      <p className="trouble" role="alert">
        {both.trouble}
      </p>
    );
  }
  return (
    <div className="settings">
      <p className="note">
        Never put a secret in either file. Keep it in a vault and name it where it is needed as{" "}
        <code>vault:&lt;vault&gt;/&lt;key&gt;</code>; charter refuses a value that looks like a
        credential.
      </p>
      <Section
        file={both.shared}
        testid="settings-shared"
        title="Shared"
        who="Committed; your team sees this."
        groups={[...SHARED, ...harnessPluginGroups(harnesses, "project")]}
        extensions={extensions}
        theme={theme}
        saving={saving}
        send={(base, change) => commands.saveProjectSettings(plane, "shared", base, change)}
        onSaved={saved}
      />
      <Section
        file={both.local}
        testid="settings-local"
        title="Local"
        who="This machine only. Gitignored; charter neither reads nor writes it where git would commit it."
        groups={[...LOCAL, ...harnessPluginGroups(harnesses, "project")]}
        extensions={extensions}
        theme={theme}
        saving={saving}
        // Its head already says why the file is not read, among its refusals.
        groupsSayLeftOut={false}
        send={(base, change) => commands.saveProjectSettings(plane, "local", base, change)}
        onSaved={saved}
      />
    </div>
  );
}

/**
 * **Workspace settings** (charter-app#280, ADR 0048): the `settings` of one workspace's
 * `workspace.json`, in a view tab of its own (`tabs.workspaceSettingsView`).
 *
 * The layer between the project's two files: `charter.toml`, then this workspace, then
 * `charter.local.toml`. A workspace refines its project for the team, and this machine's Local
 * file still has the last word; none of the three reaches past this machine's approval. It holds
 * the groups a workspace can set: Extensions — the same group as Project settings', asked with
 * `project_extensions` for this workspace — Harness plugins, one group per harness
 * (charter-app#282), asked with `project_harness_plugins` for this workspace, and **Theme**
 * (charter-app#281): the workspace's pick, read by the same resolver as the project's
 * (`project_theme` for this workspace), and its **colour**. Each extension, each plugin and the
 * theme says which layer decided it — and, when Local was left out, each group says why, once
 * (charter-app#319), as Project settings' do. And its **Repos** (ADR 0055): what is cloned in
 * it, added to and taken from with the new-workspace dialog's picker. A form only: the manifest holds more than settings,
 * and charter keeps the rest.
 */
export function WorkspaceSettings({ plane, workspace }: { plane: PlaneId; workspace: string }) {
  const [file, setFile] = useState<OneWorkspace | { trouble: string }>();
  /** Whether the LIVE/LOCAL confirmation is open (charter-app#301). */
  const [switching, setSwitching] = useState(false);
  const [extensions, setExtensions] = useState<ProjectExtensions>(NO_EXTENSIONS);
  const [theme, setTheme] = useState<ProjectTheme>();
  const [harnesses, setHarnesses] = useState<HarnessPlugins[]>([]);
  /** The newest read out, as in {@link ProjectSettings}. */
  const reading = useRef(0);
  /** And the newest theme read out, which is also read on its own, as there. */
  const themeReading = useRef(0);
  const readTheme = useCallback(() => {
    const mine = ++themeReading.current;
    const newest = () => themeReading.current === mine;
    void commands
      .projectTheme(plane, workspace)
      .then((said) => {
        if (newest()) setTheme(said.status === "ok" ? (said.data ?? undefined) : undefined);
      })
      .catch(() => {
        if (newest()) setTheme(undefined);
      });
  }, [plane, workspace]);

  const read = useCallback(() => {
    const mine = ++reading.current;
    const newest = () => reading.current === mine;
    void commands
      .workspaceSettings(plane, workspace)
      .then((said) => {
        if (newest()) setFile(said.status === "ok" ? said.data : { trouble: said.error });
      })
      .catch((err: unknown) => {
        if (newest()) setFile({ trouble: String(err) });
      });
    void commands
      .projectExtensions(plane, workspace)
      .then((said) => {
        if (newest())
          setExtensions(said.status === "ok" ? (said.data ?? NO_EXTENSIONS) : NO_EXTENSIONS);
      })
      .catch(() => {
        if (newest()) setExtensions(NO_EXTENSIONS);
      });
    void commands
      .projectHarnessPlugins(plane, workspace)
      .then((said) => {
        if (newest()) setHarnesses(said.status === "ok" ? (said.data ?? []) : []);
      })
      .catch(() => {
        if (newest()) setHarnesses([]);
      });
    readTheme();
  }, [plane, workspace, readTheme]);

  useEffect(read, [read]);
  // What the window draws here was asked again — by a save, or by an approval in the Extensions
  // dialog — so the sentence about it is asked again with it, as Project settings' is.
  const answers = useProjectThemeAnswers(plane, workspace);
  useEffect(() => {
    if (answers > 0) readTheme();
  }, [answers, readTheme]);

  const saved = useCallback(() => {
    read();
    // What the window keeps of its surveyed panels and views for this workspace.
    extensionsChanged(plane);
    // And the theme and colour it draws while this workspace is in front (charter-app#281).
    projectThemeChanged(plane);
  }, [plane, read]);

  if (file === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the settings…
      </p>
    );
  }
  if ("trouble" in file) {
    return (
      <p className="trouble" role="alert">
        {file.trouble}
      </p>
    );
  }
  return (
    <div className="settings">
      <p className="note">
        Read in this order: charter.toml, then this workspace, then charter.local.toml — the
        workspace refines its project for the team, and this machine has the last word. Never put a
        secret here: keep it in a vault and name it as <code>vault:&lt;vault&gt;/&lt;key&gt;</code>.
      </p>
      {/* LIVE or LOCAL (charter-app#301): the same confirmation as the workspace's menu row. */}
      <fieldset className="settings-group" aria-label="Live">
        <legend>Live</legend>
        <p className="settings-who">
          {file.live
            ? "LIVE: its charter, memory and todos are published with the plane."
            : "LOCAL: its charter, memory and todos stay on this machine."}
        </p>
        <div className="settings-actions">
          <button
            type="button"
            className="panel-view"
            tabIndex={0}
            onClick={() => setSwitching(true)}
          >
            {file.live ? "Make local…" : "Make live…"}
          </button>
        </div>
      </fieldset>
      <WorkspaceRepos plane={plane} workspace={workspace} />
      {switching && (
        <LiveDialog
          plane={plane}
          workspace={workspace}
          onClose={() => setSwitching(false)}
          onDone={() => {
            setSwitching(false);
            saved();
          }}
        />
      )}
      <Section
        file={file}
        testid="settings-workspace"
        title="Workspace"
        who={
          file.live
            ? "Committed with this LIVE workspace; your team sees this."
            : "This workspace is not LIVE, so its workspace.json stays on this machine."
        }
        groups={[...WORKSPACE, ...harnessPluginGroups(harnesses, "workspace")]}
        extensions={extensions}
        theme={theme}
        rawView={false}
        send={(base, change) =>
          commands
            .saveWorkspaceSettings(
              plane,
              workspace,
              base,
              change.kind === "edits" ? change.edits : [],
            )
            .then((said) =>
              said.status === "error"
                ? said
                : {
                    status: "ok" as const,
                    data: said.data.kind === "refused" ? said.data : { kind: "saved" as const },
                  },
            )
        }
        onSaved={saved}
      />
    </div>
  );
}

// ------------------------------------------------------------------------------------------
// what the forms cover
// ------------------------------------------------------------------------------------------

/**
 * One control: a label, how it is drawn, how it reads its value out of the file, and the edits a
 * new value makes. Every value a control holds is text while it is being typed; `edits` is where
 * it becomes a key.
 */
/** What a section shows of a file: a project's settings file, or a workspace's manifest. */
type Shown = Pick<SettingsFile, "file" | "exists" | "text" | "refusals" | "parsed" | "fields">;

type Control = {
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
const NO_EXTENSIONS: ProjectExtensions = { extensions: [], local_left_out: null };

/** A heading and what is under it. `extensions` is what the core says is in force in this
 *  project, and `theme` what it says the project draws, for the groups that draw them. */
type Group = {
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
type Saving = SavingInForce | { trouble: string } | undefined;

/** The answer in `saving`, when there is one. */
function answered(saving: Saving): SavingInForce | undefined {
  return saving === undefined || "trouble" in saving ? undefined : saving;
}

const key = (...keys: string[]): SettingsStep[] => keys.map((one) => ({ key: one }));

function same(a: readonly SettingsStep[], b: readonly SettingsStep[]): boolean {
  return (
    a.length === b.length && a.every((step, at) => JSON.stringify(step) === JSON.stringify(b[at]))
  );
}

function valueAt(file: Shown, path: readonly SettingsStep[]): SettingsValue | undefined {
  return file.fields.find((field) => same(field.path, path))?.value;
}

/** A value as one line of text: what a text box or a choice shows. */
function shown(value: SettingsValue | undefined): string {
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
function textAt(
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
function listAt(path: SettingsStep[], label: string, hint?: string): Control {
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

function entries(draft: string): string[] {
  return draft
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");
}

/** The places `[[forge]]` blocks are at, and the names `[harness.<name>]` tables have. */
function forgeBlocks(file: Shown): number[] {
  const at = new Set<number>();
  for (const { path } of file.fields) {
    const [first, second] = path;
    if (first?.key === "forge" && second?.index !== undefined) at.add(second.index);
  }
  return [...at];
}

function profiles(file: Shown): string[] {
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
function envAt(name: string): Control {
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
function where(source: string): string {
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
function onOffAt(path: SettingsStep[], label: string, hint: string, unset: string): Control {
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
function from(source: string): string {
  return source === "default" ? "its default" : `from ${where(source)}`;
}

/**
 * **Extensions, in either section** (charter-app#253, ADR 0048). A project turns an extension
 * this machine has installed on or off, and sets what it declares; Local overrides Shared key by
 * key, and neither overrides this machine's approval. The sentence under each is the core's
 * answer for the project as both files stand — which the toggle above it may be about to change.
 */
const EXTENSIONS: Group = {
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
type Scope = "project" | "workspace";

/**
 * **Harness plugins, one group per harness, in every section** (charter-app#274, #282, ADR 0050).
 * Local overrides the workspace, which overrides Shared, plugin by plugin; not set leaves a
 * plugin to the harness. A plugin charter fixes is a line and not a control, and a harness whose
 * adapter cannot apply is its "not supported yet" sentence, with what it has installed listed
 * under it. In a workspace's section a toggle writes `settings.harness_plugins.<harness>` of its
 * `workspace.json` — the same path under `settings` as the files' table.
 */
function harnessPluginGroups(harnesses: readonly HarnessPlugins[], scope: Scope): Group[] {
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
function themeGroup(unset: string, here: "project" | "workspace" = "project"): Group {
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
type Which = "shared" | "local";

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
    hint: "How far a save of the plane goes: off, commit, push, pr (push to the save branch and keep one PR open), or pr-merge (and set that PR to auto-merge).",
    none: "not set — the Saving view asks once, before anything is pushed",
  },
  {
    key: "branch",
    label: "Target branch",
    kind: "text",
    hint: "The branch a save is meant to end up on.",
    none: "the branch the plane has checked out",
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
    hint: "The branch a PR goes into.",
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
function planeGroup(section: Which): Group {
  return {
    title: "Plane",
    note: "How the plane is saved. charter.local.toml overrides charter.toml key by key. [plane] worktrees is under General.",
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
function reposGroup(section: Which): Group {
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
const KINDS = ["claude", "opencode", "codex"] as const;

/** `charter.toml`, as `docs/plane-format.md` documents it. `[frame]` is the tmux frame's, which
 *  this charter does not have and nothing reads (the doctor says so): the raw view has it. */
const SHARED: Group[] = [
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
        hint: "The charter version this plane is pinned to, as 1.2.3. Empty pins nothing.",
      }),
      textAt(key("plane", "worktrees"), "Worktrees folder", {
        hint: "Under the plane or one folder beside it, such as ../charter.worktrees.",
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
const WORKSPACE: Group[] = [EXTENSIONS, themeGroup("not set — the project's pick", "workspace")];

/** `charter.local.toml`: `[harness]`, `[plane]`'s save keys, `[repos]`, `[extensions]` and
 *  `[theme]`, which is all its readers read there. */
const LOCAL: Group[] = [
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

// ------------------------------------------------------------------------------------------
// one file
// ------------------------------------------------------------------------------------------

type Mode = "form" | "raw";

/** What a save answered, as a section reads it. */
type Sent =
  | { status: "ok"; data: { kind: "saved" } | { kind: "refused"; reasons: string[] } }
  | { status: "error"; error: string };

function Section({
  file,
  testid,
  title,
  who,
  groups,
  extensions,
  theme,
  saving: inForce,
  send,
  onSaved,
  rawView = true,
  groupsSayLeftOut = true,
}: {
  file: Shown;
  testid: string;
  title: string;
  who: string;
  groups: readonly Group[];
  extensions: ProjectExtensions;
  theme: ProjectTheme | undefined;
  /** How far a save goes in this project, for the Plane and Repos groups; a workspace's tab has
   *  neither. */
  saving?: Saving;
  /** Sends a change against the text it was typed over: `null` for a file not there yet. */
  send: (base: string | null, change: SettingsChange) => Promise<Sent>;
  onSaved: () => void;
  /** Whether the file is also offered as raw TOML. A workspace's manifest is not (#280). */
  rawView?: boolean;
  /** Whether each group that shows what is in force says why Local was left out of it
   *  (charter-app#319). The Local section's head says it for its groups. */
  groupsSayLeftOut?: boolean;
}) {
  const heading = useId();
  const [mode, setMode] = useState<Mode>(file.parsed || !rawView ? "form" : "raw");
  /** What the operator has typed into a control, by its id, until it is saved or discarded. */
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [raw, setRaw] = useState(file.text);
  const [refused, setRefused] = useState<string[]>();
  const [saving, setSaving] = useState(false);

  // A file that reads differently — after a save of it — starts every draft over, during the
  // render that sees it (React's pattern for state derived from a prop). By the text, so saving
  // the OTHER file, which reads this one again unchanged, loses nothing typed here.
  // A file that no longer parses has no form to show, so it opens in the raw view.
  const [seen, setSeen] = useState(file.text);
  if (seen !== file.text) {
    setSeen(file.text);
    setDrafts({});
    setRaw(file.text);
    if (!file.parsed && rawView) setMode("raw");
  }

  const controls = groups.map((group) => ({
    group,
    controls: group.controls(file, extensions.extensions, theme, inForce),
    notes: group.notes?.(file, extensions.extensions, theme, inForce) ?? [],
    leftOut: groupsSayLeftOut ? (group.leftOut?.(extensions, theme, inForce) ?? null) : null,
  }));
  const all = controls.flatMap((one) => one.controls);
  const changed = all.filter((one) => one.id in drafts && drafts[one.id] !== one.read(file));
  const dirty = mode === "form" ? changed.length > 0 : raw !== file.text;
  /** The file's own name, which the save button says: `workspace.json`, not its path. */
  const named = file.file.split("/").pop() ?? file.file;

  const discard = () => {
    setDrafts({});
    setRaw(file.text);
    setRefused(undefined);
  };

  const save = async () => {
    setSaving(true);
    const said = await send(
      file.exists ? file.text : null,
      mode === "raw"
        ? { kind: "raw", text: raw }
        : { kind: "edits", edits: changed.flatMap((one) => one.edits(drafts[one.id], file)) },
    ).catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setSaving(false);
    if (said.status === "error") {
      setRefused([said.error]);
    } else if (said.data.kind === "refused") {
      setRefused(said.data.reasons);
    } else {
      setRefused(undefined);
      onSaved();
    }
  };

  return (
    <section className="settings-file" aria-labelledby={heading} data-testid={testid}>
      <header className="settings-head">
        <h3 id={heading}>{title}</h3>
        <p className="settings-who">
          <code>{file.file}</code> · {who}
          {!file.exists && " Not created yet: the first save creates it."}
        </p>
      </header>

      {file.refusals.length > 0 && (
        <Reasons
          className="settings-standing"
          lead="charter does not take this from the file as it stands:"
          reasons={file.refusals}
        />
      )}

      {rawView && (
        <RadioGroup.Root
          className="settings-mode"
          orientation="horizontal"
          value={mode}
          onValueChange={(to) => setMode(to as Mode)}
          aria-label={`How to edit ${file.file}`}
        >
          <ModeItem
            value="form"
            disabled={(mode === "raw" && dirty) || !file.parsed}
            onPick={setMode}
          >
            Form
          </ModeItem>
          <ModeItem value="raw" disabled={mode === "form" && dirty} onPick={setMode}>
            Raw TOML
          </ModeItem>
        </RadioGroup.Root>
      )}
      {rawView && dirty && (
        <p className="settings-hint">Save or discard these changes to switch views.</p>
      )}

      {mode === "form" ? (
        // A file no form can read, with no raw view to mend it in, shows only why (above).
        (file.parsed ? controls : []).map(({ group, controls: under, notes, leftOut }) => (
          <fieldset key={group.title} className="settings-group">
            <legend>{group.title}</legend>
            {group.note && <p className="settings-hint">{group.note}</p>}
            {leftOut !== null && <p className="settings-hint">{leftOut}</p>}
            {under.length === 0 && (
              <p className="none">
                {typeof group.empty === "function"
                  ? group.empty(inForce)
                  : (group.empty ?? "None in this file.")}
              </p>
            )}
            {under.map((control) => (
              <SettingControl
                key={control.id}
                control={control}
                value={drafts[control.id] ?? control.read(file)}
                onChange={(to) => setDrafts((was) => ({ ...was, [control.id]: to }))}
              />
            ))}
            {notes.map((why, at) => (
              <p key={at} className="settings-hint">
                {why}
              </p>
            ))}
          </fieldset>
        ))
      ) : (
        <label className="settings-raw">
          <span className="settings-hint">
            The whole file, comments and all. Anything no form covers is edited here.
          </span>
          <textarea
            value={raw}
            spellCheck={false}
            rows={Math.max(8, raw.split("\n").length + 1)}
            aria-label={`${file.file}, as TOML`}
            onChange={(event) => setRaw(event.target.value)}
          />
        </label>
      )}

      <div className="settings-actions">
        <button type="button" tabIndex={0} disabled={!dirty || saving} onClick={() => void save()}>
          {saving ? "Saving…" : `Save ${named}`}
        </button>
        <button type="button" tabIndex={0} disabled={!dirty || saving} onClick={discard}>
          Discard
        </button>
      </div>

      {refused && (
        <Reasons className="settings-refused" lead="Nothing was saved:" reasons={refused} alert />
      )}
    </section>
  );
}

/** Sentences from the core, each drawn as it said it. By position: two may be the same words. */
function Reasons({
  className,
  lead,
  reasons,
  alert = false,
}: {
  className: string;
  lead: string;
  reasons: readonly string[];
  alert?: boolean;
}) {
  return (
    <div className={className} role={alert ? "alert" : undefined}>
      <p className="note">{lead}</p>
      <ul>
        {reasons.map((why, at) => (
          <li key={at} className="trouble">
            {why}
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * One side of the Form / Raw TOML switch. **It picks itself on focus**, for `StartChat`'s
 * reason (`docs/ui-primitives.md`): Radix learns an arrow key is down from a `document`
 * listener that runs after React has already moved the focus, so without this the arrows move
 * the ring and not the pick.
 */
function ModeItem({
  value,
  disabled,
  onPick,
  children,
}: {
  value: Mode;
  disabled: boolean;
  onPick: (mode: Mode) => void;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <span className="choice">
      <RadioGroup.Item
        className="dot"
        value={value}
        id={id}
        disabled={disabled}
        onFocus={() => onPick(value)}
      >
        <RadioGroup.Indicator className="dot-mark" />
      </RadioGroup.Item>
      <label htmlFor={id}>{children}</label>
    </span>
  );
}

function SettingControl({
  control,
  value,
  onChange,
}: {
  control: Control;
  value: string;
  onChange: (to: string) => void;
}) {
  const id = useId();
  const hint = useId();
  const described = control.hint ? hint : undefined;
  return (
    <div className="settings-field">
      <label htmlFor={id}>{control.label}</label>
      {control.kind === "colour" ? (
        <ColourControl
          id={id}
          described={described}
          value={value}
          control={control}
          onChange={onChange}
        />
      ) : control.kind === "choice" ? (
        <select
          id={id}
          // #190: WebKit leaves a control out of the Tab order without `tabIndex`.
          tabIndex={0}
          value={value}
          aria-describedby={described}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">{control.unset ?? "not set"}</option>
          {/* A value the file holds that is not one of the choices is still shown as held —
              the core decides what it means, and a form that silently showed another would
              write that one on the next save. */}
          {value !== "" && !control.choices?.includes(value) && (
            <option value={value}>{value}</option>
          )}
          {control.choices?.map((choice) => (
            <option key={choice} value={choice}>
              {control.labels?.[choice] ?? choice}
            </option>
          ))}
        </select>
      ) : control.kind === "lines" ? (
        <textarea
          id={id}
          value={value}
          spellCheck={false}
          rows={Math.max(2, value.split("\n").length)}
          aria-describedby={described}
          onChange={(event) => onChange(event.target.value)}
        />
      ) : (
        <input
          id={id}
          type="text"
          value={value}
          spellCheck={false}
          aria-describedby={described}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
      {control.hint && (
        <p className="settings-hint" id={hint}>
          {control.hint}
        </p>
      )}
    </div>
  );
}

/**
 * A workspace's colour (charter-app#281): the palette as a select, and — while the pick is
 * custom — the platform's own colour well beside it, labelled, whose value is the `#rrggbb` the
 * file holds. `value` is what the file will hold: a palette name, a `#rrggbb`, or empty.
 */
function ColourControl({
  id,
  described,
  value,
  control,
  onChange,
}: {
  id: string;
  described: string | undefined;
  value: string;
  control: Control;
  onChange: (to: string) => void;
}) {
  const well = useId();
  // A `#rrggbb`: what the colour well can hold. Anything else the file holds that is not a
  // palette name — `#fff`, say — is shown as held, below, rather than as a custom colour the
  // well would silently turn black.
  const custom = /^#[0-9a-fA-F]{6}$/.test(value);
  /** Where a new custom colour starts: the accent the window is drawn in, as `#rrggbb`. */
  const start = () =>
    /^#[0-9a-fA-F]{6}/.exec(inForce().values["accent.base"])?.[0] ??
    DEFAULT_THEME.values["accent.base"];
  return (
    <>
      <select
        id={id}
        // #190: WebKit leaves a control out of the Tab order without `tabIndex`.
        tabIndex={0}
        value={custom ? "custom" : value}
        aria-describedby={described}
        onChange={(event) => {
          const to = event.target.value;
          onChange(to === "custom" ? (custom ? value : start()) : to);
        }}
      >
        <option value="">{control.unset ?? "not set"}</option>
        {/* A value the file holds that is neither a name nor a colour is still shown as held,
            for the reason `SettingControl` gives. */}
        {value !== "" && !custom && !control.choices?.includes(value) && (
          <option value={value}>{value}</option>
        )}
        {control.choices?.map((choice) => (
          <option key={choice} value={choice}>
            {control.labels?.[choice] ?? choice}
          </option>
        ))}
      </select>
      {custom && (
        <>
          <label htmlFor={well}>Custom colour</label>
          <input
            id={well}
            type="color"
            tabIndex={0}
            value={value}
            onChange={(event) => onChange(event.target.value)}
          />
        </>
      )}
    </>
  );
}
