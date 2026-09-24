import { useCallback, useEffect, useId, useState, type ReactNode } from "react";
import * as RadioGroup from "@radix-ui/react-radio-group";
import { LoaderCircle } from "lucide-react";
import {
  commands,
  type PlaneId,
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
 * **Where #253 goes.** Each section is a list of {@link Group}s, and a group is data: a heading
 * and the controls under it, each reading and writing keys by path. Per-project extensions are
 * one more group in either section — reading `[extensions]` out of `SettingsFile.fields`, which
 * already carries every key in the file, forms or no forms.
 */
export function ProjectSettings({ plane }: { plane: PlaneId }) {
  const [both, setBoth] = useState<Both | { trouble: string }>();

  const read = useCallback(() => {
    void commands
      .projectSettings(plane)
      .then((said) => setBoth(said.status === "ok" ? said.data : { trouble: said.error }))
      .catch((err: unknown) => setBoth({ trouble: String(err) }));
  }, [plane]);

  useEffect(read, [read]);

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
        onSaved={read}
      />
      <Section
        plane={plane}
        file={both.local}
        title="Local"
        who="This machine only. Gitignored; charter will not write it anywhere git would commit it."
        groups={LOCAL}
        onSaved={read}
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
  read: (file: SettingsFile) => string;
  /** The edits `draft` makes to `file`, the file it was typed over. */
  edits: (draft: string, file: SettingsFile) => SettingsEdit[];
};

/** A heading and what is under it. */
type Group = { title: string; note?: string; controls: (file: SettingsFile) => Control[] };

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
          return cut < 0 ? [line, ""] : [line.slice(0, cut).trim(), line.slice(cut + 1)];
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
];

/** `charter.local.toml`: `[harness]` and nothing else, which is all the loader reads there. */
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
  onSaved,
}: {
  plane: PlaneId;
  file: SettingsFile;
  title: string;
  who: string;
  groups: readonly Group[];
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
  const [seen, setSeen] = useState(file.text);
  if (seen !== file.text) {
    setSeen(file.text);
    setDrafts({});
    setRaw(file.text);
  }

  const controls = groups.map((group) => ({ group, controls: group.controls(file) }));
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
        <div className="settings-standing">
          <p className="note">charter does not take this from the file as it stands:</p>
          <ul>
            {file.refusals.map((why) => (
              <li key={why} className="trouble">
                {why}
              </li>
            ))}
          </ul>
        </div>
      )}

      <RadioGroup.Root
        className="settings-mode"
        orientation="horizontal"
        value={mode}
        onValueChange={(to) => setMode(to as Mode)}
        aria-label={`How to edit ${file.file}`}
      >
        <ModeItem value="form" disabled={(mode === "raw" && dirty) || !file.parsed}>
          Form
        </ModeItem>
        <ModeItem value="raw" disabled={mode === "form" && dirty}>
          Raw TOML
        </ModeItem>
      </RadioGroup.Root>
      {dirty && <p className="settings-hint">Save or discard these changes to switch views.</p>}

      {mode === "form" ? (
        controls.map(({ group, controls: under }) => (
          <fieldset key={group.title} className="settings-group">
            <legend>{group.title}</legend>
            {group.note && <p className="settings-hint">{group.note}</p>}
            {under.length === 0 && <p className="none">None in this file.</p>}
            {under.map((control) => (
              <Field
                key={control.id}
                control={control}
                value={drafts[control.id] ?? control.read(file)}
                onChange={(to) => setDrafts((was) => ({ ...was, [control.id]: to }))}
              />
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
        <div role="alert" className="settings-refused">
          <p className="note">Nothing was saved:</p>
          <ul>
            {refused.map((why) => (
              <li key={why} className="trouble">
                {why}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

function ModeItem({
  value,
  disabled,
  children,
}: {
  value: Mode;
  disabled: boolean;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <span className="choice">
      <RadioGroup.Item className="dot" value={value} id={id} disabled={disabled}>
        <RadioGroup.Indicator className="dot-mark" />
      </RadioGroup.Item>
      <label htmlFor={id}>{children}</label>
    </span>
  );
}

function Field({
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
          value={value}
          aria-describedby={described}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">not set</option>
          {/* A value the file holds that is not one of the choices is still shown as held —
              the core decides what it means, and a form that silently showed another would
              write that one on the next save. */}
          {value !== "" && !control.choices?.includes(value) && (
            <option value={value}>{value}</option>
          )}
          {control.choices?.map((choice) => (
            <option key={choice} value={choice}>
              {choice}
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
