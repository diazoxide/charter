import { useCallback, useEffect, useRef, useState } from "react";
import { LiveDialog } from "../LiveDialog";
import { useProjectThemeAnswers } from "../projectTheme";
import { useNotClonedHere, useWorkspaceRepos } from "../WorkspaceRepos";
import {
  commands,
  type HarnessPlugins,
  type PlaneId,
  type ProjectExtensions,
  type ProjectTheme,
  type SettingsChange,
  type WorkspaceSettings,
} from "../bindings";
import {
  EXTENSIONS,
  harnessPluginGroups,
  key,
  NO_EXTENSIONS,
  textAt,
  themeGroup,
  type Control,
  type Shown,
} from "./fileControls";
import { asked, fileSetting, useSettingsDriver, type Driven, type Wrote } from "./driver";
import type { FileSetting, LiveSetting, SettingsGroup } from "./groups";

/**
 * **The Workspace level** (SE-20, #1170; V89b, V89e, V89h): one workspace's settings in the
 * Settings tab, grouped as Live · Repos · Extensions · Appearance · Plugins. It retires the old
 * Workspace settings page (charter-app#280), and keeps everything that page reached.
 *
 * A workspace's settings are the `settings` of its `workspace.json`: the layer between the
 * project's `charter.toml` and this machine's `charter.local.toml`, read in that order — the
 * workspace refines its project for the team, and this machine has the last word. Extensions,
 * Appearance (the theme, the icons and the workspace's colour) and Plugins are kept there —
 * every key a workspace's `settings` may hold (`docs/plane-format.md`), `theme.icons` included,
 * which the old page had no control for — and are written
 * by the same driver as the Project level's (`driver.ts`), through `save_workspace_settings`.
 *
 * **Live and Repos are not values in the file.** Live is whether the workspace is published
 * with the project, switched through the same confirmation as the workspace's menu
 * (`LiveDialog`); Repos is what is cloned in it, applied with a button (`WorkspaceRepos.tsx`).
 * Both are drawn by a hook of their own ({@link LiveSetting}).
 */

/** What the core said about the workspace, which the groups are declared from. */
export type WorkspaceRead = {
  plane: PlaneId;
  workspace: string;
  settings: WorkspaceSettings;
  extensions: ProjectExtensions;
  harnesses: readonly HarnessPlugins[];
  theme: ProjectTheme | undefined;
};

/** What the driver answers at the Workspace level. */
export type WorkspaceLevel =
  | { state: "reading" }
  | { state: "trouble"; trouble: string }
  | (Driven<WorkspaceSettings> & { read: WorkspaceRead });

/** The sentence a setting's help ends on: where it is kept, and who sees it. */
function kept(settings: WorkspaceSettings): string {
  return settings.live
    ? `Kept in ${settings.file}, committed with this LIVE workspace; your team sees it.`
    : `Kept in ${settings.file}, which stays on this machine while the workspace is not LIVE.`;
}

/** Whether the workspace is published with the project, and the way to switch it. */
function liveSetting(read: WorkspaceRead, switched: () => void): LiveSetting {
  const { plane, workspace } = read;
  const live = read.settings.live;
  return {
    id: "workspace.live.live",
    label: "Published with the project",
    help: live
      ? "LIVE: its charter, memory and todos are published with the project."
      : "LOCAL: its charter, memory and todos stay on this machine.",
    useControl: () => {
      const [asking, setAsking] = useState(false);
      return {
        // The button is named by what it does; the row's label names the group it is in, as the
        // Repos row's does.
        grouped: true,
        control: (ids) => (
          <div role="group" aria-labelledby={ids.labelledBy}>
            <button
              type="button"
              className="panel-view"
              // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
              tabIndex={0}
              aria-describedby={ids.describedBy}
              onClick={() => setAsking(true)}
            >
              {live ? "Make local…" : "Make live…"}
            </button>
            {asking && (
              <LiveDialog
                plane={plane}
                workspace={workspace}
                onClose={() => setAsking(false)}
                onDone={() => {
                  setAsking(false);
                  switched();
                }}
              />
            )}
          </div>
        ),
      };
    },
  };
}

/** What is cloned in the workspace, ticked and applied. */
function reposSetting(read: WorkspaceRead): LiveSetting {
  const { plane, workspace } = read;
  return {
    id: "workspace.repos.cloned",
    label: "Cloned here",
    help: "Ticked repos are cloned into this workspace; unticked ones are removed, unless they hold work that is not pushed.",
    useControl: () => ({ ...useWorkspaceRepos(plane, workspace), grouped: true }),
  };
}

/** What the workspace names and this machine has not cloned, each removable from it (#1228). */
function absentSetting(read: WorkspaceRead): LiveSetting {
  const { plane, workspace } = read;
  return {
    id: "workspace.repos.absent",
    label: "Not cloned here",
    help: "Repos this workspace names that are not cloned on this machine. Removing one takes it out of workspace.json; nothing is deleted.",
    useControl: () => ({ ...useNotClonedHere(plane, workspace), grouped: true }),
  };
}

/**
 * **The five groups**, in V89h's order, each with its stable id. A group with no setting is
 * hidden: Extensions with no extension, Plugins with no harness that can set one. A
 * `workspace.json` that is not a JSON object has no file settings to show until it is mended:
 * its standing refusals say why, above the groups.
 */
export function workspaceGroups(read: WorkspaceRead, switched: () => void): SettingsGroup[] {
  const file: Shown = read.settings;
  const fromFile = (group: string, controls: Control[]): FileSetting[] =>
    file.parsed
      ? controls.map((one) => fileSetting(group, "workspace", one, kept(read.settings)))
      : [];
  const extensions = asked(EXTENSIONS, file, read);
  const theme = asked(themeGroup("not set — the project's pick", "workspace"), file, read);
  const plugins = harnessPluginGroups(read.harnesses, "workspace").map((group) =>
    asked(group, file, read),
  );
  return [
    {
      id: "workspace.live",
      label: "Live",
      help: "Whether this workspace's charter, memory and todos are published with the project, or stay on this machine.",
      settings: [liveSetting(read, switched)],
    },
    {
      id: "workspace.repos",
      label: "Repos",
      help: "The repos cloned in this workspace, and the ones it names that are not cloned here.",
      settings: [reposSetting(read), absentSetting(read)],
    },
    {
      id: "workspace.extensions",
      label: "Extensions",
      help: "Which of this machine's extensions are on in this workspace, and what each is set to.",
      settings: fromFile("workspace.extensions", extensions.controls),
      notes: extensions.notes,
    },
    {
      id: "workspace.appearance",
      label: "Appearance",
      help: "The theme and the icons the window draws while this workspace is in front, and the colour it is tinted with.",
      settings: fromFile("workspace.appearance", [
        ...theme.controls,
        textAt(key("theme", "icons"), "Icons", {
          hint: "The file trees' icons while this workspace is in front: charter-icons, or an extension's as <extension>/<icon theme>. Empty is the project's.",
        }),
      ]),
      notes: theme.notes,
    },
    {
      id: "workspace.plugins",
      label: "Plugins",
      help: "Which harness plugins the chats purlis starts in this workspace have on.",
      settings: fromFile(
        "workspace.plugins",
        plugins.flatMap((one) => one.controls),
      ),
      notes: plugins.flatMap((one) => one.notes),
    },
  ];
}

/** Writes the manifest of `workspace` through the core: a form's edits, or its whole text. */
function saveManifest(
  plane: PlaneId,
  workspace: string,
  base: string | null,
  change: SettingsChange,
): Promise<Wrote<WorkspaceSettings>> {
  return commands
    .saveWorkspaceSettings(plane, workspace, base, change)
    .then((said) =>
      said.status === "error"
        ? { refused: [said.error] }
        : said.data.kind === "refused"
          ? { refused: said.data.reasons }
          : { saved: said.data.settings },
    );
}

/**
 * **The Workspace level's driver** (`driver.ts`): reads the workspace's `workspace.json` and what
 * the core says is in force in it, and writes one setting at a time through
 * `save_workspace_settings`, and the whole manifest under Edit as JSON (NO-7, #1232).
 */
export function useWorkspaceLevel(plane: PlaneId, workspace: string): WorkspaceLevel {
  const [extensions, setExtensions] = useState<ProjectExtensions>(NO_EXTENSIONS);
  const [harnesses, setHarnesses] = useState<HarnessPlugins[]>([]);
  const [theme, setTheme] = useState<ProjectTheme>();
  /** The newest asking of what is in force: an answer to an older one is dropped. */
  const asking = useRef(0);
  const themeAsking = useRef(0);

  const readTheme = useCallback(() => {
    const mine = ++themeAsking.current;
    const newest = () => themeAsking.current === mine;
    void commands
      .projectTheme(plane, workspace)
      .then((said) => {
        if (newest()) setTheme(said.status === "ok" ? (said.data ?? undefined) : undefined);
      })
      .catch(() => newest() && setTheme(undefined));
  }, [plane, workspace]);

  const readInForce = useCallback(() => {
    const mine = ++asking.current;
    const newest = () => asking.current === mine;
    void commands
      .projectExtensions(plane, workspace)
      .then((said) => {
        if (newest())
          setExtensions(said.status === "ok" ? (said.data ?? NO_EXTENSIONS) : NO_EXTENSIONS);
      })
      .catch(() => newest() && setExtensions(NO_EXTENSIONS));
    void commands
      .projectHarnessPlugins(plane, workspace)
      .then((said) => {
        if (newest()) setHarnesses(said.status === "ok" ? (said.data ?? []) : []);
      })
      .catch(() => newest() && setHarnesses([]));
    readTheme();
  }, [plane, workspace, readTheme]);

  // What the window draws here was asked again — by a write, or by an approval in the
  // Extensions dialog — so the sentence about it is asked again with it.
  const answers = useProjectThemeAnswers(plane, workspace);
  useEffect(() => {
    if (answers > 0) readTheme();
  }, [answers, readTheme]);

  const driver = useSettingsDriver<WorkspaceSettings>(`${plane}\u0000${workspace}`, {
    plane,
    read: () =>
      commands
        .workspaceSettings(plane, workspace)
        .then((said) => (said.status === "ok" ? { ok: said.data } : { trouble: said.error })),
    files: (settings) => ({ workspace: settings }),
    save: (_which, base, edits) => saveManifest(plane, workspace, base, { kind: "edits", edits }),
    // Edit as JSON (NO-7, #1232): the whole manifest, checked by the core as a form's write is.
    saveRaw: (_which, base, text) => saveManifest(plane, workspace, base, { kind: "raw", text }),
    inForce: readInForce,
  });

  if (driver.state !== "read") return driver;
  return {
    ...driver,
    read: { plane, workspace, settings: driver.now, extensions, harnesses, theme },
  };
}
