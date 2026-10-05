import { useId, useMemo, useState, type ReactNode } from "react";
import { LoaderCircle } from "lucide-react";
import type { PlaneId, SettingsEdit, SettingsWhich } from "../bindings";
import { DEFAULT_THEME, inForce } from "../theme/theme";
import { atCreation } from "../windowprefs";
import { Choice, Field, SettingGroup, SettingRow, SettingsLayout, type RowIds } from "./components";
import { heldIn, keysOf, type Driven, type Files } from "./driver";
import {
  inAFile,
  LEVELS,
  type FileSetting,
  type Level,
  type LiveSetting,
  type SettingsFileId,
  type SettingsGroup,
} from "./groups";
import { chooseGroup, settingsPlace, useShownGroup } from "./links";
import { KEPT, projectGroups, useProjectLevel } from "./project";
import { named, RawEditor, RawLinks, type RawDraft, type RawFile } from "./RawToml";
import { useWorkspaceLevel, workspaceGroups } from "./workspace";
import { youGroups } from "./you";

/**
 * **Settings** (SE-16, #1166; the spec on #558, rulings V89a–i): the one tab where a setting is
 * read and changed, one level at a time (`CONTEXT.md`, **Settings** and **Level**). A view tab
 * (`tabs.settingsView`), opened from the app menu's Settings…, `⌘,` and the palette.
 *
 * **Two columns.** The level switcher and what that level is sit at the top; the left nav lists
 * the level's groups; the right column shows the chosen group and nothing else. **You**; where
 * the tab is in a project, **Project** (SE-17); and where it is about a workspace — opened at
 * one, or on its strip — **Workspace** (SE-20) are offered. Persona joins the switcher as its
 * groups land, declared as data the same way (`groups.ts`).
 *
 * **The level is the tab's** (D-SE17a): the tab is keyed by it, and the switcher asks for the tab
 * to show another level (`onLevelChange`) rather than keeping one of its own — so opening
 * Settings at a level whose tab is open brings that tab forward, whichever level it was opened
 * at.
 *
 * "Preferences" was this tab's You level before it had a name (charter-app#283), and is retired.
 * So is the long Project settings page with its Form / Raw TOML switch (charter-app#252, SE-19):
 * its forms are the Project level's groups, and its raw view is the level's **Edit as TOML**
 * link per file (`RawToml.tsx`).
 */
export function SettingsTab({
  plane,
  workspace,
  level = "you",
  onLevelChange,
}: {
  /** The project the tab is in; without one only You is offered. */
  plane?: PlaneId;
  /** The workspace the tab is about, in that project; without one Workspace is not offered. */
  workspace?: string;
  level?: Level;
  onLevelChange?: (level: Level) => void;
}) {
  const offered = LEVELS.filter(
    (one) =>
      one.id === "you" ||
      (one.id === "project" && plane !== undefined) ||
      (one.id === "workspace" && plane !== undefined && workspace !== undefined),
  );
  const at = offered.some((one) => one.id === level) ? level : "you";
  const change = (to: string) => onLevelChange?.(to as Level);
  if (at === "workspace" && plane !== undefined && workspace !== undefined)
    return (
      <WorkspaceLevelTab
        key={`${plane}\u0000${workspace}`}
        plane={plane}
        workspace={workspace}
        levels={offered}
        onLevelChange={change}
      />
    );
  return at === "project" && plane !== undefined ? (
    <ProjectLevelTab key={plane} plane={plane} levels={offered} onLevelChange={change} />
  ) : (
    <YouLevel levels={offered} onLevelChange={change} />
  );
}

type Switcher = {
  levels: readonly { id: Level; label: string }[];
  onLevelChange: (level: string) => void;
};

function YouLevel({ levels, onLevelChange }: Switcher) {
  const groups = useMemo(() => youGroups(), []);
  const where = atCreation().layout.path || "the layout file";
  return (
    <Shown
      level="you"
      place={settingsPlace("you")}
      levels={levels}
      onLevelChange={onLevelChange}
      about={`This machine only, in every project. Kept in ${where}.`}
      groups={groups}
    />
  );
}

function ProjectLevelTab({ plane, ...switcher }: Switcher & { plane: PlaneId }) {
  const project = useProjectLevel(plane);
  const groups = useMemo(
    () => (project.state === "read" ? projectGroups(project.read) : []),
    [project],
  );
  // Each file as its whole text (SE-19, V89d), written through the level's driver: the core
  // checks it and writes it, in the queue every setting's write is in.
  const raw: RawFile[] =
    project.state === "read"
      ? (["shared", "local"] as const).map((which: SettingsWhich) => ({
          id: which,
          file: project.read[which],
          kept: KEPT[which],
          save: (base, text) => project.writeRaw(which, base, text),
        }))
      : [];
  const standing =
    project.state === "read"
      ? [project.read.shared, project.read.local].flatMap((file) =>
          file.refusals.map((why) => `${file.file}: ${why}`),
        )
      : [];
  return (
    <Shown
      level="project"
      place={settingsPlace("project", plane)}
      {...switcher}
      about="This project, for everyone who opens it. Never put a secret in its files: keep it in a vault and name it as vault:<vault>/<key>."
      groups={groups}
      waiting={waitingFor(project)}
      standing={standing}
      mend={
        project.state === "read"
          ? [project.read.shared, project.read.local]
              .filter((file) => !file.parsed)
              .map((file) => `Open ${named(file)} under Edit as TOML to mend it.`)
          : []
      }
      driver={project.state === "read" ? project : undefined}
      raw={raw}
    />
  );
}

/** What stands in for a level's groups until it has been read. */
function waitingFor(
  level: { state: "reading" } | { state: "trouble"; trouble: string } | Driven<unknown>,
) {
  return level.state === "reading" ? (
    <p className="pending" aria-busy="true">
      <LoaderCircle className="node-icon spinning" />
      Reading the settings…
    </p>
  ) : level.state === "trouble" ? (
    <p className="trouble" role="alert">
      {level.trouble}
    </p>
  ) : undefined;
}

function WorkspaceLevelTab({
  plane,
  workspace,
  ...switcher
}: Switcher & { plane: PlaneId; workspace: string }) {
  const level = useWorkspaceLevel(plane, workspace);
  const groups = useMemo(
    () => (level.state === "read" ? workspaceGroups(level.read, level.reread) : []),
    [level],
  );
  return (
    <Shown
      level="workspace"
      place={settingsPlace("workspace", plane, workspace)}
      {...switcher}
      about={`The workspace ${workspace}, read between charter.toml and charter.local.toml: it refines its project for the team, and this machine has the last word. Never put a secret in its settings: keep it in a vault and name it as vault:<vault>/<key>.`}
      groups={groups}
      waiting={waitingFor(level)}
      standing={level.state === "read" ? level.read.settings.refusals : []}
      driver={level.state === "read" ? level : undefined}
    />
  );
}

/**
 * One level, drawn: its groups that have a setting in the nav, and the chosen one on the right.
 * `waiting` stands in for the groups until the level has been read.
 *
 * **Which group is the place's** (SE-22, `links.ts`): the one last picked at this level and
 * target, or the one a link last landed on — so a level comes back at the group it was left
 * at, and a link to a group shows it even in a tab already open.
 */
function Shown({
  level,
  place,
  levels,
  onLevelChange,
  about,
  groups: declared,
  waiting,
  standing = [],
  mend = [],
  driver,
  raw = [],
}: Switcher & {
  level: Level;
  /** Where the group shown is remembered (`links.settingsPlace`). */
  place: string;
  about: string;
  groups: readonly SettingsGroup[];
  waiting?: ReactNode;
  /** What charter refuses in the level's files as they stand. */
  standing?: readonly string[];
  /** Where a file that is not read as it stands is mended: said under {@link standing}. */
  mend?: readonly string[];
  /** What writes the level's file settings, once the level has been read. */
  driver?: Driven<unknown>;
  /** The level's files as raw TOML, each with its link at the foot of the nav (SE-19). */
  raw?: readonly RawFile[];
}) {
  // Per tab and not remembered (V89c): a level drawn afresh starts with the whole nav.
  const [filter, setFilter] = useState("");
  /** The file whose text is on the right in place of a group, while one is. */
  const [editing, setEditing] = useState<string>();
  const shown = useShownGroup(place);
  // A link that lands here clears the filter and puts away a file's text, so the group it
  // names is on screen. Adjusted during render, React's way for state that follows a value
  // that changed. A draft typed into the text is kept, as it is when a group is picked.
  const linked = shown?.linked ?? 0;
  const [landed, setLanded] = useState(linked);
  if (landed !== linked) {
    setLanded(linked);
    setFilter("");
    setEditing(undefined);
  }
  const choose = (group: string) => chooseGroup(place, group);
  const groups = narrowed(
    declared.filter((one) => one.settings.length > 0),
    filter,
  );
  const group = groups.find((one) => one.id === shown?.group) ?? groups[0];
  /** What is typed into each file's text, kept while a group is looked at. */
  const [drafts, setDrafts] = useState<Partial<Record<string, RawDraft>>>({});
  const rawFile = raw.find((one) => one.id === editing);
  const found =
    waiting === undefined ? groups.reduce((all, one) => all + one.settings.length, 0) : undefined;
  return (
    <SettingsLayout
      levels={levels}
      level={level}
      onLevelChange={onLevelChange}
      about={about}
      groups={groups}
      group={rawFile ? "" : (group?.id ?? "")}
      onGroupChange={(to) => {
        setEditing(undefined);
        choose(to);
      }}
      filter={filter}
      onFilterChange={(to) => {
        // The group on screen stays the chosen one while it is still matched, and is the one
        // shown again once the box is cleared.
        if (group) choose(group.id);
        setEditing(undefined);
        setFilter(to);
      }}
      found={rawFile ? undefined : found}
      foot={
        raw.length > 0 && waiting === undefined ? (
          <RawLinks files={raw} editing={rawFile?.id} onEdit={setEditing} />
        ) : undefined
      }
    >
      {waiting ?? (
        <>
          {standing.length + mend.length > 0 && (
            <div className="ui-settings-standing">
              <p className="note">charter does not take this from the files as they stand:</p>
              <ul>
                {standing.map((why, at) => (
                  <li key={at} className="trouble">
                    {why}
                  </li>
                ))}
              </ul>
              {mend.map((where) => (
                <p key={where} className="note">
                  {where}
                </p>
              ))}
            </div>
          )}
          {rawFile ? (
            <RawEditor
              key={rawFile.id}
              raw={rawFile}
              draft={drafts[rawFile.id]}
              onDraft={(to) =>
                setDrafts((was) => ({
                  ...was,
                  [rawFile.id]: typeof to === "function" ? to(was[rawFile.id]) : to,
                }))
              }
            />
          ) : (
            group && <ShownGroup key={group.id} group={group} driver={driver} />
          )}
        </>
      )}
    </SettingsLayout>
  );
}

/**
 * **The groups and settings a filter leaves** (SE-21, V89c): a group whose own label or help
 * holds the words keeps every setting; any other group keeps the settings whose label or help
 * holds them, and is left out when none does. Case is ignored, and so is space around the
 * words; with none, every group is left as it is.
 */
function narrowed(groups: readonly SettingsGroup[], filter: string): readonly SettingsGroup[] {
  const words = filter.trim().toLocaleLowerCase();
  if (words === "") return groups;
  const holds = (...texts: string[]) =>
    texts.some((text) => text.toLocaleLowerCase().includes(words));
  return groups.flatMap((one) => {
    if (holds(one.label, one.help)) return [one];
    const settings = one.settings.filter((setting) => holds(setting.label, setting.help));
    return settings.length > 0 ? [{ ...one, settings }] : [];
  });
}

/** The chosen group, drawn from its data. Keyed by the group, so each setting's hook is always
 *  the same one in a given row. */
function ShownGroup({ group, driver }: { group: SettingsGroup; driver?: Driven<unknown> }) {
  return (
    <SettingGroup label={group.label} help={group.help}>
      {group.notes?.map((why, at) => (
        <p key={at} className="ui-setting-help">
          {why}
        </p>
      ))}
      {group.settings.map((setting) =>
        inAFile(setting) ? (
          driver && <FileRow key={setting.id} setting={setting} driver={driver} />
        ) : (
          <LiveRow key={setting.id} setting={setting} />
        ),
      )}
    </SettingGroup>
  );
}

function LiveRow({ setting }: { setting: LiveSetting }) {
  const { control, reset, grouped, error, undo } = setting.useControl();
  return (
    <SettingRow
      label={setting.label}
      help={setting.help}
      reset={reset}
      grouped={grouped}
      error={error}
      undo={undo}
      control={control}
    />
  );
}

/**
 * **Where a value comes from** (SE-18, V89d): the level and the file, or that no file at this
 * level holds it. A movable value kept on this machine over one the Shared file holds says what
 * the Shared file has, so the person sees why their value is not their team's. Only a movable
 * one: a Local-only setting at a key charter.toml also has (the default profile, where
 * charter.toml has the default harness) is another setting, not an override of it.
 */
function originOf(setting: FileSetting, files: Files, from: SettingsFileId | undefined): string {
  if (from === undefined) return "Not set at this level, so the value beneath it is in force.";
  const file = files[from]?.file ?? "";
  if (from === "workspace") return `From ${file}, at the Workspace level.`;
  if (from === "shared") return `From ${file}, at the Project level: shared with your team.`;
  const shared = sharedUnder(setting, files);
  const under = shared === undefined ? "" : ` charter.toml has ${shared}, which this overrides.`;
  return `From ${file}, at the Project level: this machine only.${under}`;
}

/**
 * What charter.toml has for a movable setting whose value this machine's file overrides — `a
 * value` when it reads as nothing — or `undefined` when nothing is overridden.
 */
function sharedUnder(setting: FileSetting, files: Files): string | undefined {
  const shared = files.shared;
  if (!setting.movable || shared === undefined || keysOf(setting, shared).length === 0)
    return undefined;
  return setting.read(shared) || "a value";
}

/** The two files a movable value may be kept in, as the file choice offers them. */
const PLACES = [
  { value: "shared", label: "Shared", says: "charter.toml, which your team sees" },
  { value: "local", label: "Only on this machine", says: "charter.local.toml" },
] as const;

/** Every key the setting has in `file`, taken out. */
function removals(setting: FileSetting, files: Files, from: SettingsFileId | undefined) {
  const file = from === undefined ? undefined : files[from];
  return file === undefined
    ? []
    : keysOf(setting, file).map((path): SettingsEdit => ({ path, value: null }));
}

/**
 * **A setting kept in a file**: drawn at what is on disk — or at what is being written, until
 * that write settles — and written as it changes. A pick is written at once; a typed value when
 * the field is left (`Field`'s `onCommit`).
 *
 * Every row says where its value comes from and offers a reset, which takes it out of that file
 * so the value beneath shows through (SE-18). A movable row's file choice moves a value between
 * `charter.toml` and `charter.local.toml`: a move writes both files and has no Undo, so the pick
 * is held and made on a button, never as the arrows pass over it (`ui-primitives.md`). While no
 * file holds the value, the pick only says where the next value goes, and writes nothing.
 */
function FileRow({ setting, driver: project }: { setting: FileSetting; driver: Driven<unknown> }) {
  const from = heldIn(setting, project.files);
  const [pick, setPick] = useState<SettingsWhich>();
  /** The file a new value goes to: where it is, else where it was picked to go. */
  const into: SettingsFileId = from ?? pick ?? setting.file;
  const file = project.files[into];
  const onDisk = file === undefined ? "" : setting.read(file);
  const [draft, setDraft] = useState<string>();
  const value = draft ?? project.pending[setting.id] ?? onDisk;
  const writing = project.pending[setting.id] !== undefined;
  const write = (to: string) => project.write(setting, to, into);
  // Always handed to the queue, which skips a write that would change nothing at its turn: a
  // value typed back to what is on disk while another write of it is pending is still written.
  const commit = () => {
    if (draft === undefined) return;
    setDraft(undefined);
    write(draft);
  };
  const out = removals(setting, project.files, from);
  // Never one that would loosen what the level holds fast: the sandbox (D-SE17g, D-SE18e).
  const mayTakeOut = from !== undefined && !setting.oneWay && project.mayChange(out);
  /** What charter.toml has under this machine's value, which a move to it would replace. */
  const overridden = from === "local" ? sharedUnder(setting, project.files) : undefined;
  return (
    <SettingRow
      label={setting.label}
      help={setting.help}
      error={project.refused[setting.id]}
      undo={project.undoable === setting.id && !writing ? project.undo : undefined}
      origin={originOf(setting, project.files, from)}
      badge={overridden !== undefined ? "Overrides charter.toml" : undefined}
      reset={
        mayTakeOut
          ? { label: "Reset", disabled: writing, onReset: () => project.reset(setting) }
          : undefined
      }
      place={
        setting.movable && (
          <Place
            held={from === "shared" || from === "local" ? from : undefined}
            pick={pick ?? (from === "shared" || from === "local" ? from : "shared")}
            onPick={setPick}
            mayMove={mayTakeOut}
            replacing={overridden}
            disabled={writing}
            onMove={(to) => {
              setPick(undefined);
              project.move(setting, to);
            }}
          />
        )
      }
      control={(ids) =>
        setting.kind === "colour" ? (
          <Colour ids={ids} setting={setting} value={value} onValueChange={write} />
        ) : setting.kind === "choice" ? (
          <Choice
            kind="select"
            ids={ids}
            options={[
              // A value the file holds that is not one of the choices is still shown as held:
              // the core decides what it means, and a select that showed another would write it.
              ...(value !== "" && !setting.choices?.includes(value) ? [value] : []),
              ...(setting.choices ?? []),
            ].map((one) => ({ value: one, label: setting.labels?.[one] ?? one }))}
            value={value}
            unset={
              setting.oneWay && writing
                ? undefined
                : (setting.unset ?? (value === "" ? "not set" : undefined))
            }
            onValueChange={write}
          />
        ) : (
          <Field
            kind={setting.kind === "lines" ? "list" : "text"}
            ids={ids}
            value={value}
            onChange={setDraft}
            onCommit={commit}
          />
        )
      }
    />
  );
}

/**
 * **The file choice** (SE-18, V89d): Shared or Only on this machine, a radio group. Where a file
 * holds the value, a pick of the other is held until its button moves it; where none does, the
 * pick is where the next value goes.
 */
function Place({
  held,
  pick,
  onPick,
  mayMove,
  replacing,
  disabled,
  onMove,
}: {
  /** The file that holds the value, if one does. */
  held: SettingsWhich | undefined;
  pick: SettingsWhich;
  onPick: (to: SettingsWhich) => void;
  mayMove: boolean;
  /** What a move to charter.toml writes over: the team's value, which no Undo puts back. */
  replacing: string | undefined;
  disabled: boolean;
  onMove: (to: SettingsWhich) => void;
}) {
  const id = useId();
  const name = useId();
  return (
    <div className="ui-setting-place">
      <span id={name}>Where it is kept</span>
      <Choice
        kind="radio"
        ids={{ id, labelledBy: name }}
        options={PLACES}
        value={pick}
        // Not while a write is pending: the focus comes back here once Move is pressed.
        disabled={held !== undefined && !mayMove}
        onValueChange={(one) => onPick(one as SettingsWhich)}
      />
      {held !== undefined && pick !== held && mayMove && (
        <button
          type="button"
          className="ui-setting-reset"
          // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
          tabIndex={0}
          disabled={disabled}
          onClick={() => {
            onMove(pick);
            // The button goes once it is pressed; the focus goes to the file the value went to.
            // A Choice names its options by place (D-DS3e-10).
            const at = PLACES.findIndex((one) => one.value === pick);
            document.getElementById(`${id}-${at}`)?.focus();
          }}
        >
          {pick === "local"
            ? "Move to charter.local.toml"
            : `Move to charter.toml${replacing === undefined ? "" : `, replacing ${replacing}`}`}
        </button>
      )}
    </div>
  );
}

/** The pick that says a workspace's colour is its own `#rrggbb` rather than a palette name. */
const CUSTOM = "custom";

/**
 * **A colour** (a workspace's, charter-app#281): the palette as a select, and — while the pick
 * is custom — the platform's own colour well beside it, labelled, whose value is the `#rrggbb`
 * the file holds. Picking Custom writes the accent the window is drawn in, as a start; the well
 * writes its colour once it is picked — the native `change`, when the picker closes — or left,
 * never at every step of a drag (React's `onChange` is every step).
 */
function Colour({
  ids,
  setting,
  value,
  onValueChange,
}: {
  ids: RowIds;
  setting: FileSetting;
  value: string;
  onValueChange: (to: string) => void;
}) {
  const well = useId();
  const [dragged, setDragged] = useState<string>();
  // A `#rrggbb`: what the colour well can hold. Anything else the file holds that is not a
  // palette name — `#fff`, say — is shown as held rather than as a custom colour the well would
  // silently turn black.
  const custom = /^#[0-9a-fA-F]{6}$/.test(value);
  const names = (setting.choices ?? []).filter((one) => one !== CUSTOM);
  /** Where a new custom colour starts: the accent the window is drawn in, as `#rrggbb`. */
  const start = () =>
    /^#[0-9a-fA-F]{6}/.exec(inForce().values["accent.base"])?.[0] ??
    DEFAULT_THEME.values["accent.base"];
  const commit = (to: string | undefined) => {
    setDragged(undefined);
    if (to !== undefined && to !== value) onValueChange(to);
  };
  return (
    <>
      <Choice
        kind="select"
        ids={ids}
        options={[
          ...(value !== "" && !custom && !names.includes(value) ? [value] : []),
          ...(setting.choices ?? []),
        ].map((one) => ({ value: one, label: setting.labels?.[one] ?? one }))}
        value={custom ? CUSTOM : value}
        unset={setting.unset}
        onValueChange={(to) => {
          if (to === CUSTOM) {
            if (!custom) onValueChange(start());
          } else onValueChange(to);
        }}
      />
      {custom && (
        <>
          <label htmlFor={well}>Custom colour</label>
          <input
            id={well}
            type="color"
            tabIndex={0}
            value={dragged ?? value}
            onChange={(event) => setDragged(event.currentTarget.value)}
            ref={(element) => {
              if (element) element.onchange = () => commit(element.value);
            }}
            onBlur={() => commit(dragged)}
          />
        </>
      )}
    </>
  );
}
