import { useId, useMemo, useState, type ReactNode } from "react";
import { LoaderCircle } from "lucide-react";
import type { PlaneId } from "../bindings";
import { DEFAULT_THEME, inForce } from "../theme/theme";
import { atCreation } from "../windowprefs";
import { Choice, Field, SettingGroup, SettingRow, SettingsLayout, type RowIds } from "./components";
import type { Driven } from "./driver";
import {
  inAFile,
  LEVELS,
  type FileSetting,
  type Level,
  type LiveSetting,
  type SettingsGroup,
} from "./groups";
import { projectGroups, useProjectLevel } from "./project";
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
  const standing =
    project.state === "read"
      ? [project.read.shared, project.read.local].flatMap((file) =>
          file.refusals.map((why) => `${file.file}: ${why}`),
        )
      : [];
  return (
    <Shown
      level="project"
      {...switcher}
      about="This project, for everyone who opens it. Never put a secret in its files: keep it in a vault and name it as vault:<vault>/<key>."
      groups={groups}
      waiting={waitingFor(project)}
      standing={standing}
      driver={project.state === "read" ? project : undefined}
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
 */
function Shown({
  level,
  levels,
  onLevelChange,
  about,
  groups: declared,
  waiting,
  standing = [],
  driver,
}: Switcher & {
  level: Level;
  about: string;
  groups: readonly SettingsGroup[];
  waiting?: ReactNode;
  /** What charter refuses in the level's files as they stand. */
  standing?: readonly string[];
  /** What writes the level's file settings, once the level has been read. */
  driver?: Driven<unknown>;
}) {
  // Per tab and not remembered (V89c): a level drawn afresh starts with the whole nav.
  const [filter, setFilter] = useState("");
  const groups = narrowed(
    declared.filter((one) => one.settings.length > 0),
    filter,
  );
  const [chosen, choose] = useState<string>();
  const group = groups.find((one) => one.id === chosen) ?? groups[0];
  const found =
    waiting === undefined ? groups.reduce((all, one) => all + one.settings.length, 0) : undefined;
  return (
    <SettingsLayout
      levels={levels}
      level={level}
      onLevelChange={onLevelChange}
      about={about}
      groups={groups}
      group={group?.id ?? ""}
      onGroupChange={choose}
      filter={filter}
      onFilterChange={(to) => {
        // The group on screen stays the chosen one while it is still matched, and is the one
        // shown again once the box is cleared.
        if (group) choose(group.id);
        setFilter(to);
      }}
      found={found}
    >
      {waiting ?? (
        <>
          {standing.length > 0 && (
            <div className="ui-settings-standing">
              <p className="note">charter does not take this from the files as they stand:</p>
              <ul>
                {standing.map((why, at) => (
                  <li key={at} className="trouble">
                    {why}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {group && <ShownGroup key={group.id} group={group} driver={driver} />}
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
  const { control, reset, grouped, error } = setting.useControl();
  return (
    <SettingRow
      label={setting.label}
      help={setting.help}
      reset={reset}
      grouped={grouped}
      error={error}
      control={control}
    />
  );
}

/**
 * **A setting kept in a file**: drawn at what is on disk — or at what is being written, until
 * that write settles — and written as it changes. A pick is written at once; a typed value when
 * the field is left (`Field`'s `onCommit`).
 */
function FileRow({ setting, driver: project }: { setting: FileSetting; driver: Driven<unknown> }) {
  const file = project.files[setting.file];
  const onDisk = file === undefined ? "" : setting.read(file);
  const [draft, setDraft] = useState<string>();
  const value = draft ?? project.pending[setting.id] ?? onDisk;
  const writing = project.pending[setting.id] !== undefined;
  // Always handed to the queue, which skips a write that would change nothing at its turn: a
  // value typed back to what is on disk while another write of it is pending is still written.
  const commit = () => {
    if (draft === undefined) return;
    setDraft(undefined);
    project.write(setting, draft);
  };
  return (
    <SettingRow
      label={setting.label}
      help={setting.help}
      error={project.refused[setting.id]}
      undo={project.undoable === setting.id && !writing ? project.undo : undefined}
      control={(ids) =>
        setting.kind === "colour" ? (
          <Colour
            ids={ids}
            setting={setting}
            value={value}
            onValueChange={(to) => project.write(setting, to)}
          />
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
            onValueChange={(to) => project.write(setting, to)}
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
