import { LiveDialog } from "./LiveDialog";
import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import * as RadioGroup from "@radix-ui/react-radio-group";
import { LoaderCircle } from "lucide-react";
import { extensionsChanged } from "./extensionsOn";
import { projectThemeChanged, useProjectThemeAnswers } from "./projectTheme";
import { DEFAULT_THEME, inForce } from "./theme/theme";
import { WorkspaceRepos } from "./WorkspaceRepos";
import {
  commands,
  type HarnessPlugins,
  type PlaneId,
  type ProjectExtensions,
  type ProjectTheme,
  type SandboxState,
  type ProjectSettings as Both,
  type SettingsChange,
  type WorkspaceSettings as OneWorkspace,
} from "./bindings";
import {
  harnessPluginGroups,
  LOCAL,
  NO_EXTENSIONS,
  SHARED,
  WORKSPACE,
  type Control,
  type Group,
  type Saving,
  type Shown,
} from "./settings/fileControls";

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
  const [sandbox, setSandbox] = useState<SandboxState>();
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
    // The sandbox, and this machine's opt-out count (ADR 0067 §7, V78 d): local, never sent.
    void commands
      .sandboxState(plane)
      .then((said) => {
        if (newest()) setSandbox(said.status === "ok" ? (said.data ?? undefined) : undefined);
      })
      .catch(() => {
        if (newest()) setSandbox(undefined);
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
      {sandbox && (
        <p className="note" data-testid="settings-sandbox">
          {sandbox.on ? (
            <>Sandbox: on. {sandbox.said}. Counted on this machine only, and never sent.</>
          ) : (
            <>
              Sandbox: off. <code>[sandbox] mode = &quot;on&quot;</code> in the Shared file runs
              every chat sandboxed.
            </>
          )}
        </p>
      )}
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
