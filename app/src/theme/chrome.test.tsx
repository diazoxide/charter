/**
 * **The token test, run on the window's chrome and its dialogs** (DS-1a, #956).
 *
 * `views.test.tsx` holds every view a tab can show to the rendered token check
 * (`complaints.testkit.ts`); this holds the rest of the window to it: the title bar, the three
 * strips, the explorer and its panels, the status line, the palette and the alerts drawer, drawn
 * by `App` itself with the core mocked — in each built-in theme, and again with a workspace
 * colour tinting the theme in force. Then every dialog the window has is opened once and checked
 * the same way, in each built-in theme.
 *
 * A dialog is drawn in a portal on `document.body`, outside the container a render hands back,
 * so everything here checks the whole body.
 *
 * **Each dialog is drawn by its own component**, open, with the props the window hands it, rather
 * than reached through the window: that is one press for a dialog that a whole-window path would
 * need a dozen mocked commands to arrive at, and it is the same markup. The three that open from
 * a view (the vault tab's, Push and Land, an extension action that asks first) are opened from
 * their own components the same way. Which files draw a dialog is read from the sources, and a
 * test below fails for a file with a dialog that has no way in here.
 */
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, configure, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "../App";
import { AboutCharter } from "../About";
import { ENDS_IT } from "../actions";
import { AlertsDrawer } from "../AlertsDrawer";
import { AnswerPanel } from "../AnswerPanel";
import { ApproveExtension } from "../ApproveExtension";
import { ApprovePlane } from "../ApprovePlane";
import { AskPersona } from "../AskPersona";
import type { LandQuestion, PlaneId, VaultContents } from "../bindings";
import { BriefPanel } from "../Brief";
import { LandAsk, PushAsk } from "../ChangeActions";
import { ChatAsk } from "../ChatAsk";
import { ClosingProject } from "../ClosingProject";
import { DeleteVault } from "../DeleteVault";
import { DeleteWorkspace } from "../DeleteWorkspace";
import { Health } from "../Doctor";
import { EndingChat } from "../EndingChat";
import { AskFirst } from "../ExtensionAction";
import { Extensions } from "../Extensions";
import { LinkWorkItem } from "../LinkWorkItem";
import { LiveDialog } from "../LiveDialog";
import { NewBranch } from "../NewBranch";
import { NewPersona } from "../NewPersona";
import { NewProject } from "../NewProject";
import { NewVault } from "../NewVault";
import { NewWorkspace } from "../NewWorkspace";
import { Palette } from "../Palette";
import { PanelList } from "../PanelList";
import { PersonaProfile } from "../PersonaProfile";
import { MidTurn, QuitWarning, type Ending } from "../QuitWarning";
import { RelaunchAsk } from "../RelaunchAsk";
import { RemoveFromWorkspace } from "../RemoveFromWorkspace";
import { RemovePersona } from "../RemovePersona";
import { RenameWorkspace } from "../RenameWorkspace";
import { SavingView } from "../SavingView";
import { StartChat } from "../StartChat";
import { TabRename } from "../TabRename";
import { TaskEndAsk } from "../TaskEnd";
import { PinItem, UpdateItem } from "../Updates";
import { VaultTab } from "../VaultTab";
import { OpenVault } from "../Vaults";
import { forgetThisLaunch } from "../regions";
import { complaints } from "./complaints.testkit";
import { BUILT_IN, DEFAULT_THEME, drawIn, drawTint, inForce, tinted } from "./theme";

vi.mock("../SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane" tabIndex={-1}>
      session {session}
    </div>
  ),
}));

configure({ asyncUtilTimeout: 5_000 });

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const CHAT = {
  session: 4,
  name: "claude 4",
  cwd: `${ALPHA}/svc`,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: null,
  unreported: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

/** A task chat 4 dispatched, with no tab of its own. */
const TASK = {
  ...CHAT,
  session: 6,
  name: "devops 6",
  in_front: false,
  persona: "devops",
  label: "talk",
  from: {
    chat: 4,
    name: "claude 4",
    workspace: "alpha",
    task: true,
    tab: false,
    reported: false,
    unreported: false,
  },
};

const workspace = (name: string, colour: string | null, chats: unknown[] = []) => ({
  name,
  path: `${PLANE}/workspaces/${name}`,
  vision: "Ship the thing",
  todos: [],
  chats,
  colour,
});

/**
 * The core, for a window with one project: alpha (in `colour`, or none) with a repo, a chat in a
 * tab and a task it dispatched, and beta beside it. The project draws `theme` everywhere.
 */
function core(theme: string, colour: string | null) {
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "plane_at_launch":
          return { plane: PLANE, from: PLANE, why: null };
        case "plane_pins":
          return { project: false, workspaces: ["alpha", "beta"], missing: [] };
        case "opened_chats":
          return [CHAT];
        case "plane_sidebar":
          return {
            root: PLANE,
            personas: ["steward", "devops"],
            persona: "steward",
            unfiled: [],
            workspaces: [workspace("alpha", colour, [CHAT, TASK]), workspace("beta", null)],
          };
        case "project_theme_drawn":
          return theme;
        case "workspace_panels":
          return {
            workspace: a.workspace,
            repos: ["svc"],
            paths: { svc: `${ALPHA}/svc` },
            absent: [],
            refused: [],
            todos: [],
            todos_refused: null,
            personas: ["steward", "devops"],
            persona: "steward",
            contributed: [],
          };
        case "workspace_repos":
          return { workspace: a.workspace, repos: [], cache_refused: null };
        case "alerts_everywhere":
          return [
            {
              plane: PLANE,
              alerts: [
                {
                  severity: "warn",
                  subject: "reinit",
                  detail: "1 workspace is behind the current layout: alpha",
                  way: { kind: "fix", id: "workspace-reinit" },
                },
                {
                  severity: "bad",
                  subject: "save",
                  detail: "the project's save is blocked: a conflict",
                  way: { kind: "saving" },
                },
              ],
              stopped: null,
            },
          ];
        case "worktree_list":
        case "reopened_views":
        case "chat_states":
        case "chats_that_would_not_start":
        case "running_sessions":
        case "owed_restarts":
        case "extension_views":
        case "extension_panels":
        case "extensions_on":
          return [];
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
  drawTint(null);
  drawIn(DEFAULT_THEME);
});

/** The window, its project open and its chat drawn, in `theme`. */
async function windowIn(theme: string, colour: string | null = null) {
  core(theme, colour);
  render(<App />);
  await waitFor(() => expect(inForce()).toBe(BUILT_IN[theme]));
  await screen.findByTestId("pane");
  await screen.findByRole("tab", { name: /alpha/ });
}

/**
 * **Known fault, in a file another lane holds this round** (#956): three elements of
 * `PlaneView.tsx` put a workspace's tint on themselves (`tintOf`) without declaring the hue it
 * is a tint of in `data-colour`, as the workspace's tab beside them does. The values are the
 * theme's own tint, but the check cannot tell them from a colour the window wrote, so it says
 * each one. They are held here by name, so a fourth such element still fails, and so does
 * the day they are declared: then this list goes. The fix is `data-colour={colourOf(workspace)
 * ?? undefined}` on each of the three.
 */
const UNDECLARED_TINT = ["<span.workspace>", "<span.workspace-mark>", "<header.bar>"];

/**
 * The complaints about `UNDECLARED_TINT`'s elements that are exactly the theme's tint in
 * `colour`, set aside; every other complaint is kept.
 */
function besideUndeclaredTint(said: string[], colour: string) {
  const tint = tinted(inForce(), colour);
  const values = new Set(Object.values(tint.values));
  const undeclared = new Set<string>();
  const rest = said.filter((one) => {
    const hit = /^(<[^>]+>) style --[\w-]+: (#[0-9a-f]{6}) is a colour literal$/.exec(one);
    if (hit === null || !UNDECLARED_TINT.includes(hit[1]) || !values.has(hit[2])) return true;
    undeclared.add(hit[1]);
    return false;
  });
  return { rest, undeclared: [...undeclared].sort() };
}

describe.each(Object.keys(BUILT_IN))("the window's chrome in %s", (theme) => {
  beforeEach(() => drawIn(BUILT_IN[theme]));

  it("draws the title bar, the strips, the explorer and the status line with tokens only", async () => {
    await windowIn(theme);

    expect(complaints(document.body)).toEqual([]);
  });

  it("draws them with tokens only with a workspace colour tinting the theme", async () => {
    await windowIn(theme, "teal");
    await waitFor(() =>
      expect(document.documentElement.style.getPropertyValue("--accent-base")).toBe(
        tinted(BUILT_IN[theme], "teal").values["accent.base"],
      ),
    );

    const { rest, undeclared } = besideUndeclaredTint(complaints(document.body), "teal");
    expect(rest).toEqual([]);
    expect(undeclared).toEqual([...UNDECLARED_TINT].sort());
  });

  it("draws the palette with tokens only", async () => {
    await windowIn(theme, "teal");
    await userEvent.keyboard("{Meta>}k{/Meta}");
    await screen.findByRole("dialog");

    expect(besideUndeclaredTint(complaints(document.body), "teal").rest).toEqual([]);
  });

  it("draws the alerts drawer with tokens only", async () => {
    await windowIn(theme, "teal");
    await userEvent.click(await screen.findByRole("button", { name: /^Alerts/ }));
    await screen.findByRole("dialog", { name: "Alerts" });

    expect(besideUndeclaredTint(complaints(document.body), "teal").rest).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------
// Every dialog, opened once.
// ---------------------------------------------------------------------------------------------

const P = PLANE as unknown as PlaneId;
const nothing = () => {};

/** A chat as the quit warning and the questions that end chats list it. */
const ENDING: Ending = {
  key: "a/4",
  name: "claude 4",
  harness: "claude",
  cwd: `${ALPHA}/svc`,
  workspace: "alpha",
  state: "running",
};

const PROFILES = [
  {
    name: "claude",
    kind: "claude" as const,
    shown: "claude",
    source: "built-in",
    is_default: true,
    ready_to_type: true,
    harness: null,
    sandbox: null,
    approval: null,
  },
];

const VAULT: VaultContents = {
  name: "ops",
  provider: "1password",
  count: 1,
  health: { ok: true, detail: "1 secret in 1Password" },
  secrets: [{ key: "API_TOKEN", size: "16–31 bytes", updated: "2026-09-24T11:32:17Z" }],
  refused: null,
  identity_unset_elsewhere: [],
  identity: [{ variable: "service-account-token", held: "keyring", kept: true }],
  identity_in_app_env: [],
};

const LAND: LandQuestion = {
  repo: "widget",
  number: 7,
  url: "https://github.com/acme/widget/pull/7",
  head: "6dcb09b5b57875f334f61aebed695e2e4193db5e",
  head_short: "6dcb09b5b578",
  through: "merge",
  forge: "github",
  request: "pull request",
  sigil: "#",
  queue: "merge queue",
  how: "purlis merges it now, at 6dcb09b5b578 and no other.",
  squash: true,
  said: ["checks passed"],
};

/** The core, for the dialogs that read something as they open. Anything else is refused, so a
 *  dialog that reads more draws its refusal, which is a state to check too. */
function dialogCore() {
  mockIPC((cmd) => {
    switch (cmd) {
      case "installed_extensions":
        return {
          extensions: [
            {
              id: "solarized",
              name: "Solarized",
              path: "/home/dev/ext/solarized",
              standing: "trusted",
              refused: null,
              themes_in_force: [],
              ask: {
                id: "solarized",
                name: "Solarized",
                path: "/home/dev/ext/solarized",
                declares: [],
                fingerprint: "a".repeat(64),
                first: false,
                runs_as_you: "runs as you do",
                fingerprint_note: "fingerprinted",
                state_note: null,
              },
            },
          ],
          built_in_themes: ["charter-dark"],
          dropped: [],
          unreadable: null,
        };
      case "reachable_repos":
        return { repos: [{ name: "svc", path: "acme/svc", description: "" }], trouble: [] };
      case "vault_open":
        return VAULT;
      case "plane_saving":
        return {
          stage: "committed",
          changed: [],
          ahead: 2,
          pr: null,
          request: "pull request",
          blocked: null,
          branch: "main",
          pushes: true,
          behind: 0,
          pushFailed: null,
          live: [],
          conflicts: [],
          notice: null,
          mode: "push",
          modeFrom: "charter.toml",
          journal: [],
        };
      case "workspace_saving":
        return [];
      case "about_charter":
        return {
          version: "0.2.0",
          build: { kind: "release" },
          profile: "release",
          notes: {
            version: "0.2.0",
            date: "2026-10-01",
            markdown:
              "### Added\n\n- **A thing.** It does `this`, and [more](https://example.com).",
          },
        };
      case "workspace_live_preview":
        return {
          live: false,
          files: ["workspaces/alpha/workspace.md"],
          remote: "origin",
          mode: "push",
        };
      case "extension_themes":
      case "extension_views":
      case "extension_panels":
        return [];
      default:
        throw new Error(`${cmd} is not answered in this test`);
    }
  });
}

/** One dialog of the window: the file it is in, and how a test opens it. */
type Opened = { name: string; file: string; open: () => Promise<unknown> };

const DIALOGS: Opened[] = [
  {
    name: "About",
    file: "About.tsx",
    open: async () => {
      render(<AboutCharter />);
      await userEvent.click(screen.getByTestId("title-about"));
    },
  },
  {
    name: "the alerts drawer",
    file: "AlertsDrawer.tsx",
    open: async () =>
      render(
        <AlertsDrawer
          open
          onOpenChange={nothing}
          reading={{
            at: "read",
            planes: [
              {
                plane: P,
                stopped: null,
                alerts: [
                  {
                    severity: "bad",
                    subject: "save",
                    detail: "the project's save is blocked: a conflict",
                    way: { kind: "saving" },
                  },
                ],
              },
            ],
          }}
          planes={[P]}
          nameOf={() => "plane"}
          does={{ openSettings() {}, openProject() {}, openSaving() {}, reread() {} }}
        />,
      ),
  },
  {
    name: "Answer",
    file: "AnswerPanel.tsx",
    open: async () =>
      render(
        <AnswerPanel
          plane={P}
          of={{ chat: 6, name: "devops 6" }}
          asking={false}
          onClose={nothing}
        />,
      ),
  },
  {
    name: "the extension prompt",
    file: "ApproveExtension.tsx",
    open: async () =>
      render(
        <ApproveExtension
          ask={{
            id: "solarized",
            name: "Solarized",
            path: "/home/dev/ext/solarized",
            declares: ["a theme"],
            fingerprint: "a".repeat(64),
            first: true,
            runs_as_you: "runs as you do",
            fingerprint_note: "fingerprinted",
            state_note: null,
          }}
          onApprove={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "the first-open prompt",
    file: "ApprovePlane.tsx",
    open: async () =>
      render(
        <ApprovePlane
          ask={{
            path: PLANE,
            first: true,
            changes: [],
            contributes: { plugins: [], env: [], starts: [], profiles: [], grants: [] },
          }}
          onApprove={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "Ask a persona",
    file: "AskPersona.tsx",
    open: async () =>
      render(
        <AskPersona
          persona="devops"
          chat="claude 4"
          workspaces={["beta"]}
          trouble="devops could not start: no profile"
          asking={false}
          onAsk={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "a task's brief",
    file: "Brief.tsx",
    open: async () =>
      render(<BriefPanel plane={P} of={{ name: "devops 6", chat: 6 }} onClose={nothing} />),
  },
  {
    name: "Push",
    file: "ChangeActions.tsx",
    open: async () =>
      render(<PushAsk plane={P} workspace="alpha" change="tokens" onClose={nothing} />),
  },
  {
    name: "Land",
    file: "ChangeActions.tsx",
    open: async () =>
      render(
        <LandAsk plane={P} workspace="alpha" change="tokens" question={LAND} onClose={nothing} />,
      ),
  },
  {
    name: "a chat's question",
    file: "ChatAsk.tsx",
    open: async () =>
      render(
        <ChatAsk
          title="Approve claude's profile"
          says="claude 4 starts with this command."
          answer="Approve"
          warns="claude 4 is in the middle of a turn."
          trouble="The profile changed since it was shown."
          busy={false}
          onAnswer={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "closing a project",
    file: "ClosingProject.tsx",
    open: async () =>
      render(
        <ClosingProject name="plane" chats={[ENDING]} heard onClose={nothing} onCancel={nothing} />,
      ),
  },
  {
    name: "deleting a vault",
    file: "DeleteVault.tsx",
    open: async () =>
      render(
        <DeleteVault
          vault="ops"
          unreadable="ops could not be read"
          trouble="ops is in use"
          deleting={false}
          onDelete={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "deleting a workspace",
    file: "DeleteWorkspace.tsx",
    open: async () =>
      render(
        <DeleteWorkspace
          workspace="alpha"
          atRisk={[]}
          deleting={false}
          onDelete={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "the doctor",
    file: "Doctor.tsx",
    open: async () => {
      render(
        <Health
          doctor={{
            running: false,
            run: nothing,
            report: {
              rows: [
                {
                  name: "git",
                  status: "ok",
                  detail: "found",
                  hint: "install git",
                  checked: true,
                  settings: null,
                  fix: null,
                },
                {
                  name: "charter.toml",
                  status: "warn",
                  detail: "plane.mod in charter.toml is not read",
                  hint: "Fix or remove it",
                  checked: true,
                  settings: "project.saving",
                  fix: null,
                },
                {
                  name: "tmux",
                  status: "fail",
                  detail: "not found",
                  hint: "install tmux",
                  checked: true,
                  settings: null,
                  fix: null,
                },
              ],
              app_rows: [],
              full: false,
              path: "/usr/bin:/bin",
            },
          }}
          onOpenSettings={nothing}
        />,
      );
      await userEvent.click(screen.getByTestId("status-doctor"));
    },
  },
  {
    name: "the question before a chat ends",
    file: "EndingChat.tsx",
    open: async () =>
      render(
        <EndingChat
          offer={{
            id: "tab.close:1",
            title: "End chat claude 4",
            available: true,
            reason: "",
            does: { verb: "closeTab", tab: 1, ends: true },
            note: ENDS_IT,
          }}
          smart={{ available: true, why: null, close_first: false }}
          running={["devops 6"]}
          closing={["steward 2"]}
          back={["web 5"]}
          ownTabs={1}
          onEnd={nothing}
          onSmartClose={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "an extension action that asks first",
    file: "ExtensionAction.tsx",
    open: async () =>
      render(
        <AskFirst
          extension="persona-statistics"
          action={{ id: "drop", title: "Drop…", asks_first: true, deletes: true }}
          onRun={async () => ({ refused: "persona-statistics refused: busy" })}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "the extensions",
    file: "Extensions.tsx",
    open: async () => {
      render(<Extensions onClose={nothing} />);
      await screen.findByRole("button", { name: "Review" });
    },
  },
  {
    name: "linking a work item",
    file: "LinkWorkItem.tsx",
    open: async () =>
      render(
        <LinkWorkItem
          chat="claude 4"
          linked="#12"
          trouble="#99 is not a work item of this project"
          linking={false}
          onLink={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "making a workspace live",
    file: "LiveDialog.tsx",
    open: async () =>
      render(<LiveDialog plane={P} workspace="alpha" onClose={nothing} onDone={nothing} />),
  },
  {
    name: "a new branch",
    file: "NewBranch.tsx",
    open: async () =>
      render(
        <NewBranch
          repo="svc"
          trouble="fix-it already exists"
          making={false}
          onCut={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "a new persona",
    file: "NewPersona.tsx",
    open: async () =>
      render(
        <NewPersona
          plane={PLANE}
          trouble="devops already exists"
          making={false}
          onCreate={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "a new project",
    file: "NewProject.tsx",
    open: async () => {
      render(
        <NewProject
          trouble="the folder is not empty"
          making={false}
          onCreate={nothing}
          onOpenRepo={nothing}
          opening={false}
          onCancel={nothing}
        />,
      );
      await userEvent.click(screen.getByText("Advanced"));
    },
  },
  {
    name: "a new vault",
    file: "NewVault.tsx",
    open: async () => {
      render(
        <NewVault
          plane={PLANE}
          trouble="ops already exists"
          making={false}
          onCreate={nothing}
          onMade={nothing}
          onCancel={nothing}
        />,
      );
      await userEvent.click(screen.getByRole("radio", { name: "1Password" }));
    },
  },
  {
    name: "a new workspace",
    file: "NewWorkspace.tsx",
    open: async () => {
      render(
        <NewWorkspace
          plane="plane"
          planeId={P}
          trouble="alpha already exists"
          making={false}
          onCreate={nothing}
          onCancel={nothing}
        />,
      );
      await userEvent.type(screen.getByLabelText("Name"), "svc");
      await screen.findByRole("checkbox", { name: "svc" });
    },
  },
  {
    name: "the palette",
    file: "Palette.tsx",
    open: async () => {
      render(<Palette offers={[]} onRun={() => ({ ok: true }) as const} />);
      await userEvent.keyboard("{F2}");
    },
  },
  {
    name: "a list row's card",
    file: "PanelList.tsx",
    open: async () => {
      function List() {
        const [open, setOpen] = useState<string>();
        return (
          <PanelList
            rows={[
              {
                key: "a",
                text: "Review the rollout plan",
                note: "2026-09-20",
                mark: "todo",
                tone: "trouble",
                detail: { kind: "text", text: "The whole of the rollout plan." },
                runs: null,
                actions: [],
              },
            ]}
            empty={{ headline: "Nothing here", body: null, offer: null }}
            label="Todos"
            testid="list"
            open={open}
            onOpen={setOpen}
          />
        );
      }
      render(<List />);
      await userEvent.click(screen.getByRole("button", { name: /Review the rollout plan/ }));
    },
  },
  {
    name: "a persona's profile",
    file: "PersonaProfile.tsx",
    open: async () =>
      render(
        <PersonaProfile
          persona="devops"
          unreadable="devops.md could not be read"
          trouble="the profile is not one of the project's"
          saving={false}
          onSave={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "the quit warning",
    file: "QuitWarning.tsx",
    open: async () => render(<QuitWarning chats={[ENDING]} onQuit={nothing} onCancel={nothing} />),
  },
  {
    name: "the ask about a chat mid-turn",
    file: "QuitWarning.tsx",
    open: async () => render(<MidTurn chats={[ENDING]} onWait={nothing} onRestart={nothing} />),
  },
  {
    name: "the question a relaunch asks",
    file: "RelaunchAsk.tsx",
    open: async () =>
      render(
        <RelaunchAsk
          question={{ projects: [{ plane: PLANE, chats: 2, views: 1 }], after_update: true }}
          nameOf={() => "plane"}
          onAnswer={nothing}
        />,
      ),
  },
  {
    name: "removing a repo from a workspace",
    file: "RemoveFromWorkspace.tsx",
    open: async () =>
      render(
        <RemoveFromWorkspace
          plane={P}
          workspace="alpha"
          repo="svc"
          onClose={nothing}
          onDone={nothing}
        />,
      ),
  },
  {
    name: "removing a persona",
    file: "RemovePersona.tsx",
    open: async () =>
      render(
        <RemovePersona
          persona="devops"
          trouble="devops has chats open"
          deleting={false}
          onDelete={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "renaming a workspace",
    file: "RenameWorkspace.tsx",
    open: async () =>
      render(
        <RenameWorkspace
          workspace="alpha"
          startsFresh="claude 4 starts a fresh conversation after the rename."
          trouble="beta already exists"
          renaming={false}
          onRename={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "Save all",
    file: "SavingView.tsx",
    open: async () => {
      render(<SavingView plane={PLANE} workspace="alpha" />);
      await userEvent.click(await screen.findByRole("button", { name: "Save all" }));
    },
  },
  {
    name: "the chat picker",
    file: "StartChat.tsx",
    open: async () =>
      render(
        <StartChat
          options={{
            profiles: PROFILES,
            refused: [["broken", "declares no command"]],
            personas: ["steward"],
            persona: "steward",
            persona_profiles: {},
            ignore_fix: null,
            ignore_fix_id: null,
            declares_none: false,
          }}
          trouble="claude did not start"
          onStart={nothing}
          onApprove={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "a tab's name refused",
    file: "TabRename.tsx",
    open: async () => {
      render(
        <TabRename name="claude 4" onSave={async () => "That name is taken"} onDone={nothing} />,
      );
      const box = await screen.findByRole("textbox");
      await userEvent.clear(box);
      await userEvent.type(box, "web{Enter}");
    },
  },
  {
    name: "ending a task",
    file: "TaskEnd.tsx",
    open: async () =>
      render(
        <TaskEndAsk
          asked={{
            name: "devops 6",
            working: true,
            no_report: null,
            below: ["web 7"],
            stopping: false,
            reported: false,
            session: 6,
            belowToo: false,
            busy: false,
            trouble: "devops 6 could not be told",
          }}
          onBelow={nothing}
          onAnswer={nothing}
          onCancel={nothing}
        />,
      ),
  },
  {
    name: "the update offer",
    file: "Updates.tsx",
    open: async () => {
      render(
        <UpdateItem
          updates={{
            state: {
              kind: "offered",
              offer: { version: "0.2.0", current: "0.1.0", channel: "stable", notes: "Fixes." },
            },
            channel: "stable",
            check: nothing,
            install: nothing,
            choose: nothing,
            restart: nothing,
          }}
        />,
      );
      await userEvent.click(screen.getByTestId("status-update"));
    },
  },
  {
    name: "the pin drift",
    file: "Updates.tsx",
    open: async () => {
      render(
        <PinItem
          pin={{
            drift: true,
            pinned: "0.1.0",
            brought: "0.2.0",
            said: ["the plane pins an older charter"],
          }}
          again={nothing}
        />,
      );
      await userEvent.click(screen.getByTestId("status-pin"));
    },
  },
  ...(
    [
      [
        "Add secret",
        async () => userEvent.click(await screen.findByRole("button", { name: "Add" })),
      ],
      ["Edit value", () => fromTheMenuOf("API_TOKEN", "Edit value")],
      ["Rename", () => fromTheMenuOf("API_TOKEN", "Rename")],
      ["Delete", () => fromTheMenuOf("API_TOKEN", "Delete")],
      [
        "how it signs in",
        async () =>
          userEvent.click(
            await screen.findByRole("button", { name: "Change how this vault signs in" }),
          ),
      ],
    ] as const
  ).map(([what, press]): Opened => ({
    name: `the vault tab's ${what}`,
    file: "VaultTab.tsx",
    open: async () => {
      render(<VaultTab plane={PLANE} vault="ops" onChanged={nothing} />);
      await press();
    },
  })),
  {
    name: "Open vault",
    file: "Vaults.tsx",
    open: async () =>
      render(
        <OpenVault
          vaults={[
            {
              name: "ops",
              provider: "keyring",
              count: 2,
              health: { ok: true, detail: "2 secrets" },
            },
            {
              name: "team",
              provider: "1password",
              count: null,
              health: { ok: false, detail: "no token" },
            },
          ]}
          offers={new Map()}
          onPress={nothing}
          onCancel={nothing}
        />,
      ),
  },
];

/** Opens the menu of the vault tab's row for `key` and picks `item`. */
async function fromTheMenuOf(key: string, item: string) {
  await userEvent.click(await screen.findByRole("button", { name: key }));
  await userEvent.click(await screen.findByRole("menuitem", { name: item }));
}

/** The dialogs open on the page: a dialog, an alert dialog, or a popover's content, which its
 *  popper wraps whatever role it has (a refused tab name's is an alert). */
const openDialogs = () =>
  document.querySelectorAll(
    '[role="dialog"], [role="alertdialog"], [data-radix-popper-content-wrapper]',
  );

/**
 * Every source file that draws a dialog: one that uses Radix's dialog, alert dialog or popover,
 * or writes the role itself. Read from the files, so a dialog added tomorrow is held here.
 */
const FILES_WITH_DIALOGS = Object.keys(import.meta.glob(["../**/*.tsx", "!../**/*.test.tsx"]))
  .map((path) => path.slice("../".length))
  .filter((path) =>
    /from "@radix-ui\/react-(?:alert-dialog|dialog|popover)"|role="(?:alert)?dialog"/.test(
      readFileSync(join(process.cwd(), "src", path), "utf8"),
    ),
  )
  .sort();

describe("every dialog is opened here", () => {
  it("has a way in for every file that draws one", () => {
    expect(FILES_WITH_DIALOGS.length).toBeGreaterThan(30);
    const here = new Set(DIALOGS.map((one) => one.file));
    expect(FILES_WITH_DIALOGS.filter((file) => !here.has(file))).toEqual([]);
  });
});

describe.each(Object.keys(BUILT_IN))("every dialog in %s", (theme) => {
  beforeEach(() => {
    drawIn(BUILT_IN[theme]);
    dialogCore();
  });

  it.each(DIALOGS)("draws $name with tokens only", async (dialog) => {
    await dialog.open();
    await waitFor(() => expect(openDialogs().length).toBeGreaterThan(0));

    expect(complaints(document.body)).toEqual([]);
  });
});
