import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import * as RadioGroup from "@radix-ui/react-radio-group";
import { LoaderCircle } from "lucide-react";
import { extensionsChanged } from "./extensionsOn";
import { projectThemeChanged, useProjectThemeAnswers } from "./projectTheme";
import { BUILT_IN, SYSTEM } from "./theme/theme";
import {
  commands,
  type PlaneId,
  type ProjectExtension,
  type ProjectTheme,
  type ProjectSettings as Both,
  type SettingsEdit,
  type SettingsFile,
  type SettingsStep,
  type SettingsValue,
} from "./bindings";

/**
 * **Project settings** (charter-app#252): a plane's two settings files, as forms and as raw TOML,
 * in a view tab of their own (`tabs.SETTINGS_VIEW`).
 *
 * - **Shared** is `charter.toml`, committed: the team sees it.
 * - **Local** is `charter.local.toml`, gitignored: this machine only. Harness profiles live here.
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
 */
export function ProjectSettings({ plane }: { plane: PlaneId }) {
  const [both, setBoth] = useState<Both | { trouble: string }>();
  const [extensions, setExtensions] = useState<ProjectExtension[]>([]);
  const [theme, setTheme] = useState<ProjectTheme>();
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
      .projectTheme(plane)
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
      .projectExtensions(plane)
      .then((said) => {
        if (newest()) setExtensions(said.status === "ok" ? (said.data ?? []) : []);
      })
      .catch(() => {
        if (newest()) setExtensions([]);
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
        plane={plane}
        file={both.shared}
        title="Shared"
        who="Committed; your team sees this."
        groups={SHARED}
        extensions={extensions}
        theme={theme}
        onSaved={saved}
      />
      <Section
        plane={plane}
        file={both.local}
        title="Local"
        who="This machine only. Gitignored; charter will not write it anywhere git would commit it."
        groups={LOCAL}
        extensions={extensions}
        theme={theme}
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
type Control = {
  id: string;
  label: string;
  hint?: string;
  /** `text` is one line, `choice` a closed set, `lines` one entry per line. */
  kind: "text" | "choice" | "lines";
  choices?: readonly string[];
  /** What a `choice`'s empty option says. */
  unset?: string;
  /** What a `choice` shows for each of its values, when that is not the value itself. */
  labels?: Readonly<Record<string, string>>;
  read: (file: SettingsFile) => string;
  /** The edits `draft` makes to `file`, the file it was typed over. */
  edits: (draft: string, file: SettingsFile) => SettingsEdit[];
};

/** A heading and what is under it. `extensions` is what the core says is in force in this
 *  project, and `theme` what it says the project draws, for the groups that draw them. */
type Group = {
  title: string;
  note?: string;
  controls: (
    file: SettingsFile,
    extensions: readonly ProjectExtension[],
    theme: ProjectTheme | undefined,
  ) => Control[];
  /** Sentences about this file the group says under its heading. */
  notes?: (
    file: SettingsFile,
    extensions: readonly ProjectExtension[],
    theme: ProjectTheme | undefined,
  ) => string[];
  /** What the group says when it has no controls; "None in this file." otherwise. */
  empty?: string;
};

const key = (...keys: string[]): SettingsStep[] => keys.map((one) => ({ key: one }));

function same(a: readonly SettingsStep[], b: readonly SettingsStep[]): boolean {
  return (
    a.length === b.length && a.every((step, at) => JSON.stringify(step) === JSON.stringify(b[at]))
  );
}

function valueAt(file: SettingsFile, path: readonly SettingsStep[]): SettingsValue | undefined {
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
function forgeBlocks(file: SettingsFile): number[] {
  const at = new Set<number>();
  for (const { path } of file.fields) {
    const [first, second] = path;
    if (first?.key === "forge" && second?.index !== undefined) at.add(second.index);
  }
  return [...at];
}

function profiles(file: SettingsFile): string[] {
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
  const pairs = (file: SettingsFile) =>
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

/** What an extension is in this project, and why, in a sentence after its name. */
function standing(it: ProjectExtension): string {
  const file = it.source === "local" ? "charter.local.toml" : "charter.toml";
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
  if (source === "local") return "from charter.local.toml";
  if (source === "shared") return "from charter.toml";
  return "its default";
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
};

/** charter's own picks, for when the core could not be asked what else there is. */
const BUILT_IN_PICKS = [...Object.keys(BUILT_IN), SYSTEM];

/**
 * **Theme, in either section** (charter-app#273, ADR 0048): `[theme] use`. Local's pick wins over
 * Shared's; an extension's theme is drawn only while the project has that extension on and this
 * machine approved it, and the core's sentence says why when the pick in force is not drawn —
 * under the section whose file made it.
 */
function themeGroup(unset: string): Group {
  const path = key("theme", "use");
  return {
    title: "Theme",
    controls: (_file, _extensions, theme) => {
      const options = theme?.options ?? BUILT_IN_PICKS.map((value) => ({ value, label: value }));
      const labels = Object.fromEntries(options.map((one) => [one.value, one.label]));
      const drawn =
        theme?.draws == null
          ? "the window's own theme — your theme.json, else the first theme from an extension this project has on, else charter-dark"
          : (labels[theme.draws] ?? theme.draws);
      return [
        {
          ...textAt(path, "Theme", {
            kind: "choice",
            choices: options.map((one) => one.value),
            hint: `Drawn in this project: ${drawn}. The terminal follows the window.`,
          }),
          unset,
          labels,
        },
      ];
    },
    notes: (file, _extensions, theme) => {
      if (theme === undefined) return [];
      return [
        ...(theme.why !== null && theme.file === file.file ? [theme.why] : []),
        ...theme.ignored.filter((one) => one.file === file.file).map((one) => one.why),
      ];
    },
  };
}

/** The harness kinds a profile may name — `profiles::KINDS`, in the registry's order. */
const KINDS = ["claude", "opencode", "codex"] as const;

/** `charter.toml`, as `docs/plane-format.md` documents it. `[frame]` is the tmux frame's, which
 *  this charter does not have and nothing reads (the doctor says so): the raw view has it. */
const SHARED: Group[] = [
  {
    title: "Plane",
    controls: () => [
      textAt(key("workspace", "default"), "Default workspace"),
      textAt(key("persona", "default"), "Default persona", {
        hint: "The persona a chat starts as when nothing else names one.",
      }),
      textAt(key("harness", "default"), "Default harness", {
        hint: "claude, opencode, codex, or a profile charter.local.toml declares.",
      }),
      textAt(key("memory", "share"), "How far a memory travels", {
        kind: "choice",
        choices: ["local", "commit", "push"],
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
  EXTENSIONS,
  themeGroup("not set — the window's own theme"),
];

/** `charter.local.toml`: `[harness]`, `[extensions]` and `[theme]`, which is all its readers read
 *  there. */
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
  EXTENSIONS,
  themeGroup("not set — charter.toml's pick"),
];

// ------------------------------------------------------------------------------------------
// one file
// ------------------------------------------------------------------------------------------

type Mode = "form" | "raw";

function Section({
  plane,
  file,
  title,
  who,
  groups,
  extensions,
  theme,
  onSaved,
}: {
  plane: PlaneId;
  file: SettingsFile;
  title: string;
  who: string;
  groups: readonly Group[];
  extensions: readonly ProjectExtension[];
  theme: ProjectTheme | undefined;
  onSaved: () => void;
}) {
  const heading = useId();
  const [mode, setMode] = useState<Mode>(file.parsed ? "form" : "raw");
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
    if (!file.parsed) setMode("raw");
  }

  const controls = groups.map((group) => ({
    group,
    controls: group.controls(file, extensions, theme),
    notes: group.notes?.(file, extensions, theme) ?? [],
  }));
  const all = controls.flatMap((one) => one.controls);
  const changed = all.filter((one) => one.id in drafts && drafts[one.id] !== one.read(file));
  const dirty = mode === "form" ? changed.length > 0 : raw !== file.text;

  const discard = () => {
    setDrafts({});
    setRaw(file.text);
    setRefused(undefined);
  };

  const save = async () => {
    setSaving(true);
    const said = await commands
      .saveProjectSettings(
        plane,
        file.which,
        file.exists ? file.text : null,
        mode === "raw"
          ? { kind: "raw", text: raw }
          : { kind: "edits", edits: changed.flatMap((one) => one.edits(drafts[one.id], file)) },
      )
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
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
    <section
      className="settings-file"
      aria-labelledby={heading}
      data-testid={`settings-${file.which}`}
    >
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
      {dirty && <p className="settings-hint">Save or discard these changes to switch views.</p>}

      {mode === "form" ? (
        controls.map(({ group, controls: under, notes }) => (
          <fieldset key={group.title} className="settings-group">
            <legend>{group.title}</legend>
            {group.note && <p className="settings-hint">{group.note}</p>}
            {under.length === 0 && <p className="none">{group.empty ?? "None in this file."}</p>}
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
          {saving ? "Saving…" : `Save ${file.file}`}
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
      {control.kind === "choice" ? (
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
