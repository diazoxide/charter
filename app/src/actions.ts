/**
 * Every action the window can do, in one list.
 *
 * **The palette invents no command list of its own, and neither does the bar.** The tmux
 * frame learned this the expensive way: `charter/frame/palette.py` builds its rows out of
 * `frame/actions.py`'s offers and nothing else, because the menu it replaced had grown a
 * second answer to "how do I do a thing" and the two answers drifted. Here the same rule is
 * kept by making :func:`catalogue` the only place an action is written down — the header's
 * buttons are a view of it, filtered to the few ids that have earned a permanent place on
 * the bar, and the palette is a view of all of it. Delete a row here and the button goes
 * with it, which is what a test asserts.
 *
 * **An offer is DATA, not a closure.** What a row does is a `Does` — a value naming the verb
 * and what it is about — and the window turns that into work through :func:`perform`, in an
 * event handler. Two things fall out of that. The catalogue is a pure function of the window
 * as it stands, so the whole of it can be asserted on without a window; and no row can be
 * holding a stale copy of the arrangement it was built from, because a row holds no copy of
 * anything.
 *
 * **An action that cannot run right now is listed WITH ITS REASON.** It is never dropped: an
 * operator cannot ask about an option they cannot see. `available` and `reason` are two
 * fields rather than one derived from the other, so a row that says the wrong thing is a
 * defect here and not an ambiguity in the surface drawing it.
 *
 * **A refusal is a value, not a throw.** `perform` answers a `Ran`, so a refusal the core
 * gave travels to the operator as the core's own sentence rather than as whatever a `catch`
 * decided to say about it.
 */
import type {
  ChatWorktree,
  Curations,
  ExtensionCommand,
  ExtensionView,
  FinishedTask,
  HarnessGlance,
  MemoryScope,
  PanelBlock,
  RowAction,
  SubjectCurations,
} from "./bindings";
import { activityTitle, activityView } from "./activity";
import { chatNetworkTitle, chatNetworkView } from "./chatNetwork";
import { DISPATCHES_TITLE, DISPATCHES_VIEW } from "./dispatches";
import { backSaid } from "./chatState";
import { handedOff, tasksOf, type ListedChat } from "./chatsTree";
import { MAIN } from "./here";
import {
  DRAFT,
  MEMORY_VIEW,
  SHARED_MEMORY_TITLE,
  SHARED_MEMORY_VIEW,
  archiveTitle,
  archiveView,
  archiveWhere,
  memoryKey,
  memoryOf,
  memoryRefOf,
  scopeKey,
  type MemoryRef,
} from "./memories";
import { publishedSaid } from "./memoryMoves";
import { searchFromFocus, type SearchAsk } from "./contentSearch";
import { pieceFilesTitle, pieceFilesView, type Place } from "./pieceViews";
import { searchKeySaid } from "./searchKey";
import { SESSION_VIEW, sessionTitle, sessionTitleOf, sessionView } from "./sessions";
import { shellKeySaid } from "./shellKey";
import { switcherKeySaid } from "./switcherKey";
import { askedByOf, chatsOfTab } from "./tabChats";
import { neighbour } from "./tabTasks";
import { taskKeyNote, taskKeySaid } from "./taskKeys";
import { todoOpenId, todoView } from "./todos";
import { onAMac } from "./tabKeys";
import type { RegionId } from "./regions";
import { ATTENTION_VIEWS, VIEWS, type OwnViewId, type PanelViewId, type ViewId } from "./sideViews";
import { SIDE_KEYS_SAID } from "./sideKeys";
import { SETTINGS_GROUPS } from "./settings/catalogue";
import { profilePage } from "./settings/profileAddress";
import { askSettingsLink, linkToGroup, settingsPlace, type SettingsLink } from "./settings/links";
import {
  changesTitle,
  changesView,
  chatOf,
  contentsOf,
  focusedChat,
  focusedContent,
  harnessCardView,
  panesOf,
  placedOf,
  sessionOf,
  viewKey,
  type AskedBy,
  type Direction,
  type Tabs,
  type ViewRef,
} from "./tabs";

/**
 * **Where a refusal is put right, when that is a setting** (#1201, SE-22): the words of the link
 * and the Settings group it opens. "Choose your editor in Settings first." names a setting, and
 * a sentence that names one without a way to it leaves the person to find it.
 */
export type SettingsWay = { label: string; link: SettingsLink };

/** A finished task's Merge… on its row's menu (#1534), by its dispatch's id. */
export const taskMergeId = (id: string) => `task.merge:${id}`;
/** A finished task's Discard branch… on its row's menu (#1534), by its dispatch's id. */
export const taskDiscardId = (id: string) => `task.discard:${id}`;

/** The row that asks the focused workspace's refused reads again (#1244). */
export const READ_AGAIN = "workspace.readagain";

/** What running an action answered: one line to say, or a refusal in the words it came in —
 *  with the Settings group that puts it right, where a setting does. */
export type Ran =
  { ok: true; said?: string } | { ok: false; refused: string; settings?: SettingsWay };

/**
 * **What the window says about the last action** — the line under the strip, and the palette's
 * line while it is up: one state drawn in two places. `settings` is a refusal's way to the
 * setting that puts it right (#1201), drawn as a link beside the words, and either line
 * follows it.
 */
export type Said = { from: string; refused: boolean; words: string; settings?: SettingsWay };

/** What the window says of a row's answer: nothing for a quiet success. */
export function saidOf(from: string, answer: Ran): Said | undefined {
  if (answer.ok) return answer.said ? { from, refused: false, words: answer.said } : undefined;
  return answer.settings === undefined
    ? { from, refused: true, words: answer.refused }
    : { from, refused: true, words: answer.refused, settings: answer.settings };
}

/**
 * The key the palette claims, and the key it can hand back.
 *
 * `F2` is the tmux frame's own key and it is claimed on the window, capture-phase, so the
 * pane's terminal never sees it (`Palette.opensIt`). An operator whose harness binds `F2`
 * therefore had no way to send it — no chord, no second press, no setting (charter-app#47).
 *
 * **The way out is the frame's own idiom, not a new one.** tmux answers the same question
 * with `send-prefix`: press the prefix twice and the second one goes through. So the second
 * `F2` closes the palette and delivers `F2` to the chat in front, and the row below is the
 * same thing with a name, so it can be browsed and typed for rather than only known.
 */
export const PASS_THROUGH_KEY = "F2";

/** The row that sends it, looked up by id wherever the keystroke is handled. */
export const PASS_THROUGH_ID = "pane.sendkey";

/**
 * What a terminal sends for that key, so the second press delivers exactly what the first
 * one swallowed.
 *
 * `ESC O Q` (SS3 Q) is what xterm.js itself sends for an unmodified `F2` — its
 * `evaluateKeyboardEvent` maps key code 113 with no modifier to `C0.ESC + "OQ"`. Read out of
 * the version this app depends on rather than off a table, because the claim is not "this is
 * F2 in VT100" but "this is what the pane would have sent".
 */
export const PASS_THROUGH_BYTES = "\u001bOQ";

/**
 * The mark a pane puts on itself to say the chat has the keyboard in here.
 *
 * **It is what makes "whose key is this?" answerable at all.** The window claims its keys on
 * the window, capture-phase, which is before the focus has had any say — so the only thing a
 * listener up there can ask about the chat is where the keystroke was DELIVERED. xterm reads
 * from its own textarea, and that textarea is a descendant of the pane holding it, so a
 * keydown inside a marked element is a keydown the operator aimed at a shell
 * (`Palette.theChatKeepsIt`, charter-app#106).
 *
 * An attribute rather than the pane's class, because the class is how the pane is DRAWN and
 * this is what it MEANS: a rule that reads `.pane` is one restyling away from being wrong.
 */
export const CHAT_KEYBOARD = "data-chat-keyboard";

/**
 * The mark a chat's tab puts on itself to say `F2` renames it there (charter-app#254).
 *
 * `F2` is the platform's rename key for a focused item — and it is also the palette's, claimed
 * on the window, capture-phase, from anywhere (`Palette.opensIt`). **A focused chat tab is the
 * one place the palette stands back**, for the reason `CHAT_KEYBOARD` gives about a pane: the
 * key means something else where it landed. The palette is still `⌘K` from the tab, and `F2`
 * from anywhere else. The rename box carries it too, so `F2` typed into a name opens nothing. An attribute rather than the tab's role, because the role is what the tab
 * IS and this is what the key MEANS on it: a view's tab is a tab too, and has no rename.
 */
export const RENAMES_ON_F2 = "data-renames-on-f2";

/**
 * The **plane root**: the workspace strip's first tab, always drawn (SI-1), and the strip every
 * chat working in no workspace appears on.
 *
 * It began as the strip for chats outside every workspace, drawn only when there were some:
 * the sidebar has always shown those chats rather than dropping them, and a strip that shows
 * one workspace's chats has to have somewhere to put them (charter-app#130). The operator's
 * ruling made it permanent, because the plane root is where a chat that looks after the plane
 * itself — its personas, its settings, its workspaces — works. It is not a workspace: it has
 * no `workspace.md`, no memory and no todos, and a chat started on it is told so
 * (`$CHARTER_PLANE_ROOT_SESSION`).
 *
 * The internal name stays what it was, because it is still exactly what it holds: a chat
 * whose directory is in no workspace. Slashes, because this stands where a workspace name
 * stands and a workspace name is a directory name: no directory can contain one, so it can
 * never collide with a real workspace. It never reaches the operator — `catalogue` gives its
 * rows their own words.
 */
export const OUTSIDE = "outside/every/workspace";

/** What the palette, a menu and a screen reader call it: the glossary's **plane root**. */
export const OUTSIDE_TITLE = "Plane root";

/** The root tab's tooltip — the operator's words, exactly (SI-1). The tab draws only an icon. */
export const ROOT_TIP = "Project — chats here start at the project root";

/** The key that opens a shell tab, as this platform spells it — said on the row, so the palette
 *  is where an operator learns it (`shellKey.ts`). */
export const SHELL_KEY_SAID = shellKeySaid(onAMac());

/** The key that opens a Search tab, as this platform spells it — said on *Search in files*'s
 *  row, for the same reason (`searchKey.ts`, #1137). */
export const SEARCH_KEY_SAID = searchKeySaid(onAMac());

/** The key that opens the project switcher, as this platform spells it — said on its row, for
 *  the same reason (`switcherKey.ts`, FR-27). */
export const SWITCHER_KEY_SAID = switcherKeySaid(onAMac());

/** What a row does, as a value the window can carry out. */
export type Does =
  | { verb: "chat.new" }
  /** Opens a plain shell tab (SI-5): the operator's own `$SHELL`, no harness, no profile —
   *  in `workspace`'s directory and filed under it, or where a new chat would start when it
   *  names none. Nothing asks first: a shell starts no harness, so ADR 0022's picker has
   *  nothing to ask about. */
  | { verb: "newShell"; workspace?: string }
  /** Opens the picker for a new tab whose chat starts in `path` — the plane root's own row
   *  (SI-1). It starts nothing by itself; the picker is the only path ADR 0022 admits. */
  | { verb: "newChatIn"; path: string }
  | { verb: "split"; direction: Direction }
  /** Hands a key the palette claimed to the chat in front, rather than swallowing it. */
  | { verb: "sendKey"; key: string }
  /** `ends` is whether carrying it out ends a chat — a pane showing a view closes and kills
   *  nothing, and only a row that ends a chat is asked about first (`PlaneView.ENDS_A_CHAT`). */
  | { verb: "closePane"; ends: boolean }
  | { verb: "closeTab"; tab: number; ends: boolean }
  | { verb: "selectTab"; tab: number }
  /** Opens the name of a chat's tab for editing, in place on the strip (charter-app#254). */
  | { verb: "renameTab"; tab: number }
  /** Asks which work item a chat works on, and links it (V60, ADR 0088). */
  | { verb: "linkWorkItem"; tab: number }
  /** Ends a chat's work link. */
  | { verb: "unlinkWorkItem"; tab: number }
  /** Asks, then starts a chat again on the project's instructions as they are now (NO-3): the
   *  same chat, in a new run with no conversation resumed. */
  | { verb: "startFresh"; tab: number }
  | { verb: "restartChat"; tab: number }
  /** Restart chat on a row of the Chats list, for a chat with no tab (#1462): by its number. */
  | { verb: "restartListed"; session: number }
  /** Opens the dialog that asks a persona for something from a chat's tab. It starts nothing
   *  by itself: the dialog's answer does, through `ask_persona_chat`. */
  | { verb: "askPersona"; tab: number; persona: string }
  /** Pins or unpins a chat, a workspace or a project (ADR 0039).
   *
   *  Three verbs and not one, because they are three stores: a project's pin and a
   *  workspace's go in the machine store and a chat's goes in the plane's own app record
   *  (ADR 0040). A design that treated "pin" as one thing would discover that in review. */
  | { verb: "pinTab"; tab: number; pinned: boolean }
  | { verb: "pinWorkspace"; workspace: string; pinned: boolean }
  | { verb: "pinProject"; plane: string; pinned: boolean }
  | { verb: "focusWorkspace"; workspace: string }
  /** Asks for a new workspace. It creates nothing by itself: the name, and the validation the
   *  CLI applies to it, are the dialog's and the core's (`workspace_create`). */
  | { verb: "createWorkspace" }
  /** Asks to delete one workspace and everything in it.
   *
   *  **It deletes nothing by itself, and it carries no `force`.** The core's guard
   *  (`wscmd::work_at_risk`) is what decides, in `workspace_remove`; the dialog shows what is
   *  at risk, the first press is refused when work would be discarded, and forcing is a
   *  second press on a sentence the operator has read. A `force` here would be a row that
   *  discards work with nobody warned — the same objection `worktree.discard` records. */
  | { verb: "removeWorkspace"; workspace: string }
  /** `inside` is the next and the previous chat in a tab (V100-36): the tab is switched to it
   *  whatever the person set for a pressed task, so the key never gives a task a tab. */
  | { verb: "showChat"; session: number; inside?: true }
  /** Gives a task a tab of its own (#1489). Nothing ends, and nothing is asked. */
  | { verb: "ownTab"; session: number }
  /** Opens a task beside the session that asked for it, inside that session's tab (#1489). */
  | { verb: "beside"; session: number }
  /** Sends a task back out of its own tab or its pane (#1489): the minimise. **It ends
   *  nothing**, so it is never asked about, and it is not a close. */
  | { verb: "sendBack"; session: number }
  /** Opens the menu of the chats that live in a tab: its session's and its tasks' (#1487).
   *  It shows nothing by itself: a row of that menu does. */
  | { verb: "showTabTasks"; tab: number }
  /** Opens the Brief panel of a task (#1494): the brief it was sent, read back from its
   *  dispatch record. It reads, and changes nothing. */
  | { verb: "showBrief"; session: number }
  /** Opens the form that answers the question task `session` put to its asking chat (#1496,
   *  #1551). It sends nothing by itself: the person's Send in the form does. */
  | { verb: "answerQuestion"; session: number }
  /** Drops a chat's request for the operator until it asks again (charter-app#248). The chat
   *  itself is untouched; the core holds the ignore, so the window's queue is told, not kept. */
  | { verb: "ignoreNeedsYou"; session: number }
  /** Cancels a chat's smart close (ADR 0064): nothing is sent to it and nothing is closed — the
   *  chat stays open and running, and a record it writes afterwards closes nothing. */
  | { verb: "cancelSmartClose"; session: number }
  /** Takes a chat off the needs-you list where its Smart close stopped without a record
   *  (SI-8f). Nothing is asked of the core: the list's entry is the window's own. */
  | { verb: "dismissStopped"; session: number }
  /** Asks to stop a chat, or a chat and every chat `below` it (#1448). **It stops nothing by
   *  itself**: the window asks first, and the core's `stop_chat` is what ends anything. Only a
   *  person's press reaches it: no chat has a way to this verb. */
  | { verb: "stopChat"; session: number; below: boolean }
  /** Asks to stop every task at work below a session, and keep the session (#1498, V100-53).
   *  **It stops nothing by itself**: the window asks once, naming how many, and the core's
   *  `stop_all_tasks` is what ends anything, each task the way Stop and get its report does. */
  | { verb: "stopAllTasks"; session: number }
  /** Ends a task one of the two ways a person ends one (#1488, V100-5): `report` is Stop and
   *  get its report, `now` is Close now. **It ends nothing by itself**: the window asks the
   *  core what ending it would do, asks the person where the task is mid-turn or has tasks at
   *  work below it, and the core's `end_task` is what ends anything. Only a person's press
   *  reaches it: no chat has a way to this verb. */
  | { verb: "endTask"; session: number; way: TaskEndWay }
  /** Opens a view in a tab of its own, or brings forward the tab already showing it.
   *
   *  **One verb for charter's views and an extension's** — the persona view is
   *  `{ from: null, view: "persona", key }` and persona statistics is the extension's id and
   *  its view's. It reads and changes nothing by itself: what an extension's view shows is
   *  asked of its program when the tab draws it, through the core's gate. A persona's row is
   *  here rather than only a click on the panel because a menu is a third reader of this list
   *  (`Menus.tsx`) and the persona rows had nothing in it to read (charter-app#174). */
  | { verb: "openView"; view: ViewRef; title: string }
  /** Resumes a session from its record (SI-8d): a NEW chat in the record's place, on its
   *  harness, given its conversation where it can be, and told the record in its briefing. It
   *  starts a chat, so it answers a `Ran` — the core can refuse. */
  | { verb: "resumeSession"; path: string }
  /** Opens a memory's tab (SI-9b, ADR 0065): in the strip's preview tab, replacing what it
   *  previewed — or, `keep`, as a tab of its own that nothing replaces (a double-click). */
  | { verb: "openMemory"; ref: MemoryRef; title: string; keep: boolean }
  /** Opens a memory's tab, kept, in edit mode: starting an edit keeps a preview (ADR 0065 Q1). */
  | { verb: "editMemory"; ref: MemoryRef; title: string }
  /** The window's Delete for a memory: it moves to its store's `archive/`, its tab closes, and
   *  an Undo is offered for a few seconds (ADR 0065 Q8). Nothing asks first, because nothing
   *  is lost: Undo, or `unarchive` on the command line, puts it back. */
  | { verb: "archiveMemory"; ref: MemoryRef; title: string }
  /** Moves a memory whole to the store `to` (KN-3, #1190): the tab's own Move, from a row's
   *  menu or the palette, with the same Undo. Nothing asks first, as the tab's Move does not:
   *  the row says who reads the store it goes to, and Undo moves it back. */
  | { verb: "moveMemory"; ref: MemoryRef; title: string; to: MemoryScope }
  /** Opens a new memory's tab in edit mode, for the store `scope` (ADR 0065 Q9). Nothing is
   *  written until it is saved. */
  | { verb: "newMemory"; scope: MemoryScope }
  /** Keeps a preview tab (SI-9b): the next single click previews in a tab of its own. */
  | { verb: "keepTab"; tab: number }
  /** Runs an extension's action on nothing in particular — a palette command's (charter-app#341).
   *
   *  **It runs nothing by itself when the action asks first**: the window asks, and the core
   *  refuses an action that asks first without the operator's yes, so a surface that forgot
   *  is a refusal rather than a delete. `name` is the extension's, for the question's words. */
  | { verb: "runAction"; extension: string; action: RowAction; name: string }
  /** Asks which of the plane's vaults to open (charter-app#235). It opens nothing by itself:
   *  what the picker's row runs is that vault's own `vault.open:<name>`. */
  | { verb: "pickVault" }
  /** Asks for a new vault's name and provider. It makes nothing by itself: what may be called
   *  what is `charter vault add`'s to say, through `vault_create`. */
  | { verb: "createVault" }
  /** Asks to delete one vault (SI-3). It deletes nothing by itself: the dialog lists what the
   *  vault holds and takes the vault's name typed back before `vault_remove` runs. */
  | { verb: "removeVault"; vault: string }
  /** Asks for a new persona's name, role and routing line. It makes nothing by itself: what a
   *  persona may be called and what it needs is `charter persona create`'s to say, through
   *  `persona_create`. */
  | { verb: "createPersona" }
  /** Opens a persona's `persona.md` in whatever the system opens a `.md` file with. charter
   *  has no editor of its own for one, and the core finds the file from the name. */
  | { verb: "editPersona"; persona: string }
  /** Asks which profile one persona's chats start on (#1445). It writes nothing by itself: the
   *  dialog lists the project's profiles, and `persona_set_profile` refuses any other. */
  | { verb: "setPersonaProfile"; persona: string }
  /** Asks to delete one persona. It deletes nothing by itself: the dialog says what goes, and
   *  `persona_remove` refuses one another persona still extends or uses. */
  | { verb: "removePersona"; persona: string }
  /** Closes one of the focused workspace's todos as done: the journal records it first. */
  | { verb: "closeTodo"; workspace: string; slug: string }
  /** Drops one of the focused workspace's todos with nothing journalled. */
  | { verb: "forgetTodo"; workspace: string; slug: string }
  /** **Names the worktree it acts on**, and never "whichever one is in front".
   *
   *  It used to carry only `force`, which made `worktree.remove` a row about the chat in
   *  front and left the explorer's own rows with nothing to offer (charter-app#174). The
   *  front chat's row still exists and still says "this chat's" — it now simply spells out
   *  the piece it means, the same way `tab.close:<id>` spells out its tab. */
  | { verb: "removeWorktree"; cut: Cut; force: boolean }
  | { verb: "mergeWorktree"; cut: Cut }
  /** Records the piece `done` in its log, as `charter worktree done` run inside it would
   *  (charter#368). Not destructive: the tree and the branch are left as they are. */
  | { verb: "declareWorktreeDone"; cut: Cut }
  /** Focuses the explorer on one branch, which turns it into that branch's cockpit (FM-5): its
   *  state, its chats and its files. It changes nothing on disk. */
  | { verb: "focusBranch"; cut: Cut }
  | { verb: "focusRepo"; repo: string }
  /** Makes a clone the spot the next chat starts in — the explorer's pick, one level up from a
   *  piece (charter-app#174). It starts nothing: the picker still asks, and the core still
   *  decides whether that directory can be started in. The path is the one the core spelled. */
  | { verb: "pickClone"; repo: string; path: string }
  /** A new tab whose chat starts in that clone — this one tab. The explorer's pick is left as
   *  it was, so the NEXT plain `New tab` starts where it would have; making the clone the spot
   *  for every chat after is `pickClone`'s, and the two rows must not do the same thing. */
  | { verb: "newTabIn"; repo: string; path: string }
  /** Opens the dialog that cuts a new branch in that clone (GL-1, ADR 0072 §4). The dialog
   *  names the branch; the core cuts it, and nothing is started. */
  | { verb: "newBranch"; repo: string }
  /** Clones repos the workspace names and this machine has not cloned, one after another,
   *  through the clone path Settings › Repos uses (`clone_repo`, #1215). Where each lands is
   *  the core's answer; credentials are git's and the forge's, never asked for here. */
  | { verb: "cloneMissing"; workspace: string; repos: string[] }
  /** Asks whether to take a repo the workspace names, and this machine has not cloned, out of
   *  the workspace (#1228). It writes nothing by itself: the question's yes calls
   *  `drop_repo_membership`, which drops only the repo's row in `workspace.json`. */
  | { verb: "askDropMembership"; workspace: string; repo: string }
  /** Shows the opener, so another project can be opened into this window beside the ones it
   *  already holds. It opens nothing by itself — the trust gate is the opener's (ADR 0035). */
  | { verb: "openProject" }
  /** Shows the dialog that makes a NEW project — a plane charter scaffolds.
   *
   *  It scaffolds nothing by itself, for `openProject`'s reason one step further on: what is
   *  written is `scaffold::init`'s and the open that follows is `Planes::open_if_approved`'s.
   *  **A plane charter has just created is still opened through the gate** (ADR 0035), so the
   *  first open of it raises the same trust dialog any other project's would. */
  | { verb: "createProject" }
  /** Shows what has contributed what to this window: charter's own themes, and every
   *  extension this machine has, with what each is contributing right now (ADR 0041).
   *  It puts nothing in force by itself — an extension contributes only once it is approved,
   *  and the approval is the dialog's. */
  | { verb: "showExtensions" }
  /** Shows a view of a side and gives it the keyboard (#1673): what its key does. */
  | { verb: "showSideView"; view: ViewId }
  /** Puts a region away or brings it back, as its status-line toggle and ⌘B do. */
  | { verb: "toggleRegion"; region: RegionId }
  /** Puts the app's own `charter` on a terminal's `PATH` — VS Code's "Install 'code' command
   *  in PATH". Only ever on this row: nothing links a command anywhere behind the operator's
   *  back (spec decision 21, `purlis_core::clipath`). A refusal comes back as the core's
   *  sentence: somebody else's `charter` already there, a cancelled password prompt, or a
   *  platform where the installer already did it. */
  | { verb: "installCli" }
  /** Brings a project this window already holds to the front. Nothing is opened, nothing is
   *  closed, and the project that was in front keeps every chat it had running. */
  | { verb: "selectProject"; plane: string }
  /** Opens the project switcher (FR-27): the palette, listing only the projects this window
   *  holds, the last one the operator was in first. It switches nothing by itself; a row in it
   *  is a `selectProject`. */
  | { verb: "switchProject" }
  /** Lets go of one project, which ends its chats and takes its tab out. Nothing of the
   *  project on disk goes. */
  | { verb: "closeProject"; plane: string }
  /** Moves a project into another OS window — a new one when `to` is null, the main window
   *  when it is `"main"` (charter#126). Nothing is closed and nothing is started: its chats go
   *  on running, and the window it arrives in draws them. */
  | { verb: "moveProject"; plane: string; to: string | null }
  /** Opens Settings at that project's Project level (SE-19; charter-app#252) — bringing the project to the
   *  front first when it is not. It writes nothing by itself: a save is the tab's, through the
   *  core's own checks. */
  | { verb: "openSettings"; plane: string }
  /** Opens that project's Saving tab (charter-app#294) — bringing the project to the front
   *  first when it is not. It saves nothing by itself: the save is the tab's button. */
  | { verb: "openSaving"; plane: string }
  /** Opens Settings at that workspace's level (SE-20; charter-app#280), on that workspace's
   *  strip. It writes nothing by itself: a write is the tab's, through the core's own checks. */
  | { verb: "openWorkspaceSettings"; workspace: string }
  /** Asks whether to make that workspace LIVE or LOCAL (charter-app#301): a confirmation that
   *  says what it publishes and where. Nothing changes until it is answered. */
  | { verb: "switchLive"; workspace: string }
  /** Asks for a workspace's new name. It renames nothing by itself: the name, the refusals and
   *  the move are the core's (`workspace_rename`, `wscmd::rename`), as they are for
   *  `charter workspace rename` (charter#367). */
  | { verb: "renameWorkspace"; workspace: string }
  /** Opens the Settings tab (SE-16) at the focused level (SE-23): the focused workspace's, else
   *  the project in front's, else You. It writes nothing by itself: a value is changed on the
   *  tab. */
  | { verb: "openSettingsTab" }
  /** Opens the Settings tab at the You level (SE-23): this machine's settings, whatever is
   *  focused, so they stay one palette row away while a project is in front (V89c). */
  | { verb: "openYourSettings" }
  /** Opens Settings at one group (#1201): a row per group, by the group's address. A You group
   *  carries the project in front when there is one, and none when the window has no project;
   *  a Project group carries its project, and a Workspace group its project and workspace. */
  | { verb: "openSettingsGroup"; group: string; plane?: string; workspace?: string }
  | { verb: "readAgain" }
  | { verb: "taskBranch"; id: string; act: "merge" | "discard" }
  /** Opens a new chat for one curation action on one subject, with the action's prompt typed
   *  into it and never sent (ADR 0061). It carries the action's id and nothing of its text: the
   *  core resolves the subject again (`curate`), so what is typed is the core's prompt now. */
  | { verb: "curate"; subject: string; action: string }
  | { verb: "quit" }
  /** Puts a branch file's or folder's path on the clipboard (FM-10): relative to the branch's
   *  folder, or `absolute`. The core places the path and copies it, so the absolute path is
   *  never one the window joined. It changes nothing on disk. */
  | { verb: "copyPath"; at: BranchPath; absolute: boolean }
  /** Shows a branch file or folder in the operating system's file manager (FM-10). The core
   *  places it, inside the branch and through no link, and reveals it. */
  | { verb: "revealPath"; at: BranchPath }
  /** Opens a branch file in the operator's editor at `line` (RC-20). */
  | { verb: "openInEditor"; at: BranchPath; line: number }
  /** Opens a shell tab whose working directory is a folder of a branch (FM-10). The core
   *  resolves the folder; the shell starts no harness, so nothing asks first. */
  | { verb: "shellInFolder"; at: BranchPath }
  /** Opens the picker of this project's chats for a branch file or folder (FM-9, #1151): the
   *  one picked has a reference to it typed in, and never sent. It types nothing by itself:
   *  the pick does, through the core, which places the path as it does for a drop. */
  | { verb: "addToChat"; at: BranchPath; folder: boolean }
  /** Starts a chat on the branch, in its folder, with a reference to this file or folder typed
   *  as its first prompt and never sent (FM-9). The core places the path and renders it in the
   *  harness's syntax; on a harness it cannot type into, the reference is copied instead. */
  | { verb: "startChatHere"; at: BranchPath }
  /** A row that cannot run. It still carries a `Does`, so "what it would do" and "whether it
   *  can" stay separate questions — and `perform` refuses it rather than guessing. */
  | { verb: "nothing" };

/** One thing the window can do, and everything a surface needs to offer it. */
export type Offer = {
  /**
   * Charter's own name for the action.
   *
   * **A colon separates the verb from a name it is about**, and that is structural rather
   * than decorative: `matches` filters on the part BEFORE the colon, so typing `7` does not
   * list every tab whose number happens to contain a seven while typing `select` still
   * lists them all. `frame/tabmenu.py` spells the same distinction for the same reason — a
   * name reaches the list without becoming part of charter's vocabulary.
   */
  id: string;
  /** What the operator reads, on the row and on the button. One source for both. */
  title: string;
  available: boolean;
  /** Non-empty exactly when `available` is false. */
  reason: string;
  does: Does;
  /**
   * The NAME this row's title carries, when it carries one — a tab's name, a workspace's, a
   * chat's — as the exact substring of `title` it appears as.
   *
   * It is a field rather than something read back out of the title, because what is a name
   * and what is charter's own word is not recoverable from the finished sentence: `Switch to
   * tab release.3` and `Remove the folder of this chat's branch` both contain `re`. `narrow`
   * uses it to put a row the operator's words FOUND ahead of a row that merely has those
   * letters in somebody's name — which at fifty chats is the whole difference (charter-app#48).
   */
  name?: string;
  /**
   * What this row does that its title cannot fit, for a row whose consequence is worth a
   * second sentence. Drawn beside the row in the palette, and as the tooltip of a button that
   * is only a glyph.
   *
   * It is not a `reason`: a reason is why a row CANNOT run, and is non-empty exactly when
   * `available` is false. A note is about a row that can.
   */
  note?: string;
  /**
   * For a row about one chat whose words differ while that chat is mid-turn: the chat, and
   * what the row reads then. **The surface that draws the row reads the chat's state as it
   * draws** ({@link titleOf}), so the catalogue is not built again each time a chat moves,
   * which would redraw the whole window for one chat's turn.
   */
  midTurn?: { session: number; title: string };
  /**
   * For a row about one chat whose note differs while that chat reports no state: the chat, and
   * what the note reads then. Read as the row is drawn ({@link noteOf}), as `midTurn` is.
   */
  noState?: { session: number; note: string };
  /**
   * The group a submenu draws this row under, for a row that is in one: a curation action's
   * declaring persona, or [`LEFT_OUT`] for an action the core left out. None for charter's own
   * — they come first, ungrouped. The palette ignores it: its rows name their group in the
   * title already.
   */
  group?: string;
  /**
   * **A row of a menu that the palette leaves out** (#1468): one of a set made per tab, where
   * the palette lists only the in-front tab's. Twenty tabs and eight personas made 160 rows
   * all titled alike ("Ask devops…"); the palette lists the eight of the chat in front, and
   * each tab's menu keeps its own ({@link inPalette}).
   */
  menuOnly?: true;
};

/** The window as it now stands: everything an offer's availability is decided from. */
export type Now = {
  tabs: Tabs;
  /** The plane's workspaces, in the order the sidebar lists them. */
  workspaces: readonly string[];
  /** The ones that are LIVE (charter-app#301): published with the plane. */
  live?: readonly string[];
  /** The workspace the panels are showing. */
  focused?: string;
  /** Where the chat in front is working, when it is working in a charter worktree. */
  worktree?: ChatWorktree;
  /**
   * The focused workspace's worktrees, as the explorer lists them (charter-app#174).
   *
   * **This is the ~100 rows the issue put a number on**, and it is the price of the
   * explorer's rows having a menu at all: two rows per piece, at ADR 0026's ten clones with
   * five pieces each. `menuRows` is what pays it per render, and it stopped scanning for it
   * — see [`catalogued`].
   *
   * Only the FOCUSED workspace's, because that is the only one any surface draws: the
   * explorer, the bottom bar and this list all answer about one workspace, and carrying every
   * workspace's pieces would multiply the number above by the workspace count to serve rows
   * nothing can right-click.
   */
  pieces?: readonly Cut[];
  /**
   * The focused workspace's clones, each with the path the core spelled (`Panels.paths`).
   *
   * Three rows each (charter-app#174, GL-1): a clone had no menu because nothing here was about one,
   * and nothing could be until the core said where a clone is. Ten clones is thirty rows,
   * counted beside the pieces in `actions.test.ts`.
   */
  clones?: readonly Clone[];
  /**
   * The repos the focused workspace names that are not cloned here (`Panels.absent`), one
   * `Clone <repo>` row each, and one row that clones them all (#1215).
   */
  absent?: readonly string[];
  /** Of those, the ones this window is cloning or about to: their rows say so rather than
   *  start a second clone of the same repo. */
  cloning?: readonly string[];
  /** Where the next chat starts, when the explorer has picked somewhere — the path, so a
   *  clone's pick row can say it is already the spot rather than offer it again. */
  startsIn?: string;
  /**
   * The plane's personas, as the right-hand panel lists them.
   *
   * One row each, and they are cheap: a plane has a handful, not a workspace's worth.
   */
  personas?: readonly string[];
  /**
   * The project's harness profiles, by name: one Settings row each, to the profile's own page
   * (#1201, D-1201-4). Absent until the window holds the list, and then no row is drawn.
   */
  profiles?: readonly string[];
  /**
   * The plane's vaults, by name (`vault_list`), one row each: a vault opens its own tab. A
   * vault is the plane's, so these rows are the same whichever workspace is focused.
   */
  vaults?: readonly string[];
  /**
   * The focused workspace's open todos, by slug and title (`Panels.todos`), one close and one
   * forget row each. Only the focused workspace's: the Todos panel is the one surface that
   * draws them, and it is about that workspace.
   */
  todos?: readonly { slug: string; title: string }[];
  /**
   * The session records of the place in front — the focused workspace's, or the plane root's
   * when it is focused (SI-8d) — newest first, one open and one resume row each.
   */
  sessions?: readonly { path: string; title: string; resumable: boolean }[];
  /**
   * The views approved extensions offer this window (`extension_views`), one row each. The
   * palette is how a keyboard reaches them; the personas panel's heading is how a pointer does.
   */
  views?: readonly ExtensionView[];
  /**
   * The palette commands approved extensions add (`extension_commands`, charter-app#341), one
   * row each, named `<extension's name>: <title>`.
   */
  commands?: readonly ExtensionCommand[];
  /** The plane's root. Every worktree command needs it, and there may not be one. */
  plane?: string;
  /**
   * Every project this window holds, left to right as the project tabs show them.
   *
   * A window can hold several (ADR 0033), and switching between them is navigation rather
   * than a state change: the project left behind keeps every chat it had running. The rows
   * are here for the same reason the tab rows are — a way to reach a project without a
   * pointer, and at eight projects the palette is faster than the strip.
   */
  projects?: readonly Project[];
  /** Whether this window is a split window — a project tab moved into a window of its own
   *  (charter#126) — and so offers to move its projects back to the main window. */
  split?: boolean;
  /**
   * The ROW whose refusal the operator has not answered yet, by id.
   *
   * It was the refusal's words, and only presence was ever read. Now that a removal names its
   * piece there are as many removals as there are pieces, and the discard row has to appear
   * beside the one that was refused rather than beside all of them — so what the window hands
   * over is which row spoke, and this module matches the ids it wrote itself.
   */
  refused?: string;
  /**
   * Whether a read of the focused workspace stands refused (#1244): the workspace, the forge
   * cache, a repo or a tree purlis could not read. While one does, {@link READ_AGAIN} is a row,
   * on the refusal line's menu and in the palette, so its way out does not hang on the explorer.
   */
  readRefused?: boolean;
  /** The chats asking for you, oldest first. */
  needsYou: readonly number[];
  /**
   * What this operator has pinned (ADR 0039).
   *
   * Three lists rather than a flag on each thing, because a pin is not a property of the
   * chat, the workspace or the plane — it is the operator's arrangement of them, held
   * somewhere else entirely (ADR 0040), and a copy on the thing would be a second answer.
   */
  pinned?: {
    readonly chats: readonly number[];
    /** View tabs, by `tabs.viewKey`. A tab with no chat is pinned as the view it shows. */
    readonly views?: readonly string[];
    readonly workspaces: readonly string[];
    readonly projects: readonly string[];
  };
  /** The chats that can be waiting on you without saying so, by name — a Codex chat stopped
   *  mid-turn for an approval says nothing (charter-app#52). The row for the queue reads it,
   *  so a palette that says "Nothing needs you." is never saying more than charter knows. */
  quiet?: readonly string[];
  /** What a chat is called, for a row that names one. */
  nameOf: (session: number) => string;
  /** The chats that reported back to a chat in the queue, by name (charter-app#259), so its row
   *  says what the operator is being asked to look at. */
  reportsTo?: (session: number) => readonly string[];
  /** What a chat in the queue had its commits refused for (SQ-16), so its row says so. */
  refusedIn?: (session: number) => readonly string[];
  /**
   * What the plane's workspaces, personas and the plane itself are offered to curate
   * (`curation_offers`, ADR 0061) — the core's answer, one row per action and one disabled row
   * per action it left out. Every subject's, because the palette lists them all; the menus
   * pick theirs out by id ([`curateRows`]).
   */
  curations?: Curations;
  /** The chats being smart-closed (ADR 0064): each one's tab offers to cancel it. */
  wrappingUp?: readonly number[];
  /** The work item each chat works on, by session, as `chat_work_item` answered (ADR 0088). */
  workItems?: Readonly<Record<number, string>>;
  /** Whether a chat can have a work link: it is filed in a workspace. A chat at the project
   *  root, or working outside the project, is offered neither row (ADR 0088 §4). */
  linkable?: (session: number) => boolean;
  /** The files each chat started on that the project has changed since, by session
   *  (`chats_plane_updated`, charter#369): such a chat is offered Start fresh (NO-3). */
  planeUpdated?: Readonly<Record<number, readonly string[]>>;
  /** Whether a chat can be restarted on its conversation: it has a Restart chat row. A shell
   *  has no conversation, and no row. */
  restartable?: (session: number) => boolean;
  /** Why each chat's Smart close stopped without its record (SI-8f), for its needs-you rows. */
  stopped?: Readonly<Record<number, string>>;
  /**
   * What **Ask {persona}…** offers on this project's chats (`ask_persona_offer`): the personas
   * that can be asked, or why none can, where an administrator's policy locks all dispatch.
   * None until the core has answered, and then there are no rows.
   */
  ask?: {
    personas: readonly string[];
    locked: string | null;
    /** The asks policy takes off one chat's tab: a locked pair, from the persona that chat
     *  runs as to the one asked, and policy's sentence saying who locked it. */
    locked_for?: readonly { session: number; persona: string; why: string }[];
  };
  /** Whether a persona can be asked from a chat's tab: not from a shell, which is on no
   *  harness profile to start the persona's chat on. */
  askable?: (session: number) => boolean;
  /** Why a chat in the queue needs you when nothing it said itself says so (#1448), as its row
   *  says each. */
  neededFor?: (session: number) => readonly string[];
  /** The chats a chat in the queue started that the operator stopped, by name (#1448). */
  stoppedBelow?: (session: number) => readonly string[];
  /** Every running chat of the project, with or without a tab, as the Chats section lists it
   *  (#1447): each is offered Stop (#1448). */
  listed?: readonly ListedChat[];
  /** Which chat a task is a task of, where the window knows it before `listed` is read (at a
   *  launch, from what was put back): what makes a task's own tab a task's from its first
   *  frame (#1489). Left out, it is read off `listed`. */
  askedBy?: AskedBy;
  /** Whether the core starts a task fresh (#1609): left out, {@link TASKS_START_FRESH}. */
  tasksStartFresh?: boolean;
  /** The chats being stopped (#1448): each one's Stop row ends it now. */
  stopping?: readonly number[];
  /** Each chat's finished tasks, by its number (#1485): a tab whose tasks have all finished
   *  still has a task menu to open (#1487). */
  finished?: ReadonlyMap<number, readonly FinishedTask[]>;
  /** The branch nearest the operator: the cockpit's, else the one the explorer picked. What
   *  *Search in files* searches first, as ⌘⇧F does (#1137). */
  branch?: Place;
  /** The cards of the harnesses the project has (`harnessCards.ts`, #1134): one
   *  *What <product> can do here* row each, with no chat open. */
  harnesses?: readonly HarnessGlance[];
  /** The project's memory stores (`memory_scopes`): an open memory tab's Move rows, one per
   *  store but its own (#1190). None while unread, and then there are no Move rows. */
  memoryStores?: readonly MemoryScope[];
  /** The regions put away (#1673): each side's region row says whether it brings it back or
   *  puts it away. */
  away?: readonly RegionId[];
  /** The extensions' panels the right side shows as views (#1678), each a row by its name. */
  sidePanels?: readonly { view: PanelViewId; name: string }[];
};

/** What the window does when a row is run. One function per verb, whichever surface asked. */
export type Doing = {
  newChat: () => void;
  /** A shell tab, in `workspace` when a row names one, else where a new chat would start. */
  newShell: (workspace?: string) => void;
  split: (direction: Direction) => void;
  closePane: () => void;
  closeTab: (tab: number) => void;
  selectTab: (tab: number) => void;
  /** Brings the tab forward with its name open for editing. Nothing is renamed until the
   *  operator says the name, so it answers no `Ran`. */
  renameTab: (tab: number) => void;
  /** Opens the dialog that asks for the work item. Nothing is linked until it is answered. */
  linkWorkItem: (tab: number) => void;
  /** Asks whether to start the tab's chat fresh, and does on a yes (NO-3). */
  startFresh: (tab: number) => void;
  /** Restarts the tab's chat on its conversation, once its turn has ended. */
  restartChat: (tab: number) => void;
  /** The same for a chat with no tab, from its row in the Chats list (#1462). */
  restartListed: (session: number) => void;
  /** Opens the dialog that asks `persona` for something from the tab's chat. */
  askPersona: (tab: number, persona: string) => void;
  /** Ends the chat's work link, through `chat_work_unlink`; a refusal is the core's sentence. */
  unlinkWorkItem: (tab: number) => Promise<Ran>;
  /** Each answers a `Ran`, because a pin can be refused: the stores are bounded, and
   *  "charter pins at most 32 projects — unpin one first" is a sentence the operator can act
   *  on and must therefore reach them. */
  pinTab: (tab: number, pinned: boolean) => Promise<Ran>;
  pinWorkspace: (workspace: string, pinned: boolean) => Promise<Ran>;
  pinProject: (plane: string, pinned: boolean) => Promise<Ran>;
  focusWorkspace: (workspace: string) => void;
  /** Opens the new-workspace dialog. Nothing is created until it is answered. */
  createWorkspace: () => void;
  /** Opens the delete dialog for one workspace. Nothing is deleted until it is answered, and
   *  what deletes is `workspace_remove` — never a lower-level call that would be past the
   *  core's guard. */
  removeWorkspace: (workspace: string) => void;
  showChat: (session: number, inside?: boolean) => void;
  /** Gives a task a tab of its own, in front. */
  ownTab: (session: number) => void;
  /** Opens a task beside its session, in the session's tab, with the keyboard in it. */
  beside: (session: number) => void;
  /** Takes a task's own tab or pane away. The task goes on, in its session's tab. */
  sendBack: (session: number) => void;
  /** Opens the menu of that tab's chats, with the keyboard on the chat it shows. */
  showTabTasks: (tab: number) => void;
  /** Opens the Brief panel of the task chat `session` (#1494). It reads and changes nothing
   *  by itself, so it answers no `Ran`. */
  showBrief: (session: number) => void;
  /** Opens the Answer form of the task chat `session` (#1551). It sends nothing by itself, so
   *  it answers no `Ran`. */
  answerQuestion: (session: number) => void;
  /** Answers a `Ran`, because it is a command the core can refuse — a project closed meanwhile. */
  ignoreNeedsYou: (session: number) => Promise<Ran>;
  /** Answers a `Ran`, because the core can refuse it — a project closed meanwhile. */
  cancelSmartClose: (session: number) => Promise<Ran>;
  dismissStopped: (session: number) => void;
  /** Opens the question a stop asks first, or ends at once a chat that is already stopping.
   *  Nothing is stopped until it is answered, so it answers no `Ran`. */
  stopChat: (session: number, below: boolean) => void;
  /** Opens the one question Stop all tasks asks first (#1498). Nothing is stopped until it is
   *  answered, so it answers no `Ran`. */
  stopAllTasks: (session: number) => void;
  /** Ends a task the way pressed, asking first where V100-18 says to. It answers no `Ran`:
   *  what it does is said by the question, or by the task's row going. */
  endTask: (session: number, way: TaskEndWay) => void;
  /** Opens a view's tab, or brings forward the one showing it. It reads and changes nothing
   *  by itself, so it answers no `Ran`. */
  openView: (view: ViewRef, title: string) => void;
  /** Resumes a session from its record at `path`, in a tab of its own. */
  resumeSession: (path: string) => Promise<Ran>;
  /** Opens a memory's tab: previewed, or kept when `keep` (SI-9b). */
  openMemory: (ref: MemoryRef, title: string, keep: boolean) => void;
  /** Opens a memory's tab, kept, with its editor open. */
  editMemory: (ref: MemoryRef, title: string) => void;
  /** Archives a memory, closes its tab and offers Undo. The core can refuse. */
  archiveMemory: (ref: MemoryRef, title: string) => Promise<Ran>;
  /** Moves a memory to another store, its tab following it, and offers Undo. The core can
   *  refuse: a store holding that name already, one charter may not write. */
  moveMemory: (ref: MemoryRef, title: string, to: MemoryScope) => Promise<Ran>;
  /** Opens a new memory's tab for `scope`, in edit mode. */
  newMemory: (scope: MemoryScope) => void;
  /** Keeps a preview tab. */
  keepTab: (tab: number) => void;
  /** Runs an extension's action — asking first when it says to, and always when it deletes. It
   *  answers a `Ran`: the core can refuse, and what it saw change outside the extension's
   *  declared paths is a sentence the operator is owed. */
  runAction: (extension: string, action: RowAction, name: string) => Promise<Ran>;
  /** Opens the vault picker. Nothing is opened until a vault in it is. */
  pickVault: () => void;
  /** Opens the new-vault dialog. Nothing is made until it is answered. */
  createVault: () => void;
  /** Opens the delete dialog for one vault. Nothing is deleted until its name is typed back. */
  removeVault: (vault: string) => void;
  /** Opens the new-persona dialog. Nothing is made until it is answered. */
  createPersona: () => void;
  /** Hands the persona's definition to the system's editor. The core can refuse — a persona
   *  deleted meanwhile — so it answers a `Ran`. */
  editPersona: (persona: string) => Promise<Ran>;
  /** Opens the profile dialog for one persona. Nothing is written until it is answered. */
  setPersonaProfile: (persona: string) => void;
  /** Opens the delete dialog for one persona. Nothing is deleted until it is answered. */
  removePersona: (persona: string) => void;
  /** Each answers a `Ran`: the core can refuse, and what it did is a sentence the operator is
   *  owed — which todo closed, and that the journal has it or does not. */
  closeTodo: (workspace: string, slug: string) => Promise<Ran>;
  forgetTodo: (workspace: string, slug: string) => Promise<Ran>;
  /** Each takes the piece it acts on. The window no longer decides which worktree a removal
   *  meant by looking at what happens to be in front (charter-app#174). */
  removeWorktree: (cut: Cut, force: boolean) => Promise<Ran>;
  mergeWorktree: (cut: Cut) => Promise<Ran>;
  declareWorktreeDone: (cut: Cut) => Promise<Ran>;
  /** Focuses the explorer on that branch: its cockpit (FM-5). It starts and writes nothing. */
  focusBranch: (cut: Cut) => void;
  /** Narrows the explorer to a clone of the focused workspace, its own folder (#1152): the
   *  branch cockpit without Merge and Done. */
  focusRepo: (repo: string) => void;
  /** Makes that clone where the next chat starts. It starts nothing, so it answers no `Ran`. */
  pickClone: (repo: string, path: string) => void;
  /** Opens the New branch dialog for that clone. It cuts nothing until it is answered. */
  newBranch: (repo: string) => void;
  /** Clones those repos into the workspace, in turn. Each one's progress and failure is drawn
   *  on its own row; the `Ran` says which failed, in the core's words. */
  cloneMissing: (workspace: string, repos: string[]) => Promise<Ran>;
  /** Asks whether to take that repo out of the workspace's membership. Nothing is written
   *  until it is answered. */
  askDropMembership: (workspace: string, repo: string) => void;
  /** Opens the picker for a new tab whose chat starts in that directory, and nowhere else. */
  newChatIn: (path: string) => void;
  sendKey: (key: string) => Promise<Ran>;
  openProject: () => void;
  /** Opens the new-project dialog. Nothing is scaffolded and nothing is opened until it is
   *  answered, and the open it ends in is the gated one. */
  createProject: () => void;
  showExtensions: () => void;
  showSideView: (view: ViewId) => void;
  toggleRegion: (region: RegionId) => void;
  installCli: () => Promise<Ran>;
  selectProject: (plane: string) => void;
  /** Opens the project switcher. Nothing is switched until a row in it is run. */
  switchProject: () => void;
  closeProject: (plane: string) => Promise<Ran>;
  /** Moves that project into another window, or a new one. The core can refuse. */
  moveProject: (plane: string, to: string | null) => Promise<Ran>;
  /** Brings that project to the front and opens Settings at its Project level. */
  openSettings: (plane: string) => void;
  /** Brings that project to the front and opens its Saving tab. */
  openSaving: (plane: string) => void;
  /** Opens that workspace's settings tab on its strip, or brings forward the one already open. */
  openWorkspaceSettings: (workspace: string) => void;
  /** Asks whether to make that workspace LIVE or LOCAL, in a confirmation. */
  switchLive: (workspace: string) => void;
  /** Opens the rename dialog for one workspace. Nothing is renamed until it is answered. */
  renameWorkspace: (workspace: string) => void;
  /** Opens the Settings tab at the focused level, or brings forward the one already open. */
  openSettingsTab: () => void;
  /** Opens the Settings tab at the You level, or brings forward the one already open. */
  openYourSettings: () => void;
  /** Asks the focused workspace's reads again — the plane's, git's and the forge cache's —
   *  after one was refused (#1244): the explorer's Read again. */
  readAgain: () => void;
  /** Asks about a finished task's Merge or Discard of its own branch (#1534), by its dispatch's
   *  id: the Changes tab's own question, answered in the window. Nothing changes until then. */
  taskBranch: (id: string, act: "merge" | "discard") => void;
  /** Opens a curation chat. The core can refuse — the action gone, a harness that cannot be
   *  typed into — so it answers a `Ran`. */
  curate: (subject: string, action: string) => Promise<Ran>;
  quit: () => void;
  /** Each answers a `Ran`: the core places the path and can refuse it — a path that left the
   *  branch, a link, a file gone meanwhile — and says so in its own sentence. */
  copyPath: (at: BranchPath, absolute: boolean) => Promise<Ran>;
  revealPath: (at: BranchPath) => Promise<Ran>;
  /** Refused, rather than guessed, while no editor is chosen in Settings. */
  openInEditor: (at: BranchPath, line: number) => Promise<Ran>;
  /** A shell tab in that folder; a refusal is said the way a shell tab's is. */
  shellInFolder: (at: BranchPath) => void;
  /** A chat on the branch with a reference to `at` typed into it (FM-9). */
  startChatHere: (at: BranchPath) => Promise<Ran>;
  /** Opens the chat picker for `at`. Nothing is typed until a chat in it is picked. */
  addToChat: (at: BranchPath, folder: boolean) => void;
};

/**
 * One file or folder of a branch, as a file row names it (FM-10): the branch by workspace,
 * repo and piece — no piece for the repo's own folder (#948) — and the path inside it. Never a
 * directory: what it is on disk is the core's answer (`files::place`).
 */
export type BranchPath = { workspace: string; repo: string; piece: string | null; path: string };

/**
 * One worktree charter cut, named the way every worktree command names one.
 *
 * Three parts and not a path: `worktree_remove` and `worktree_merge` take the workspace, the
 * clone and the piece, so this is what a row carries and nothing here ever joins a path
 * together. The workspace is part of it because the chat in front may be working in a piece of
 * a workspace that is not the one focused.
 */
/** A piece the window can act on: its workspace, its repo and its folder's name, and the branch
 *  git has checked out there when the listing said (#989). The branch is what a row names; a
 *  folder git has on no branch is named as the folder. */
export type Cut = { workspace: string; repo: string; piece: string; branch?: string | null };

/** One clone of the focused workspace: its name, and where it is as the core spelled it. */
export type Clone = { repo: string; path: string };

/** The id `Cut` gets inside a row: the clone and the piece, which is unique within one
 *  workspace and is what the explorer's row can name without looking anything up. */
function idOf(cut: Cut): string {
  return `${cut.repo}/${cut.piece}`;
}

/** One project a window holds, as the strip and the palette both name it. */
export type Project = {
  /** Its root, which is its id everywhere else in the app. */
  plane: string;
  /** What to call it on a tab — the directory's own name. */
  name: string;
};

/**
 * Every row about the projects a window holds.
 *
 * **Exported because the project strip draws these rows and so does the palette**, and the
 * rule this module opens with says there is one place an action is written down. `catalogue`
 * splices them into its two sections — switching is navigation and goes above the line,
 * letting go of a project ends its chats and goes below — and the strip looks them up by the
 * index of the project they belong to. `switchTo` and `close` are therefore in `projects`'
 * own order, one row each, always.
 */
export function projectRows(
  projects: readonly Project[],
  /** The project in front, when one is. */
  front: string | undefined,
  /** The projects this operator has pinned, by root. */
  pinned: readonly string[] = [],
  /** Whether this window is a split window, which offers to move a project back (charter#126). */
  split = false,
): {
  open: Offer;
  create: Offer;
  /** The one row that opens the switcher over every project here (FR-27). */
  switcher: Offer;
  switchTo: Offer[];
  pin: Offer[];
  settings: Offer[];
  saving: Offer[];
  /** Into a new window of its own, one row per project (charter#126). */
  window: Offer[];
  /** Back to the main window — only in a split window, so empty in the main one. */
  back: Offer[];
  close: Offer[];
} {
  return {
    // Always available, and available with no project open too: it is how a window with
    // nothing in it gets its first one, and how a window with eight gets a ninth.
    open: can("project.open", "Open a project…", { verb: "openProject" }),
    // **The seam the project strip's `+` is drawn from**, and the id anything else that wants
    // to start a project asks for. Always available, and available with nothing open, for
    // `project.open`'s reason: a window holding no project is exactly where one is made.
    create: {
      ...can("project.create", "New project…", { verb: "createProject" }),
      note: "Makes a project in a directory of its own. It never writes into a repo you point at.",
    },
    // **One row for all of them, beside the one row per project** (FR-27). The rows below find
    // a project by its name from the whole palette; this one is the palette listing nothing but
    // the projects, the last one the operator was in aimed at, so a switch back is the key and
    // Enter. A window holding one project has nowhere to switch to, and says so in the words
    // `project.window` uses for the same fact.
    switcher:
      projects.length < 2
        ? cannot(
            "project.switch",
            "Switch project…",
            projects.length === 0
              ? "This window holds no project."
              : "It is the only project in this window.",
          )
        : {
            ...can("project.switch", "Switch project…", { verb: "switchProject" }),
            note: `The projects open in this window, the last one you were in first. ${SWITCHER_KEY_SAID}.`,
          },
    switchTo: projects.map((project) => {
      const title = `Switch to project ${project.name}`;
      // The project in front has a row that says so and cannot run — the same rule the tab
      // strip follows, and for the same reason: a strip that lists eight and a palette that
      // lists seven is the second answer this module exists to not have.
      return project.plane === front
        ? cannot(`project.select:${project.plane}`, title, "It is already in front.", project.name)
        : can(
            `project.select:${project.plane}`,
            title,
            { verb: "selectProject", plane: project.plane },
            project.name,
          );
    }),
    pin: projects.map((project) => {
      const held = pinned.includes(project.plane);
      return {
        ...can(
          `project.pin:${project.plane}`,
          `${held ? "Unpin" : "Pin"} project ${project.name}`,
          { verb: "pinProject", plane: project.plane, pinned: !held },
          project.name,
        ),
        // One thing a project's pin does that the other two do not, so it is said here and
        // not in `PIN_NOTE`: charter remembers 64 planes, and a pinned one is kept past that.
        note: held ? UNPIN_NOTE : `${PIN_NOTE} Keeps it in the opener's list.`,
      };
    }),
    // The words the operator asked for on the tab's menu, and the project's name in the note:
    // in a menu the project is the one right-clicked, and in the palette the note is what tells
    // eight of these rows apart.
    settings: projects.map((project) => ({
      ...can(`project.settings:${project.plane}`, "Project settings…", {
        verb: "openSettings",
        plane: project.plane,
      }),
      note: `${project.name}: charter.toml, for the team, and charter.local.toml, for this machine.`,
    })),
    saving: projects.map((project) => ({
      ...can(`project.saving:${project.plane}`, "Saving…", {
        verb: "openSaving",
        plane: project.plane,
      }),
      note: `${project.name}: what is not saved yet, and the save button.`,
    })),
    // **A project tab can be split into a window of its own, and moved back** (ADR 0033: planes
    // merge into one window and split back out of it). A window holding one project has
    // nothing to split it from, so the row says so rather than making a second window the same
    // as the first.
    window: projects.map((project) => {
      const title = `Move project ${project.name} to a new window`;
      return projects.length < 2
        ? cannot(
            `project.window:${project.plane}`,
            title,
            "It is the only project in this window.",
            project.name,
          )
        : {
            ...can(
              `project.window:${project.plane}`,
              title,
              { verb: "moveProject", plane: project.plane, to: null },
              project.name,
            ),
            note: "Its chats go on running. Closing that window moves it back.",
          };
    }),
    back: split
      ? projects.map((project) => ({
          ...can(
            `project.main:${project.plane}`,
            `Move project ${project.name} to the main window`,
            { verb: "moveProject", plane: project.plane, to: MAIN },
            project.name,
          ),
          note: "Its chats go on running.",
        }))
      : [],
    close: projects.map((project) => ({
      ...can(
        `project.close:${project.plane}`,
        `Close project ${project.name}`,
        { verb: "closeProject", plane: project.plane },
        project.name,
      ),
      // Its `×` is the same glyph as a tab's and it does more, so it says so too. Both halves
      // matter: what goes is every chat, and what does NOT go is anything on disk.
      note: "Ends every chat in it. Nothing of the project on disk goes.",
    })),
  };
}

/**
 * What ending a chat costs, said on the row that does it (charter-app#130).
 *
 * The `×` on a tab has always called `close_session`, which ends the program and takes the
 * chat off the board. That is the right behaviour and it is not changing. What was missing is
 * anybody saying so: the glyph reads as "hide this tab", and at fifty tabs with no undo the
 * operator tidying up was ending fifty live harnesses on that reading.
 */
export const ENDS_IT = "Ends the program it runs. There is no undo.";

/**
 * What deleting a workspace costs, said on the row that asks for it.
 *
 * Named in full rather than as "deletes the workspace", which is a word that sounds like a
 * tab closing. What goes is a directory of clones: every repo cloned into it, every worktree
 * cut in it, its memory and its todos. What charter will refuse over — uncommitted and
 * unpushed work — is the core's guard and is said by the core, on the dialog, about the
 * workspace actually in front of the operator. This is what is true of every workspace.
 */
export const DELETES_A_WORKSPACE =
  "Deletes its clones, its branches' folders, its memory and its todos. There is no undo.";

/**
 * What a pin does, said on the row that does it.
 *
 * Both halves matter and neither is obvious from the word "pin": **where** it is kept, because
 * charter's founding rule is that the plane is the state and this is one of the few things
 * that is not; and **who** it is for, because an operator who thinks a pin travels with the
 * clone will arrange a plane for a team that never sees it.
 */
export const PIN_NOTE = "Draws it first on its strip. Yours, on this machine only.";

/** And the same said the other way, so unpinning is not a row with no consequence on it. */
export const UNPIN_NOTE = "Puts it back in the plane's own order.";

/**
 * What removing a worktree costs, said on the row that does it (charter-app#174).
 *
 * The half that is worth saying is the half nobody expects: **the branch stays**. `git
 * worktree remove` takes the directory and leaves the ref, so "remove" here is not the same
 * word it is on a workspace, and a row that popped up under the pointer saying only `Remove
 * worktree fix-it` reads as the harsher of the two. What IS lost is what was never committed,
 * and the core refuses over that rather than this row warning about it.
 */
export const KEEPS_THE_BRANCH = "Takes the folder, not the branch. The branch stays where it is.";

/** Nothing happened worth saying, which is the ordinary answer. */
const DID: Ran = { ok: true };

/** The row that clones every repo the focused workspace names and this machine lacks — the
 *  explorer's `Clone all` presses it (#1215). */
export const CLONE_ALL_ID = "absent.cloneAll";

/** The row that clones one of them — the explorer's row button and its Retry press it. */
export const cloneMissingId = (repo: string) => `absent.clone:${repo}`;

/** The row that asks to take one of them out of the workspace (#1228) — the explorer's row
 *  button presses it, and its menu draws it below the line. */
export const dropMembershipId = (repo: string) => `absent.drop:${repo}`;

/** What the row that takes a repo out of the workspace says, wherever it is drawn. */
export const dropMembershipTitle = (repo: string) => `Remove ${repo} from workspace…`;

/** Whether one of the operator's pins names this thing. */
function isPinned<T>(held: readonly T[], one: T): boolean {
  return held.includes(one);
}

/** The catalogue's id for the row that opens the Activity of `tab`'s chat (#1495). */
export const activityId = (tab: number) => `tab.activity:${tab}`;

/** The catalogue's id for the row that opens the Network view of `tab`'s chat (#1662). */
export const networkId = (tab: number) => `tab.network:${tab}`;

/** The catalogue's id for the row that opens the project's Dispatches tab (#1452). */
export const DISPATCHES_SHOW = "dispatches.show";

/** An offer that can run, spelled once so `reason` cannot drift from `available`. */
function can(id: string, title: string, does: Does, name?: string): Offer {
  return { id, title, available: true, reason: "", does, name };
}

/** What Restart chat keeps and what it changes. */
const RESTART_KEEPS =
  "It keeps its conversation and starts on this project's settings as they are now.";

/**
 * What Restart chat keeps, what it changes and when it happens, said beside its row. The wait
 * is in the note, so the palette, which shows a row's plain title, says it as a menu does.
 */
export const RESTART_NOTE = `${RESTART_KEEPS} Mid-turn, it restarts when the turn ends.`;

/**
 * The note for a chat that reports no state (docs/ui-copy.md, "Uncertainty is stated, not
 * hidden"): purlis cannot wait for the end of a turn it cannot see, so it does not say it will.
 */
export const restartNoteNoState = (name: string): string =>
  `${name} reports no state, so purlis cannot tell whether it is mid-turn, and restarts it at once. ${RESTART_KEEPS}`;

/** What a row reads now: its mid-turn words while the chat it is about is `running`. */
export function titleOf(offer: Offer, running: boolean): string {
  return running && offer.midTurn !== undefined ? offer.midTurn.title : offer.title;
}

/** What a row's note reads now: its no-state words while the chat it is about reports none. */
export function noteOf(offer: Offer, unknown: boolean): string | undefined {
  return unknown && offer.noState !== undefined ? offer.noState.note : offer.note;
}

/** *Search in files*'s row (#1137). */
export const SEARCH_ID = "search.files";

/** The palette's row that shows a side's view (#1673). */
export const sideViewId = (view: ViewId) => `view.show:${view}`;
/** The palette's row that puts the navigation region away or brings it back (#1673). */
export const TOGGLE_NAVIGATION_ID = "view.toggle:navigation";
/** The palette's row that puts the attention region away or brings it back (#1678). */
export const TOGGLE_ATTENTION_ID = "view.toggle:aside";
/** The right side's own views, as the region's palette row names them: "Todos, … and Vaults". */
const RIGHT_VIEWS_SAID = ((names: string[]) =>
  `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`)(
  ATTENTION_VIEWS.map((view) => VIEWS[view].name),
);
/** What each view is, on its palette row. */
const VIEW_NOTES: Record<Exclude<OwnViewId, "search">, string> = {
  chats: "The project's chats and their tasks, on the left.",
  explorer: "The focused workspace's repos, branches and files, on the left.",
  changes:
    "The focused workspace's repos: branch, uncommitted files, branches and pipeline, on the left.",
  todos: "What is left to do in the focused workspace, on the right.",
  memory: "What the focused workspace remembers, on the right.",
  personas: "The project's personas, on the right.",
  sessions: "The focused workspace's session records, on the right.",
  vaults: "The project's vaults, on the right.",
};

/** The row that opens `harness`'s card (#1134). */
export function harnessCardId(harness: string): string {
  return `harness.card:${harness}`;
}

/** What a Search tab opened on `ask` looks in, as its row's note says it. */
function searchedSaid(ask: SearchAsk): string {
  if (ask.kind === "branch" && ask.branch !== undefined)
    return `branch ${ask.branch.piece ?? ask.branch.repo}`;
  if (ask.kind === "workspace" && ask.workspace !== undefined) return `workspace ${ask.workspace}`;
  return "this project";
}

/** An offer that cannot, which therefore has to say why. */
function cannot(id: string, title: string, reason: string, name?: string): Offer {
  return { id, title, available: false, reason, does: { verb: "nothing" }, name };
}

/**
 * The group a curation row the core left out is drawn under. Slashes, for `OUTSIDE`'s reason:
 * no persona can be called this, so it cannot collide with a declaring persona's group.
 */
export const LEFT_OUT = "left/out";

/** The prefix every curation row of one subject's id starts with. */
function curateId(subject: string): string {
  return `curate:${subject}/`;
}

/**
 * One subject's curation rows: its actions in the core's order — charter's own first, then
 * each persona's — and a row that cannot run for each action the core left out, with the
 * core's sentence as its reason.
 *
 * `cannot` is why no curation chat can be opened in this project right now (its default
 * harness cannot be typed into); every action row is then drawn with it as its reason rather
 * than refused on a click.
 */
function curationRows(subject: SubjectCurations, cannotOpen: string | null): Offer[] {
  const name = subject.name;
  const rows: Offer[] = subject.actions.map((action) => {
    const id = `${curateId(subject.subject)}${action.id}`;
    const title = `Curate ${name}: ${action.label}`;
    const group = action.declared_by ?? undefined;
    if (cannotOpen !== null) return { ...cannot(id, title, cannotOpen, name), group };
    const who = action.runner === null ? "with no persona" : `as ${action.runner}`;
    return {
      ...can(id, title, { verb: "curate", subject: subject.subject, action: action.id }, name),
      note: `Opens a chat ${who} in ${action.cwd}, with its prompt typed and not sent.`,
      group,
    };
  });
  subject.left_out.forEach((left, i) => {
    rows.push({
      ...cannot(
        `${curateId(subject.subject)}!${i}`,
        `Curate ${name}: ${left.what} is left out`,
        left.why,
        name,
      ),
      group: LEFT_OUT,
    });
  });
  if (subject.trouble !== null && rows.length === 0) {
    rows.push(cannot(`${curateId(subject.subject)}!`, `Curate ${name}`, subject.trouble, name));
  }
  return rows;
}

/**
 * Every offer, in the order a palette lists them and a bar picks from them.
 *
 * **Destructive rows go last**, which is `frame/leave.py`'s rule and the reason the palette
 * is safe to open and press Enter in: closing a chat is never one keystroke away from the
 * row the cursor starts on.
 *
 * **Names are rows too.** The tmux frame kept its forty workspaces out of the browsable list
 * because each was a whole `Action` that would have spawned a second charter process; a row
 * here is a value, so the objection does not carry — and a row an operator cannot see by
 * browsing is a row they have to be told about. What the frame was actually protecting
 * against is answered by `narrow` ranking a verb above a name, not by leaving the name out:
 * at fifty chats it is the TABS that crowd a query, and no version of this list ever left
 * those out.
 */
export function catalogue(now: Now): Offer[] {
  const front = now.tabs.inFront === undefined ? undefined : now.tabs.byId[now.tabs.inFront];
  // What the pane with the keyboard shows. A row about "the chat in front" is about THIS, and a
  // pane showing a view has no chat for it to be about.
  const focusedOn = focusedContent(now.tabs);
  const chatInFocus = focusedOn?.kind === "session";
  const offers: Offer[] = [];

  // A chat starts through the picker, which is the only path ADR 0022 admits. The palette
  // opens that question; it never answers it.
  offers.push(can("chat.new", "New tab", { verb: "chat.new" }));

  // **Beside it, and never inside the picker** (SI-5). A shell tab runs no harness, so there
  // is no profile to pick and nothing for ADR 0022 to ask. It is still a chat to the core — a
  // session with a number, recorded and put back — which is why it is a tab like one.
  offers.push({
    ...can("shell.new", "New shell", { verb: "newShell" }),
    note: `Your own shell, with no harness, where a new tab would start. ${SHELL_KEY_SAID}.`,
  });

  // **Always available, and available with nothing open.** ADR 0041 item 5: ADR 0035
  // shows what a project contributes in the dialog and nothing shows it afterwards, so the
  // surface every later trust decision is read on is the one that lists what is in force NOW.
  // It is about the machine and not about a project, which is why it does not wait for one.
  offers.push(can("extensions.show", "Extensions…", { verb: "showExtensions" }));

  // **Search in files** (FM-8, #1137): the Search view ⌘⇧F shows on the left (#1676), searching
  // as narrow as the focus — the branch nearest the operator, else the workspace in front, else
  // the project. The key is said on the row, so the palette is where it is learned. It is the
  // Search view's palette row: the views' rows below leave Search out rather than say it twice.
  {
    const ask = searchFromFocus(
      now.branch,
      now.focused === undefined || now.focused === OUTSIDE ? undefined : now.focused,
    );
    offers.push(
      now.plane === undefined
        ? cannot(
            SEARCH_ID,
            "Search in files",
            "No project is open, so there are no files to search.",
          )
        : {
            ...can(SEARCH_ID, "Search in files", { verb: "showSideView", view: "search" }),
            note: `Every file of ${searchedSaid(ask)}, in the Search view. ${SEARCH_KEY_SAID}.`,
          },
    );
  }

  // **Each side's views, and the sides themselves** (#1673, #1678, B-10): every view is a row,
  // with its key said where it has one, so the palette is where the keys are learned — as
  // Search's is. An extension's panel is a view too, by the name it declared. Search's row is
  // *Search in files*, above (#1676), so it is not said twice.
  for (const view of Object.keys(VIEWS) as OwnViewId[]) {
    if (view === "search") continue;
    const key = SIDE_KEYS_SAID[view];
    offers.push({
      ...can(sideViewId(view), `Show the ${VIEWS[view].name} view`, { verb: "showSideView", view }),
      note: key === undefined ? VIEW_NOTES[view] : `${VIEW_NOTES[view]} ${key}.`,
    });
  }
  for (const { view, name } of now.sidePanels ?? []) {
    offers.push({
      ...can(sideViewId(view), `Show the ${name} view`, { verb: "showSideView", view }),
      note: "An extension's panel, on the right.",
    });
  }
  offers.push({
    ...can(
      TOGGLE_NAVIGATION_ID,
      now.away?.includes("navigation")
        ? "Bring the Navigation region back"
        : "Put the Navigation region away",
      { verb: "toggleRegion", region: "navigation" },
    ),
    note: `The Chats, Explorer, Search and Changes views on the left. ${SIDE_KEYS_SAID.navigation}.`,
  });
  offers.push({
    ...can(
      TOGGLE_ATTENTION_ID,
      now.away?.includes("aside")
        ? "Bring the Attention region back"
        : "Put the Attention region away",
      { verb: "toggleRegion", region: "aside" },
    ),
    note: `${RIGHT_VIEWS_SAID} on the right. ${SIDE_KEYS_SAID.aside}.`,
  });

  // **A harness's card with no chat open** (HP-19, #1134): one row per harness the project
  // has, opening its card tab as a chat's header button does.
  for (const card of now.harnesses ?? []) {
    offers.push({
      ...can(
        harnessCardId(card.name),
        card.label,
        { verb: "openView", view: harnessCardView(card.name), title: card.label },
        card.title,
      ),
      note: `What a chat on ${card.title} can and cannot do in this project, in a tab of its own.`,
    });
  }

  for (const [id, title, direction] of [
    ["pane.split.right", "Split right", "row"],
    ["pane.split.down", "Split down", "column"],
  ] as const) {
    offers.push(
      front
        ? can(id, title, { verb: "split", direction })
        : cannot(id, title, "No chat is in front, so there is no pane to split."),
    );
  }

  // The key the palette claimed, handed back. Not destructive and not below the line: it is
  // how an operator whose harness binds `F2` types `F2` at all (charter-app#47).
  const sendKey = `Send ${PASS_THROUGH_KEY} to the chat in front`;
  offers.push(
    chatInFocus
      ? can(PASS_THROUGH_ID, sendKey, { verb: "sendKey", key: PASS_THROUGH_KEY })
      : cannot(
          PASS_THROUGH_ID,
          sendKey,
          front
            ? "The pane in focus shows a view, not a chat, so there is nowhere to send it."
            : "No chat is in front, so there is nowhere to send it.",
        ),
  );

  // The needs-you queue, as rows. The first row is always here so it can be browsed to on a
  // quiet plane, and says so rather than going missing.
  //
  // **And it says only as much as charter knows.** A harness that cannot report everything —
  // a Codex chat stopped mid-turn for an approval says nothing (charter-app#52) — makes
  // "Nothing needs you." a claim charter cannot stand behind, so the reason hedges instead.
  const [oldest] = now.needsYou;
  offers.push(
    oldest === undefined
      ? cannot("needs.next", "Show the chat that needs you", nothingSaidSoFar(now.quiet ?? []))
      : can("needs.next", "Show the chat that needs you", { verb: "showChat", session: oldest }),
  );
  offers.push(
    ...needsYouRows(
      now.needsYou,
      now.nameOf,
      now.tabs,
      now.reportsTo,
      now.refusedIn,
      now.neededFor,
      (session) => (now.listed ?? []).some((chat) => chat.session === session),
      now.stoppedBelow,
    ),
  );
  offers.push(...stoppedRows(now.stopped ?? {}, now.needsYou, now.nameOf, now.tabs));

  const pinned = now.pinned ?? { chats: [], workspaces: [], projects: [] };

  for (const tab of now.tabs.order) {
    const name = now.tabs.byId[tab].name;
    const title = `Switch to tab ${name}`;
    offers.push(
      tab === now.tabs.inFront
        ? cannot(`tab.select:${tab}`, title, "It is already in front.", name)
        : can(`tab.select:${tab}`, title, { verb: "selectTab", tab }, name),
    );
  }

  // **Pinning is here and not on the tab**, which is the whole of its surface. A `📌` on
  // fifty tabs is fifty more controls on the one strip that already breaks at fifty, and a
  // pin is a deliberate, occasional act — which is what the palette is for. What a pinned
  // thing gets on its strip is a mark, and pressing the mark runs this same row.
  for (const tab of now.tabs.order) {
    const name = now.tabs.byId[tab].name;
    // A tab is pinned as its own chat, or — a tab with none — as the view it opened on. The
    // pin is stored with whichever it is (the chat's record, or the view tab's), so it goes
    // away with the thing it pins.
    const chat = chatOf(now.tabs, tab);
    const first = contentsOf(now.tabs, tab)[0]?.content;
    const held =
      chat !== undefined
        ? isPinned(pinned.chats, chat)
        : first?.kind === "view" && isPinned(pinned.views ?? [], viewKey(first.view));
    offers.push({
      ...can(
        `tab.pin:${tab}`,
        `${held ? "Unpin" : "Pin"} ${chat === undefined ? "tab" : "chat"} ${name}`,
        { verb: "pinTab", tab, pinned: !held },
        name,
      ),
      note: held ? UNPIN_NOTE : PIN_NOTE,
    });
  }

  // **Renaming is a row, so the tab's menu and the palette are one surface** (charter-app#254),
  // and a double-click on the tab's name runs the same thing. Above the line: it ends nothing.
  // A tab that opened on a view is named after what it shows, and has none.
  for (const tab of now.tabs.order) {
    if (chatOf(now.tabs, tab) === undefined) continue;
    const name = now.tabs.byId[tab].name;
    offers.push(can(`tab.rename:${tab}`, `Rename chat ${name}…`, { verb: "renameTab", tab }, name));
  }

  // **A chat's Activity** (#1495, V100-44): what it and its tasks said to each other, in a tab
  // of its own. One row per chat, so the tab's menu and the palette are one surface; the words
  // are the same on every tab, as a work link's are, so the note names the chat. It only reads,
  // so it sits above the line.
  for (const tab of now.tabs.order) {
    const chat = chatOf(now.tabs, tab);
    if (chat === undefined) continue;
    const name = now.tabs.byId[tab].name;
    offers.push({
      ...can(activityId(tab), "Activity", {
        verb: "openView",
        view: activityView(chat),
        title: activityTitle(name),
      }),
      note: `Chat ${name}: what it and its tasks said to each other, in a tab of its own.`,
    });
  }

  // **A chat's Network** (#1662): what it can reach now and what it was refused, in a tab of
  // its own. One row per chat, as Activity's; it only reads, so it sits above the line.
  for (const tab of now.tabs.order) {
    const chat = chatOf(now.tabs, tab);
    if (chat === undefined) continue;
    const name = now.tabs.byId[tab].name;
    offers.push({
      ...can(networkId(tab), "Network", {
        verb: "openView",
        view: chatNetworkView(chat),
        title: chatNetworkTitle(name),
      }),
      note: `Chat ${name}: what it can reach now and what it was refused, in a tab of its own.`,
    });
  }

  // **Ask a persona from a chat's tab**: one row per chat per persona the project has
  // finished, so the tab's menu and the palette are one surface. The palette lists only the
  // rows of the tab in front (#1468, `menuOnly`). The words are the same on
  // every tab, as a work link's are, so the note names the chat. It opens a dialog and starts
  // nothing until that is answered, so it sits above the line.
  //
  // **Where policy locks all dispatch there is no such row on any tab**, and one row that
  // cannot run says why, in the palette, where a person looks for an action that has gone.
  if (now.ask !== undefined && now.ask.locked !== null) {
    offers.push(cannot(ASK_LOCKED_ID, "Ask a persona…", now.ask.locked));
  } else if (now.ask !== undefined) {
    for (const tab of now.tabs.order) {
      const chat = chatOf(now.tabs, tab);
      if (chat === undefined || !(now.askable?.(chat) ?? false)) continue;
      const name = now.tabs.byId[tab].name;
      for (const persona of now.ask.personas) {
        // A pair policy locks: off this chat's tab ({@link askRows} lists what can run), and
        // a row here that cannot, with policy's own sentence, so the palette says why.
        const locked = now.ask.locked_for?.find(
          (one) => one.session === chat && one.persona === persona,
        );
        if (locked !== undefined) {
          offers.push({
            ...cannot(askId(tab, persona), `Ask ${persona}…`, locked.why, name),
            ...(tab === now.tabs.inFront ? {} : { menuOnly: true as const }),
          });
          continue;
        }
        offers.push({
          ...can(askId(tab, persona), `Ask ${persona}…`, { verb: "askPersona", tab, persona }),
          note: `From chat ${name}. A chat starts as ${persona}, and its report comes back to this one.`,
          // One set in the palette, the chat in front's; each tab's menu has its own (#1468).
          ...(tab === now.tabs.inFront ? {} : { menuOnly: true as const }),
        });
      }
    }
  }

  // **Start fresh** (NO-3, charter#369): a chat the project's instructions changed under since it
  // started runs on what it read until it is started again. Only such a chat has the row, and
  // its tab's mark presses the same one. It ends the chat's program, so it is asked about first
  // (`PlaneView`'s `ChatAsk`), and it sits below the line in the tab's menu.
  for (const tab of now.tabs.order) {
    const chat = chatOf(now.tabs, tab);
    const files = chat === undefined ? undefined : now.planeUpdated?.[chat];
    if (files === undefined || files.length === 0) continue;
    const name = now.tabs.byId[tab].name;
    // **A task** (#1489, #1609): a fresh start is a new conversation, and a task's brief is in
    // the one it has. Where the core hands the brief again it runs, and says so; a refusal (a
    // brief purlis cannot confirm) is said in its question as any refused act is. Until the core
    // does, the row stays and says why, so its tab's menu answers the question instead of
    // losing the row.
    if (chat !== undefined && (now.askedBy ?? askedByOf(now.listed ?? []))(chat) !== undefined) {
      offers.push(
        (now.tasksStartFresh ?? TASKS_START_FRESH)
          ? {
              ...can(
                `tab.fresh:${tab}`,
                `Start chat ${name} fresh`,
                { verb: "startFresh", tab },
                name,
              ),
              note: `Changed since it started: ${files.join(", ")}. ${TASK_FRESH_NOTE}`,
            }
          : cannot(`tab.fresh:${tab}`, `Start chat ${name} fresh`, TASK_NOT_FRESH, name),
      );
      continue;
    }
    offers.push({
      ...can(`tab.fresh:${tab}`, `Start chat ${name} fresh`, { verb: "startFresh", tab }, name),
      note: `Changed since it started: ${files.join(", ")}`,
    });
  }

  // **Restart chat**: the chat's program ends and starts again on the same conversation, with
  // the project's settings as they are now — what a chat needs after a sandbox setting changed.
  // Mid-turn the restart waits for the turn to end, and the row says so (`midTurn`, and the
  // note everywhere). A chat that reports no state restarts at once, and its note says that
  // instead (`noState`). Asking twice asks for one restart. It ends a program, so it sits
  // below the line.
  for (const tab of now.tabs.order) {
    const chat = chatOf(now.tabs, tab);
    if (chat === undefined || !(now.restartable?.(chat) ?? false)) continue;
    const name = now.tabs.byId[tab].name;
    offers.push({
      ...can(`tab.restart:${tab}`, `Restart chat ${name}`, { verb: "restartChat", tab }, name),
      note: RESTART_NOTE,
      midTurn: { session: chat, title: `Restart chat ${name} when this turn ends` },
      noState: { session: chat, note: restartNoteNoState(name) },
    });
  }

  // **A chat's work link** (V60, ADR 0088 §3): which work item it works on. Above the line: it
  // ends nothing, and an unlink only ends the link. The words are the operator's ruling, the
  // same on every tab, so the note names the chat; a chat that is not in a workspace has no row,
  // because the window does not offer it there (§4).
  for (const tab of now.tabs.order) {
    const chat = chatOf(now.tabs, tab);
    if (chat === undefined || !(now.linkable?.(chat) ?? false)) continue;
    const said = `Chat ${now.tabs.byId[tab].name}`;
    offers.push({
      ...can(`tab.worklink:${tab}`, "Link to work item…", { verb: "linkWorkItem", tab }),
      note: said,
    });
    const item = now.workItems?.[chat];
    if (item !== undefined) {
      offers.push({
        ...can(`tab.workunlink:${tab}`, "Unlink work item", { verb: "unlinkWorkItem", tab }),
        note: `${said} · ${workItemSaid(item)}`,
      });
    }
  }

  // **A preview tab can be kept** (SI-9b, ADR 0065 Q1): the next single click on a memory then
  // previews in a tab of its own. A double-click on the tab runs this same row, as VS Code's
  // does; a kept tab has nothing to keep, so it has no row.
  for (const tab of now.tabs.order) {
    const lead = contentsOf(now.tabs, tab)[0]?.content;
    if (lead?.kind !== "view" || !lead.preview) continue;
    const name = now.tabs.byId[tab].name;
    offers.push({
      ...can(`tab.keep:${tab}`, `Keep tab ${name} open`, { verb: "keepTab", tab }, name),
      note: "The next memory you click opens in a tab of its own instead of replacing this one.",
    });
  }

  // **A tab wrapping up offers to stop** (ADR 0064): the chat was asked to write its record and
  // close. Cancelling ends nothing, so it is above the line, and first on the tab's menu.
  for (const tab of now.tabs.order) {
    const session = panesOf(now.tabs, tab).find((one) =>
      (now.wrappingUp ?? []).includes(one.session),
    )?.session;
    if (session === undefined) continue;
    const name = now.tabs.byId[tab].name;
    offers.push({
      ...can(
        smartCloseCancelId(tab),
        `Cancel smart close of ${name}`,
        { verb: "cancelSmartClose", session },
        name,
      ),
      note: "The chat stays open and running.",
    });
  }

  // The workspaces of this project, which is the axis the tmux frame had and the port lost
  // (ADR 0036). These rows are the workspace strip as well as palette rows — one place the
  // words and the availability are written down, the same rule the tab strip follows.
  for (const workspace of now.workspaces) {
    const [title, name] =
      workspace === OUTSIDE
        ? ["Focus the plane root", undefined]
        : [`Focus workspace ${workspace}`, workspace];
    offers.push(
      workspace === now.focused
        ? cannot(`workspace.focus:${workspace}`, title, "It is already focused.", name)
        : can(`workspace.focus:${workspace}`, title, { verb: "focusWorkspace", workspace }, name),
    );
    // **The plane root has a chat and a shell of its own, and nothing else** (SI-1): it is not
    // a workspace on the plane, so there is nothing on disk for a pin, its settings, a rename
    // or a delete to name. Its directory is the plane's, and a chat started there is told it
    // is in no workspace.
    if (workspace === OUTSIDE) {
      const noRoot = "The plane has not been read yet, so there is no root to start in.";
      offers.push(
        now.plane === undefined
          ? cannot("root.chat", "New chat at the project root", noRoot)
          : {
              ...can("root.chat", "New chat at the project root", {
                verb: "newChatIn",
                path: now.plane,
              }),
              note: "In no workspace: it looks after the project and names a workspace with -w.",
            },
        now.plane === undefined
          ? cannot(`shell.new:${OUTSIDE}`, "New shell at the project root", noRoot)
          : can(`shell.new:${OUTSIDE}`, "New shell at the project root", {
              verb: "newShell",
              workspace: OUTSIDE,
            }),
      );
      continue;
    }
    // A shell in this workspace's own directory, filed under it (SI-5): the workspace menu's
    // way to reach a terminal there without focusing it first.
    offers.push(
      can(
        `shell.new:${workspace}`,
        `New shell in ${workspace}`,
        { verb: "newShell", workspace },
        workspace,
      ),
    );
    const held = isPinned(pinned.workspaces, workspace);
    offers.push({
      ...can(
        `workspace.pin:${workspace}`,
        `${held ? "Unpin" : "Pin"} workspace ${workspace}`,
        { verb: "pinWorkspace", workspace, pinned: !held },
        workspace,
      ),
      note: held ? UNPIN_NOTE : PIN_NOTE,
    });
    // Its settings (charter-app#280): the Settings tab at this workspace's level (SE-20), told
    // apart by the name in the note, as `project.settings` rows are.
    offers.push({
      ...can(`workspace.settings:${workspace}`, "Workspace settings…", {
        verb: "openWorkspaceSettings",
        workspace,
      }),
      note: `${workspace}: Settings at its level — live, repos, extensions, appearance and plugins.`,
    });
    // Its cross-repo changes (charter#470), for the workspace in front of the operator: a view
    // tab keyed by the workspace and filed on its strip, which asks the forge when it opens and
    // when its Refresh is pressed, never on a switch.
    if (workspace === now.focused) {
      offers.push({
        ...can(`workspace.changes:${workspace}`, "Open changes", {
          verb: "openView",
          view: changesView(workspace),
          title: changesTitle(workspace),
        }),
        note: `${workspace}: each cross-repo change, each member's request and its checks.`,
      });
    }
    // LIVE or LOCAL (charter-app#301): the row says which way it goes, and asks before it does.
    const live = now.live?.includes(workspace) ?? false;
    offers.push({
      ...can(
        `workspace.live:${workspace}`,
        live ? `Make ${workspace} local…` : `Make ${workspace} live…`,
        { verb: "switchLive", workspace },
        workspace,
      ),
      note: live
        ? "Stop publishing its charter, memory and todos with the project."
        : "Publish its charter, memory and todos with the project.",
    });
    // A new name (charter#367). It asks first, in a dialog that takes the name; the core
    // refuses a taken or invalid one, and a chat running in it, in its own words.
    offers.push({
      ...can(
        `workspace.rename:${workspace}`,
        `Rename workspace ${workspace}…`,
        { verb: "renameWorkspace", workspace },
        workspace,
      ),
      note: "Its folder, its branches' folders and everything that names it. Not while a chat runs in it.",
    });
  }

  // **A workspace can be made from here, and this is the only row that offers it.** The name
  // it takes is checked by the core and by nothing written here: `workspace_create` goes
  // through `wscmd::create`, which is `charter workspace create`, so the app and a terminal
  // refuse the same names with the same sentence. A second alphabet in the window would be a
  // second answer to what a workspace may be called.
  const newWorkspace = "New workspace…";
  offers.push(
    now.plane === undefined
      ? cannot(
          "workspace.create",
          newWorkspace,
          "purlis found no project, so there is nowhere to make a workspace.",
        )
      : can("workspace.create", newWorkspace, { verb: "createWorkspace" }),
  );

  // The projects this window holds. Switching between them is navigation and not a state
  // change — the project left behind keeps every chat it had running — so these sit up here
  // with the tabs and the workspaces. Letting go of one is below the line, with the tab
  // closes it is the bigger version of.
  const projects = projectRows(now.projects ?? [], now.plane, pinned.projects, now.split);
  offers.push(
    projects.open,
    projects.create,
    projects.switcher,
    ...projects.switchTo,
    ...projects.pin,
    ...projects.settings,
    ...projects.saving,
    ...projects.window,
    ...projects.back,
  );
  // **Settings, beside the projects' own settings** (SE-16; charter-app#283 before it): it
  // opens at the focused level (SE-23, V89g) — the focused workspace's, else the project's, else
  // You, which is the machine's and not a project's, so the row is there with no project open
  // too, as `extensions.show` is. The app menu's Settings… (`⌘,`) runs the same verb.
  offers.push({
    ...can("settings.show", "Settings…", { verb: "openSettingsTab" }),
    note: "At the workspace you are in, else the project, else this machine's.",
  });
  // **And the You level by name** (SE-23, D-SE23e as amended): Settings… lands on the focused
  // level, so this machine's own settings get a row of their own, there with or without a
  // project, as Settings… is.
  offers.push({
    ...can("settings.you", "Your settings…", { verb: "openYourSettings" }),
    note: "Your text sizes and your editor, on this machine.",
  });
  offers.push(...settingsGroupRows(now.plane, now.focused === OUTSIDE ? undefined : now.focused));
  offers.push(...profilePageRows(now.plane, now.profiles));

  // **A finished task's Merge… and Discard branch…, on its row's menu** (#1534): for a task
  // that worked on a branch purlis cut for it. Each opens the task's Changes tab's own
  // question, which reads what the core says now; nothing is merged or discarded until it is
  // answered, and a merge the core would refuse is said instead. In the row's menu only
  // (D-1534-5): the Changes tab and the Dispatches row are where the palette's keyboard has them.
  for (const tasks of now.finished?.values() ?? [])
    for (const task of tasks) {
      if (task.branch === null || task.waits !== null) continue;
      offers.push({
        ...can(taskMergeId(task.id), "Merge…", { verb: "taskBranch", id: task.id, act: "merge" }),
        note: `Merge ${task.branch}, the branch of ${task.name}, into the branch it was cut from.`,
        menuOnly: true,
      });
      offers.push({
        ...can(taskDiscardId(task.id), "Discard branch…", {
          verb: "taskBranch",
          id: task.id,
          act: "discard",
        }),
        note: `Discard the folder of ${task.branch}, the branch of ${task.name}.`,
        menuOnly: true,
      });
    }

  // **Read again, while a read of the focused workspace stands refused** (#1244). ADR 0038 keeps
  // the bottom region unpressable, so its refusals had their way out only on the explorer's
  // Notices; with the explorer hidden there was nothing to press. The row is the explorer's own
  // Read again, and it exists only while there is something to read again (D-1244-2), as the
  // discard row exists only while a removal stands refused.
  if (now.readRefused === true && now.focused !== undefined && now.focused !== OUTSIDE)
    offers.push({
      ...can(READ_AGAIN, "Read the workspace again", { verb: "readAgain" }),
      note: `What purlis could not read in ${now.focused} is asked again.`,
    });

  // **The plane's personas, one row each** (charter-app#174). What the row opens is the
  // persona's view — its own tab — which is how the persona rows get a menu without a second
  // list being invented for them, and how a persona is reachable from the palette.
  //
  // **It opens the persona's own tab** — the operator's ruling of 2026-09-23, *"Its own tab"* —
  // which is charter's first built-in view, and the same verb an extension's view is opened by.
  //
  // **And a persona can be made, opened for editing and deleted from here (SI-3).** Making and
  // deleting are `charter persona create` and `remove`, through the core, so the window refuses
  // what a terminal refuses. Editing is the operator's own editor on the persona's `persona.md`:
  // a charter is prose, and charter draws no editor for it.
  const personaPlane = "purlis found no project, so there is nowhere to keep a persona.";
  offers.push(
    now.plane === undefined
      ? cannot("persona.create", "New persona…", personaPlane)
      : {
          ...can("persona.create", "New persona…", { verb: "createPersona" }),
          note: "Written as a draft in personas/<name>/persona.md, for you to finish in your editor.",
        },
  );
  for (const persona of now.personas ?? []) {
    offers.push(
      can(
        `persona.show:${persona}`,
        `Show what ${persona} is`,
        { verb: "openView", view: { from: null, view: "persona", key: persona }, title: persona },
        persona,
      ),
      {
        ...can(
          `persona.edit:${persona}`,
          `Edit ${persona}'s persona.md`,
          { verb: "editPersona", persona },
          persona,
        ),
        note: "Opens it in your editor — whatever your system opens a .md file with.",
      },
      {
        ...can(
          `persona.profile:${persona}`,
          `Set ${persona}'s profile…`,
          { verb: "setPersonaProfile", persona },
          persona,
        ),
        note: "The harness profile its chats start on: one of the project's profiles, or none.",
      },
    );
  }

  // **A new memory, in each store the window lists** (SI-9c, ADR 0065 Q9): the focused
  // workspace's journal, each persona's, and the shared store. The `+` on each memory list's
  // heading is this row, so the heading and the palette cannot disagree. Every one needs a
  // plane; a workspace's needs a workspace focused — the plane root has no journal (SI-1).
  // And the shared store's own list, which the Personas panel's "shared" row opens (Q6).
  if (now.plane !== undefined) {
    const made = (scope: MemoryScope, title: string, note: string): Offer => ({
      ...can(`memory.new:${scopeKey(scope)}`, title, { verb: "newMemory", scope }),
      note,
    });
    // **And each store's archive, in a tab of its own** (KN-4, D6): what Delete moved out of
    // it, to read and restore. The archive's button on each memory list's heading is this row.
    const archive = (scope: MemoryScope): Offer => ({
      // A verb for the row and the heading's button, which is a glyph alone; the tab it opens
      // is named for what it holds.
      ...can(`memory.archived:${scopeKey(scope)}`, `Open ${archiveWhere(scope)}`, {
        verb: "openView",
        view: archiveView(scope),
        title: archiveTitle(scope),
      }),
      note: `What was deleted from ${memoryOf(scope)}, to read and restore.`,
    });
    if (now.focused !== undefined && now.focused !== OUTSIDE) {
      const journal: MemoryScope = { kind: "workspace", name: now.focused };
      offers.push(
        made(
          journal,
          `New memory in ${now.focused}…`,
          `Recorded in ${now.focused}'s journal, as \`purlis workspace remember\` records one.`,
        ),
        archive(journal),
      );
    }
    for (const persona of now.personas ?? []) {
      const own: MemoryScope = { kind: "persona", name: persona };
      offers.push(
        made(
          own,
          `New memory for ${persona}…`,
          `Kept in personas/${persona}/memory/, as \`purlis persona remember\` keeps one.`,
        ),
        archive(own),
      );
    }
    offers.push(
      made(
        { kind: "shared" },
        "New shared memory…",
        "Kept in personas/_shared/memory/, which every persona reads.",
      ),
      archive({ kind: "shared" }),
      {
        ...can("memory.shared", "Open shared memory", {
          verb: "openView",
          view: SHARED_MEMORY_VIEW,
          title: SHARED_MEMORY_TITLE,
        }),
        note: "What every persona in this project reads, in a tab of its own.",
      },
    );
    // **The project's dispatches, in a tab of their own** (#1452): every dispatch its chats
    // made, with who asked, which persona, the outcome and the cost. The Sessions panel's
    // heading draws this row as a button.
    offers.push({
      ...can(DISPATCHES_SHOW, "Open dispatches", {
        verb: "openView",
        view: DISPATCHES_VIEW,
        title: DISPATCHES_TITLE,
      }),
      note: "The work this project's chats handed to other chats, in a tab of its own.",
    });
  }

  // **Curation actions (ADR 0061)**: one row per action a workspace, a persona or the plane is
  // offered, named `Curate <subject>: <label>`, and one row that cannot run per action the core
  // left out — never dropped silently. Above the line: an action opens a chat and types a
  // prompt, and nothing runs until the operator reads it and presses Enter.
  for (const subject of now.curations?.subjects ?? []) {
    offers.push(...curationRows(subject, now.curations?.cannot ?? null));
  }

  // **The plane's vaults, one row each, and each opens that vault's own tab** (charter-app#235)
  // — the row the Vaults panel runs when one of its rows is pressed, and the one the picker
  // runs. Then the picker itself, which is how "open a vault" is found by those words when the
  // name is not known, and the way to make one. Both need a plane; the picker needs a vault.
  const vaults = now.vaults ?? [];
  for (const vault of vaults) {
    offers.push(
      can(
        `vault.open:${vault}`,
        `Open vault ${vault}`,
        { verb: "openView", view: { from: null, view: "vault", key: vault }, title: vault },
        vault,
      ),
    );
  }
  const noVaultPlane = "purlis found no project, so there are no vaults to reach.";
  offers.push(
    now.plane === undefined
      ? cannot("vault.pick", "Open vault…", noVaultPlane)
      : vaults.length === 0
        ? cannot(
            "vault.pick",
            "Open vault…",
            "This project has no vaults yet. New vault… makes one.",
          )
        : can("vault.pick", "Open vault…", { verb: "pickVault" }),
    now.plane === undefined
      ? cannot("vault.create", "New vault…", noVaultPlane)
      : {
          ...can("vault.create", "New vault…", { verb: "createVault" }),
          note: "Kept in your system's credential store unless you choose another provider.",
        },
  );
  // **The focused workspace's open todos: closing one is above the line** (SI-3). It keeps a
  // trace — the journal records it before the todo goes — so it is not a loss. It names the
  // workspace it writes to, because that is the question a row about a todo has to answer.
  if (now.focused !== undefined && now.focused !== OUTSIDE) {
    for (const todo of now.todos ?? []) {
      // **Open it as a view tab** (#1214): the row the Todos panel's row runs, filed on the
      // focused workspace's strip, whose todo it is. A second open brings that tab forward.
      offers.push({
        ...can(
          todoOpenId(todo.slug),
          `Open todo: ${todo.title}`,
          {
            verb: "openView",
            view: todoView({ workspace: now.focused, slug: todo.slug }),
            title: todo.title,
          },
          todo.title,
        ),
        note: `${now.focused}'s todo, in a tab of its own.`,
      });
      offers.push({
        ...can(
          `todo.done:${todo.slug}`,
          `Mark done: ${todo.title}`,
          { verb: "closeTodo", workspace: now.focused, slug: todo.slug },
          todo.title,
        ),
        note: `Closes it in ${now.focused}; the workspace's journal records it.`,
      });
    }
  }
  // **The session records of the place in front, two rows each** (SI-8d): open one as a view
  // tab, and resume it as a new chat. Only the place in front's, because the Sessions panel the
  // rows stand beside is about that place — a workspace, or the plane root.
  const resume = (path: string, title: string, note: string): Offer => ({
    ...can(
      `session.resume:${path}`,
      `Resume session: ${title}`,
      { verb: "resumeSession", path },
      title,
    ),
    note,
  });
  for (const record of now.sessions ?? []) {
    offers.push(
      can(
        `session.open:${record.path}`,
        `Open session record: ${record.title}`,
        {
          verb: "openView",
          view: sessionView(record.path),
          title: sessionTitle(record.title),
        },
        record.title,
      ),
      resume(
        record.path,
        record.title,
        record.resumable
          ? "A new chat, given its conversation back, with the record in its briefing."
          : "A new chat with the record in its briefing — the record holds no conversation to give back.",
      ),
    );
  }
  // **Every open memory tab's own rows** (SI-9b): its heading's Edit and Delete are these, so
  // they are about THAT memory whichever place is in front. A new memory's tab has nothing yet
  // to edit or delete, and no rows.
  const memories = new Set<string>();
  for (const id of now.tabs.order) {
    for (const { content } of contentsOf(now.tabs, id)) {
      if (content.kind !== "view" || content.view.from !== null) continue;
      if (content.view.view !== MEMORY_VIEW || memories.has(content.view.key)) continue;
      const ref = memoryRefOf(content.view.key);
      if (ref === undefined || ref.slug === DRAFT) continue;
      memories.add(content.view.key);
      offers.push(
        ...memoryOffers(ref, now.tabs.byId[id]?.name ?? ref.slug, now.memoryStores, now.live),
      );
    }
  }

  // **And every open record tab's own Resume, whichever place is in front** (SI-8e). A record's
  // tab draws this row as its heading's button, so it is about THAT record: a tab left open on
  // another place's record, or on one the place in front no longer lists, still resumes it.
  const listed = new Set((now.sessions ?? []).map((record) => record.path));
  for (const id of now.tabs.order) {
    for (const { content } of contentsOf(now.tabs, id)) {
      if (content.kind !== "view" || content.view.from !== null) continue;
      if (content.view.view !== SESSION_VIEW || listed.has(content.view.key)) continue;
      listed.add(content.view.key);
      offers.push(
        resume(
          content.view.key,
          sessionTitleOf(now.tabs.byId[id]?.name ?? content.view.key),
          "A new chat from this record — given its conversation back where it holds one — with the record in its briefing.",
        ),
      );
    }
  }
  // **The focused workspace's clones, three rows each** (charter-app#174, GL-1). A clone is
  // where a chat can start, one level up from a branch's folder: open a tab there, cut a new branch in
  // it, or pick it as where every new chat starts. The first is the ordinary `New tab`'s picker
  // aimed at the clone for that one tab; the second opens the New branch dialog; the third is
  // the explorer's pick. None writes anything by itself, so all three are above the line.
  for (const { repo, path } of now.clones ?? []) {
    // Its own folder's cockpit first, as a branch's is (#1152): the explorer narrowed to the
    // clone, which changes nothing, and has no Merge or Done (D-1152-3).
    offers.push(
      can(`clone.focus:${repo}`, `Focus on repo ${repo}`, { verb: "focusRepo", repo }, repo),
    );
    offers.push(
      can(`clone.chat:${repo}`, `New tab in ${repo}`, { verb: "newTabIn", repo, path }, repo),
    );
    // The cut itself is a dialog away, so the row writes nothing either (GL-1).
    offers.push(
      can(`clone.branch:${repo}`, `New branch in ${repo}…`, { verb: "newBranch", repo }, repo),
    );
    const pick = `Start new chats in ${repo}`;
    offers.push(
      now.startsIn === path
        ? cannot(`clone.pick:${repo}`, pick, `New chats already start in ${repo}.`, repo)
        : can(`clone.pick:${repo}`, pick, { verb: "pickClone", repo, path }, repo),
    );
  }

  // **The repos the workspace names and nobody has cloned here, one row each, and one for all
  // of them** (#1215). Above the line: a clone adds a folder and touches nothing that is
  // there. A repo already being cloned says so instead of starting a second clone of itself.
  const workspace = now.focused;
  if (workspace !== undefined && workspace !== OUTSIDE) {
    const missing = now.absent ?? [];
    const busy = new Set(now.cloning ?? []);
    for (const repo of missing) {
      const title = `Clone ${repo}`;
      offers.push(
        busy.has(repo)
          ? cannot(cloneMissingId(repo), title, `${repo} is being cloned.`, repo)
          : {
              ...can(
                cloneMissingId(repo),
                title,
                { verb: "cloneMissing", workspace, repos: [repo] },
                repo,
              ),
              note: `${workspace} names it and it is not cloned on this machine.`,
            },
      );
    }
    // **And the way to take each out of the workspace** (#1228). Below the line, in a menu: the
    // workspace stops naming it. It asks first, and a repo being cloned is not offered it —
    // that clone would land in a workspace that no longer names it.
    for (const repo of missing) {
      const title = dropMembershipTitle(repo);
      offers.push(
        busy.has(repo)
          ? cannot(dropMembershipId(repo), title, `${repo} is being cloned.`, repo)
          : {
              ...can(
                dropMembershipId(repo),
                title,
                { verb: "askDropMembership", workspace, repo },
                repo,
              ),
              note: `${workspace} stops naming it. Nothing is deleted.`,
            },
      );
    }
    // Listed while something is missing, like the per-repo rows: with every repo cloned there
    // is nothing for it to be about, and a greyed row in every palette query that shares its
    // letters would be noise, not a reason (D-1215-3).
    const idle = missing.filter((repo) => !busy.has(repo));
    const all = "Clone all missing repos";
    if (missing.length > 0)
      offers.push(
        idle.length === 0
          ? cannot(CLONE_ALL_ID, all, "Every missing repo is being cloned.")
          : can(CLONE_ALL_ID, all, { verb: "cloneMissing", workspace, repos: idle }),
      );
  }

  // **And every view an approved extension offers, one row each.** The personas panel's heading
  // draws the same views as buttons for a pointer; this is how a keyboard reaches them, and it
  // is the same verb. The extension's id is in the words, because what is in force is shown
  // after approval and not only at it (ADR 0041 item 5). The whole plane's view, never
  // one persona's — a persona's is opened from that persona's own tab.
  for (const view of now.views ?? []) {
    offers.push(
      can(
        `view.open:${view.extension}/${view.id}`,
        `Open ${view.title} from ${view.extension}`,
        {
          verb: "openView",
          view: { from: view.extension, view: view.id, key: "" },
          title: view.title,
        },
        view.title,
      ),
    );
  }

  // **And every command an approved extension adds**, named with its name so where it came
  // from is on the row (charter-app#341). One that opens a view is the verb every view is opened
  // by; one that runs an action is the window's to ask about first.
  for (const command of now.commands ?? []) {
    const title = `${command.name}: ${command.title}`;
    const id = `ext.command:${command.extension}/${command.id}`;
    offers.push(
      command.does.kind === "open"
        ? can(
            id,
            title,
            {
              verb: "openView",
              view: { from: command.extension, view: command.does.view, key: "" },
              title: command.does.title,
            },
            command.title,
          )
        : can(
            id,
            title,
            {
              verb: "runAction",
              extension: command.extension,
              action: command.does.action,
              name: command.name,
            },
            command.title,
          ),
    );
  }

  // The chats inside the tab in front (#1487). Here, beside the other rows about what is in
  // front and above the line: none of them ends anything.
  offers.push(...tabChatRows(now));

  // The worktree of the chat in front. Merging is not destructive — it is fast-forward only
  // and never pushes — so it sits above the line; removing is below it.
  const inFront = frontWorktree(now, chatInFocus);
  const merge = "Merge this chat's branch into its clone";
  offers.push(
    "cut" in inFront
      ? can("worktree.merge", merge, { verb: "mergeWorktree", cut: inFront.cut })
      : cannot("worktree.merge", merge, inFront.why),
  );

  // **And one row per piece of the focused workspace** (charter-app#174). The explorer's rows
  // had no menu because the two rows above are about THE CHAT IN FRONT, and a piece nothing
  // is running in is not in front of anything. These name the piece, exactly as
  // `tab.close:<id>` names its tab and for the same reason: a destructive row whose target
  // the operator has to work out from somewhere else on the page is the defect, not the
  // feature. The merges are here, above the line; the removes are below with the rest.
  const pieces = now.pieces ?? [];
  const noPlane =
    now.plane === undefined ? "purlis found no project, so it cannot reach a branch." : undefined;
  for (const cut of pieces) {
    // The branch's cockpit first (FM-5): the explorer narrowed to it, which changes nothing.
    const focusOn = cut.branch ? `Focus on branch ${cut.branch}` : `Focus on folder ${cut.piece}`;
    offers.push(
      can(
        `worktree.focus:${idOf(cut)}`,
        focusOn,
        { verb: "focusBranch", cut },
        cut.branch || cut.piece,
      ),
    );
    // Reading first (RC-5): the piece's files, in the light editor. It reads and writes
    // nothing on disk until a file is picked, and then only reads.
    const browse = `Browse the files of ${cut.piece}`;
    offers.push(
      noPlane === undefined
        ? can(
            `worktree.files:${idOf(cut)}`,
            browse,
            { verb: "openView", view: pieceFilesView(cut), title: pieceFilesTitle(cut) },
            cut.piece,
          )
        : cannot(`worktree.files:${idOf(cut)}`, browse, noPlane, cut.piece),
    );
    // Of the branch, by its own name, which need not be the folder's; a folder git has on no
    // branch is named as a folder (#989). The name the row carries is the one it shows.
    const shown = cut.branch || cut.piece;
    // The branch's own folder, placed by the core (#1143): what its *Files* row offers too
    // (`fileRows` at the empty path), named here because the palette lists every branch's.
    const own = cut.branch ? `branch ${cut.branch}` : `folder ${cut.piece}`;
    for (const row of branchFolderRows(cut, own)) {
      offers.push(
        noPlane === undefined ? { ...row, name: shown } : cannot(row.id, row.title, noPlane, shown),
      );
    }
    const title = cut.branch
      ? `Merge branch ${cut.branch} into ${cut.repo}`
      : `Merge folder ${cut.piece} into ${cut.repo}`;
    offers.push(
      noPlane === undefined
        ? can(`worktree.merge:${idOf(cut)}`, title, { verb: "mergeWorktree", cut }, shown)
        : cannot(`worktree.merge:${idOf(cut)}`, title, noPlane, shown),
    );
    // Beside the merge, above the line: a declaration writes one line to the piece log and
    // touches neither the tree nor the branch (charter#368).
    const done = cut.branch ? `Mark branch ${cut.branch} done` : `Mark folder ${cut.piece} done`;
    offers.push(
      noPlane === undefined
        ? can(`worktree.done:${idOf(cut)}`, done, { verb: "declareWorktreeDone", cut }, shown)
        : cannot(`worktree.done:${idOf(cut)}`, done, noPlane, shown),
    );
  }

  // ----- destructive, and therefore last -----

  // A pane's close ends its chat exactly as a tab's does, so it says the same thing.
  // A pane showing a view closes and ends nothing, so it says so and is not asked about.
  const askedBy = now.askedBy ?? askedByOf(now.listed ?? []);
  offers.push(paneCloseOf(now.tabs, front?.focused, now.nameOf, askedBy));

  // **`End`, not `Close`** (charter-app#130). Closing a tab calls `close_session`, which ends
  // the program and takes the chat off the board — correct, and what the `×` has always done.
  // But `Close tab 3` reads as "hide this", and an operator tidying fifty tabs with no undo
  // was ending fifty live harnesses on that reading. The words are the fix: the row says what
  // it does, and every surface draws these words — the palette row, the `×`'s accessible name
  // and its tooltip are all this one string.
  //
  // **A tab showing only a view is closed, not ended** — nothing runs in it, so nothing is
  // killed and nothing is asked. A view's tab with a chat split beside it ends that chat, and
  // says so.
  for (const tab of now.tabs.order) {
    const name = now.tabs.byId[tab].name;
    const chats = panesOf(now.tabs, tab).length;
    // **A task's own tab has no close** (#1489, V100-38): the row its `−` runs sends the task
    // back into its session's tab, and ends nothing. The same id, so the tab's key, its menu
    // and its button all do the one thing a task's tab can do.
    const own = chatOf(now.tabs, tab);
    if (own !== undefined && askedBy(own) !== undefined) {
      offers.push({
        ...can(
          `tab.close:${tab}`,
          backTitle(now.tabs, own, now.nameOf, askedBy),
          { verb: "sendBack", session: own },
          name,
        ),
        note: BACK_NOTE,
      });
      continue;
    }
    if (chats === 0) {
      offers.push(
        can(`tab.close:${tab}`, `Close ${name}`, { verb: "closeTab", tab, ends: false }, name),
      );
      continue;
    }
    const title =
      chatOf(now.tabs, tab) === undefined
        ? `Close ${name} and end the chat beside it`
        : `End chat ${name}`;
    offers.push({
      ...can(`tab.close:${tab}`, title, { verb: "closeTab", tab, ends: true }, name),
      note: ENDS_IT,
    });
  }

  offers.push(...stopRows(now.listed ?? [], now.stopping ?? []));
  offers.push(...taskEndRows(now.listed ?? [], now.stopping ?? []));
  offers.push(...taskRows(now.listed ?? []));
  offers.push(...handedOffRows(now.listed ?? []));
  offers.push(...placeRows(now.tabs, now.listed ?? [], now.nameOf));
  offers.push(besideInFront(now.tabs, now.listed ?? [], now.nameOf));
  offers.push(...briefRows(now.listed ?? []));
  offers.push(...answerRows(now.listed ?? []));
  offers.push(...restartRows(now.listed ?? [], now.restartable));

  const remove = "Remove the folder of this chat's branch";
  offers.push(
    "cut" in inFront
      ? {
          ...can("worktree.remove", remove, {
            verb: "removeWorktree",
            cut: inFront.cut,
            force: false,
          }),
          note: KEEPS_THE_BRANCH,
        }
      : cannot("worktree.remove", remove, inFront.why),
  );

  for (const cut of pieces) {
    // The folder goes and the branch stays (ADR 0072 §4), so the row names the folder.
    const title = `Remove folder ${cut.piece} in ${cut.repo}`;
    offers.push(
      noPlane === undefined
        ? {
            ...can(
              `worktree.remove:${idOf(cut)}`,
              title,
              { verb: "removeWorktree", cut, force: false },
              cut.piece,
            ),
            note: KEEPS_THE_BRANCH,
          }
        : cannot(`worktree.remove:${idOf(cut)}`, title, noPlane, cut.piece),
    );
  }

  // **The one row that is absent rather than refused.** Every other unavailable action is
  // listed with its reason, because an operator cannot ask about an option they cannot see.
  // This one is different in kind: it discards work the core has just refused to discard, and
  // it is the operator's answer to a sentence they have read. A row permanently offering to
  // force is a destructive action nobody was warned about; there is nothing to warn about
  // until the refusal exists, and then the row appears beside it.
  //
  // **And it is beside THE ROW THAT WAS REFUSED**, not beside every removal there is. With
  // one removal per piece, a discard row that appeared for all of them would be fifty offers
  // to throw work away raised by one refusal about one piece.
  if (now.refused === "worktree.remove" && "cut" in inFront) {
    offers.push(
      can("worktree.discard", "Discard that work and remove the folder anyway", {
        verb: "removeWorktree",
        cut: inFront.cut,
        force: true,
      }),
    );
  }
  for (const cut of pieces) {
    if (now.refused !== `worktree.remove:${idOf(cut)}` || noPlane !== undefined) continue;
    offers.push(
      can(
        `worktree.discard:${idOf(cut)}`,
        `Discard that work and remove ${cut.piece} anyway`,
        { verb: "removeWorktree", cut, force: true },
        cut.piece,
      ),
    );
  }

  // **The most destructive row charter has**, and therefore the last one before the two that
  // lose nothing on disk. Deleting a workspace deletes its clones, its worktrees, its memory
  // and its todos, and there is no undo anywhere.
  //
  // **It carries no `force`, and that is the whole design.** `wscmd::work_at_risk` decides
  // whether anything would be discarded, inside `workspace_remove`; this row asks, the core
  // refuses, and forcing is the operator's answer to a sentence they have read — which is
  // `worktree.discard`'s rule one scope up. A row that offered to force would be charter
  // putting "delete this and everything unpushed in it" one keystroke from a palette.
  //
  // Not for the strip of chats outside every workspace: it is not a workspace on the plane,
  // and there is nothing on disk for a delete to name.
  for (const workspace of now.workspaces) {
    if (workspace === OUTSIDE) continue;
    offers.push({
      ...can(
        `workspace.remove:${workspace}`,
        `Delete workspace ${workspace}`,
        { verb: "removeWorkspace", workspace },
        workspace,
      ),
      note: DELETES_A_WORKSPACE,
    });
  }

  // **A persona, a vault and a todo can be deleted from here too (SI-3)**, and each asks
  // first where it cannot be undone. Deleting a persona removes its directory — definition,
  // memory, refs — and the core refuses one another persona still extends or uses. Deleting a
  // vault destroys a keyring vault's secrets, and the dialog takes its name typed back first.
  // Forgetting a todo drops it with nothing journalled; `todo.done` is the row that keeps a
  // trace, and it is above the line.
  for (const persona of now.personas ?? []) {
    offers.push({
      ...can(
        `persona.remove:${persona}`,
        `Delete persona ${persona}…`,
        { verb: "removePersona", persona },
        persona,
      ),
      note: `Deletes personas/${persona}/ — its definition, memory and refs. Its vault is left alone.`,
    });
  }
  for (const vault of vaults) {
    offers.push({
      ...can(
        `vault.remove:${vault}`,
        `Delete vault ${vault}…`,
        { verb: "removeVault", vault },
        vault,
      ),
      note: "A keychain vault's secrets are destroyed and cannot be recovered. It asks first.",
    });
  }
  const todoIn = now.focused;
  if (todoIn !== undefined && todoIn !== OUTSIDE) {
    for (const todo of now.todos ?? []) {
      offers.push({
        ...can(
          `todo.forget:${todo.slug}`,
          `Forget todo ${todo.title}`,
          { verb: "forgetTodo", workspace: todoIn, slug: todo.slug },
          todo.title,
        ),
        note: `Drops it from ${todoIn} with nothing journalled. Mark it done to keep a trace.`,
      });
    }
  }

  // Destructive, and therefore here: letting go of a project ends every chat in it. Nothing
  // of the project on disk goes — what is open is written into it first, and it opens again
  // with everything still in it (ADR 0033).
  //
  // **One row per project, never a "close the one in front"**, which is `tab.close:<id>`'s
  // shape one scope up. A window holding eight projects has eight things to let go of, and a
  // row that acts on whichever happens to be on screen is a destructive action whose target
  // the operator has to work out from somewhere else on the page.
  offers.push(...projects.close);

  // About the machine and not a project, so it is here with nothing open too — and low on
  // the list, because it is a row an operator runs once and a query should find the rows
  // about what is in front before it. The words are VS Code's for the same thing, which is
  // what an operator will type.
  offers.push({
    ...can("charter.installCli", "Install `purlis` command in PATH", { verb: "installCli" }),
    note: "Links the purlis this app ships into /usr/local/bin, so a terminal finds it. macOS asks for your password when that directory is not yours.",
  });

  offers.push(can("charter.quit", "Quit purlis", { verb: "quit" }));

  return offers;
}

/**
 * The pieces a Merge, Remove or Done is on its way for, and what it is doing to each (#1610).
 *
 * Those three run off the window's thread (#1007), so the window no longer holds a second
 * press until the first has landed. Without this, a double press of Merge, or Merge and Remove
 * together, ran both, and the second got git's own lock refusal, which says nothing useful.
 * One window is one webview, so a map in this module is the window's.
 */
const piecesBusy = new Map<string, string>();

/**
 * Sends one of a piece's mutating verbs unless one is already on its way for that piece.
 *
 * A second press of the same verb sends nothing and says nothing: the first press's answer is
 * the one to read. A different verb is refused with what the piece is busy with. The hold is
 * let go on every way out, landed, refused or thrown, so a refused verb can be pressed again.
 */
function onePerPiece(cut: Cut, doingNow: string, send: () => Promise<Ran>): Ran | Promise<Ran> {
  const key = JSON.stringify([cut.workspace, cut.repo, cut.piece]);
  const busy = piecesBusy.get(key);
  if (busy === doingNow) return DID;
  if (busy !== undefined) {
    return { ok: false, refused: `${cut.piece} is still ${busy}. Try again once that has landed.` };
  }
  piecesBusy.set(key, doingNow);
  let sent: Promise<Ran>;
  try {
    sent = send();
  } catch (err) {
    piecesBusy.delete(key);
    throw err;
  }
  return sent.finally(() => piecesBusy.delete(key));
}

/**
 * Carries out what a row says it does.
 *
 * **Called from an event handler and never while rendering**, which is what lets the verbs
 * it dispatches to reach the window's own live arrangement. It is also why this is a
 * function taking `Doing` rather than a closure baked into the offer: an offer is built
 * during a render and would otherwise be holding whatever the arrangement was then.
 *
 * A row that cannot run does nothing here as well as on screen. The two are separate
 * decisions on purpose — a surface that forgot to check `available` must not become the
 * place a refused action runs.
 */
export function perform(offer: Offer, doing: Doing): Ran | Promise<Ran> {
  if (!offer.available) return { ok: false, refused: offer.reason };
  const does = offer.does;
  switch (does.verb) {
    case "chat.new":
      doing.newChat();
      return DID;
    case "newShell":
      doing.newShell(...(does.workspace === undefined ? [] : [does.workspace]));
      return DID;
    case "split":
      doing.split(does.direction);
      return DID;
    case "closePane":
      doing.closePane();
      return DID;
    case "closeTab":
      doing.closeTab(does.tab);
      return DID;
    case "selectTab":
      doing.selectTab(does.tab);
      return DID;
    case "renameTab":
      doing.renameTab(does.tab);
      return DID;
    case "linkWorkItem":
      doing.linkWorkItem(does.tab);
      return DID;
    case "startFresh":
      doing.startFresh(does.tab);
      return DID;
    case "restartChat":
      doing.restartChat(does.tab);
      return DID;
    case "restartListed":
      doing.restartListed(does.session);
      return DID;
    case "askPersona":
      doing.askPersona(does.tab, does.persona);
      return DID;
    case "unlinkWorkItem":
      return doing.unlinkWorkItem(does.tab);
    case "pinTab":
      return doing.pinTab(does.tab, does.pinned);
    case "pinWorkspace":
      return doing.pinWorkspace(does.workspace, does.pinned);
    case "pinProject":
      return doing.pinProject(does.plane, does.pinned);
    case "focusWorkspace":
      doing.focusWorkspace(does.workspace);
      return DID;
    case "createWorkspace":
      doing.createWorkspace();
      return DID;
    case "removeWorkspace":
      doing.removeWorkspace(does.workspace);
      return DID;
    case "showChat":
      if (does.inside) doing.showChat(does.session, true);
      else doing.showChat(does.session);
      return DID;
    case "ownTab":
      doing.ownTab(does.session);
      return DID;
    case "beside":
      doing.beside(does.session);
      return DID;
    case "sendBack":
      doing.sendBack(does.session);
      return DID;
    case "showTabTasks":
      doing.showTabTasks(does.tab);
      return DID;
    case "showBrief":
      doing.showBrief(does.session);
      return DID;
    case "answerQuestion":
      doing.answerQuestion(does.session);
      return DID;
    case "ignoreNeedsYou":
      return doing.ignoreNeedsYou(does.session);
    case "cancelSmartClose":
      return doing.cancelSmartClose(does.session);
    case "dismissStopped":
      doing.dismissStopped(does.session);
      return DID;
    case "endTask":
      doing.endTask(does.session, does.way);
      return DID;
    case "stopChat":
      doing.stopChat(does.session, does.below);
      return DID;
    case "stopAllTasks":
      doing.stopAllTasks(does.session);
      return DID;
    case "openView":
      doing.openView(does.view, does.title);
      return DID;
    case "openMemory":
      doing.openMemory(does.ref, does.title, does.keep);
      return DID;
    case "editMemory":
      doing.editMemory(does.ref, does.title);
      return DID;
    case "archiveMemory":
      return doing.archiveMemory(does.ref, does.title);
    case "moveMemory":
      return doing.moveMemory(does.ref, does.title, does.to);
    case "newMemory":
      doing.newMemory(does.scope);
      return DID;
    case "keepTab":
      doing.keepTab(does.tab);
      return DID;
    case "resumeSession":
      return doing.resumeSession(does.path);
    case "runAction":
      return doing.runAction(does.extension, does.action, does.name);
    case "pickVault":
      doing.pickVault();
      return DID;
    case "createVault":
      doing.createVault();
      return DID;
    case "removeVault":
      doing.removeVault(does.vault);
      return DID;
    case "createPersona":
      doing.createPersona();
      return DID;
    case "editPersona":
      return doing.editPersona(does.persona);
    case "setPersonaProfile":
      doing.setPersonaProfile(does.persona);
      return DID;
    case "removePersona":
      doing.removePersona(does.persona);
      return DID;
    case "closeTodo":
      return doing.closeTodo(does.workspace, does.slug);
    case "forgetTodo":
      return doing.forgetTodo(does.workspace, does.slug);
    case "removeWorktree":
      return onePerPiece(does.cut, "being removed", () =>
        doing.removeWorktree(does.cut, does.force),
      );
    case "mergeWorktree":
      return onePerPiece(does.cut, "merging", () => doing.mergeWorktree(does.cut));
    case "declareWorktreeDone":
      return onePerPiece(does.cut, "being marked done", () => doing.declareWorktreeDone(does.cut));
    case "focusRepo":
      doing.focusRepo(does.repo);
      return DID;
    case "focusBranch":
      doing.focusBranch(does.cut);
      return DID;
    case "pickClone":
      doing.pickClone(does.repo, does.path);
      return DID;
    case "newBranch":
      doing.newBranch(does.repo);
      return DID;
    case "cloneMissing":
      return doing.cloneMissing(does.workspace, does.repos);
    case "askDropMembership":
      doing.askDropMembership(does.workspace, does.repo);
      return DID;
    case "newTabIn":
    case "newChatIn":
      doing.newChatIn(does.path);
      return DID;
    case "sendKey":
      return doing.sendKey(does.key);
    case "openProject":
      doing.openProject();
      return DID;
    case "createProject":
      doing.createProject();
      return DID;
    case "showExtensions":
      doing.showExtensions();
      return DID;
    case "showSideView":
      doing.showSideView(does.view);
      return DID;
    case "toggleRegion":
      doing.toggleRegion(does.region);
      return DID;
    case "installCli":
      return doing.installCli();
    case "selectProject":
      doing.selectProject(does.plane);
      return DID;
    case "switchProject":
      doing.switchProject();
      return DID;
    case "closeProject":
      return doing.closeProject(does.plane);
    case "moveProject":
      return doing.moveProject(does.plane, does.to);
    case "openSettings":
      doing.openSettings(does.plane);
      return DID;
    case "openSaving":
      doing.openSaving(does.plane);
      return DID;
    case "openWorkspaceSettings":
      doing.openWorkspaceSettings(does.workspace);
      return DID;
    case "switchLive":
      doing.switchLive(does.workspace);
      return DID;
    case "renameWorkspace":
      doing.renameWorkspace(does.workspace);
      return DID;
    case "openSettingsTab":
      doing.openSettingsTab();
      return DID;
    case "openYourSettings":
      doing.openYourSettings();
      return DID;
    case "readAgain":
      doing.readAgain();
      return DID;
    case "taskBranch":
      doing.taskBranch(does.id, does.act);
      return DID;
    case "openSettingsGroup":
      // Through the window's link into Settings (SE-22), which brings the level's tab forward
      // with the group shown and puts the keyboard in it. With no project there is no window
      // of one to ask: the group is shown at You's place and Your settings… opens it.
      if (does.plane === undefined) {
        linkToGroup(settingsPlace("you"), does.group);
        doing.openYourSettings();
      } else
        askSettingsLink(does.plane, {
          group: does.group,
          ...(does.workspace === undefined ? {} : { workspace: does.workspace }),
        });
      return DID;
    case "curate":
      return doing.curate(does.subject, does.action);
    case "quit":
      doing.quit();
      return DID;
    case "copyPath":
      // The branch's own folder has no path in it to name (#1143): the sentence names the folder.
      if (does.at.path === "")
        return Promise.resolve(doing.copyPath(does.at, does.absolute)).then((ran) =>
          ran.ok
            ? { ok: true, said: `Copied the absolute path of ${does.at.piece ?? does.at.repo}.` }
            : ran,
        );
      return doing.copyPath(does.at, does.absolute);
    case "revealPath":
      return doing.revealPath(does.at);
    case "openInEditor":
      return doing.openInEditor(does.at, does.line);
    case "shellInFolder":
      doing.shellInFolder(does.at);
      return DID;
    case "startChatHere":
      return doing.startChatHere(does.at);
    case "addToChat":
      doing.addToChat(does.at, does.folder);
      return DID;
    case "nothing":
      return DID;
  }
}

/**
 * **One row per Settings group** (#1201; SE-22's follow-ups): "Your settings: Text", "Project
 * settings: Saving", "Workspace settings: Repos". Each level's groups are named after the row
 * that opens the level (Your settings…, Project settings…, Workspace settings…), so a label two
 * levels share — Appearance, Extensions, Plugins, Dispatch — is told apart by its level, and
 * typing the group's name finds every level's.
 *
 * You's groups are the machine's and are always listed; the project in front's when there is
 * one; and the focused workspace's only, as the Workspace settings… row is about the workspace
 * the panels show. The labels are `settings/catalogue.ts`'s, held to the builders by its test.
 */
function settingsGroupRows(plane: string | undefined, workspace: string | undefined): Offer[] {
  const opens = (group: string, extra: { plane?: string; workspace?: string }): Does => ({
    verb: "openSettingsGroup",
    group,
    ...extra,
  });
  const rows: Offer[] = SETTINGS_GROUPS.you.map((one) => ({
    ...can(
      `settings.group:${one.id}`,
      `Your settings: ${one.label}`,
      opens(one.id, plane === undefined ? {} : { plane }),
    ),
    note: `On this machine, at ${one.label}.`,
  }));
  if (plane === undefined) return rows;
  for (const one of SETTINGS_GROUPS.project)
    rows.push({
      ...can(
        `settings.group:${one.id}`,
        `Project settings: ${one.label}`,
        opens(one.id, { plane }),
      ),
      note: `The project's Settings, at ${one.label}.`,
    });
  if (workspace === undefined) return rows;
  for (const one of SETTINGS_GROUPS.workspace)
    rows.push({
      ...can(
        `settings.group:${one.id}:${workspace}`,
        `Workspace settings: ${one.label}`,
        opens(one.id, { plane, workspace }),
      ),
      note: `${workspace}: Settings at its level, at ${one.label}.`,
    });
  return rows;
}

/**
 * **One row per profile page** (#1201, D-1201-4): "Project settings: Profile claude", to the
 * profile's own page beneath Harness & profiles, through the same link as a group's row. A
 * profile is the project's, so there are none with no project in front. A committed profile has
 * no page of its own; its row lands on Harness & profiles, the longest group its address starts
 * with (`profileAddress.ts`).
 */
function profilePageRows(
  plane: string | undefined,
  profiles: readonly string[] | undefined,
): Offer[] {
  if (plane === undefined) return [];
  return (profiles ?? []).map((name) => {
    const group = profilePage(name);
    return {
      ...can(`settings.group:${group}`, `Project settings: Profile ${name}`, {
        verb: "openSettingsGroup",
        group,
        plane,
      }),
      note: `The project's Settings, at the profile ${name}.`,
    };
  });
}

/**
 * A queued chat's two rows: `needs.show:<session>`, the chat to the front, and its Ignore.
 *
 * The catalogue's, and ALSO asked on its own: the catalogue is built only for the project in
 * front, and the title bar's list (charter-app#249) holds every project's queue — so a project
 * behind the one on screen reports these rows for its chats without building the other 117.
 */
export function needsYouRows(
  needsYou: readonly number[],
  nameOf: (session: number) => string,
  tabs: Tabs,
  /** The chats that reported back to each chat asking (charter-app#259), so its row says so. */
  reportsTo: (session: number) => readonly string[] = () => [],
  /** What a chat's commits were refused for (SQ-16), so its row says the latest. */
  refusedIn: (session: number) => readonly string[] = () => [],
  /** Why a chat needs you when nothing it said itself says so (#1448), so its row says the
   *  latest. */
  neededFor: (session: number) => readonly string[] = () => [],
  /** Whether a chat is one the project lists (#1447). A task chat that needs you has no tab
   *  until it is shown, and showing it opens one, so its row can run (#1448). */
  listed: (session: number) => boolean = () => false,
  /** The chats each chat started that the operator stopped (#1448), so its row says so. */
  stoppedBelow: (session: number) => readonly string[] = () => [],
): Offer[] {
  return needsYou.flatMap((session) => {
    const name = nameOf(session);
    const refused = refusedIn(session);
    const needed = neededFor(session);
    const back = backSaid(reportsTo(session), stoppedBelow(session));
    // **Why it needs you first** (#1448): a reason of its own is what the person is asked to
    // look at, and what the chats it started did is the chat's to read.
    const title =
      needed.length > 0
        ? `Show ${name}: ${needed[needed.length - 1]}`
        : back !== undefined
          ? `Show ${name}: ${back}`
          : refused.length > 0
            ? `Show ${name}: ${refused[refused.length - 1]}`
            : `Show ${name}, which needs you`;
    return [
      tabHolding(tabs, session) === undefined && !listed(session)
        ? cannot(showId(session), title, "That chat has no tab in this window.", name)
        : can(showId(session), title, { verb: "showChat", session }, name),
      // **Ignore, until the chat asks again** (charter-app#248): the item's `✕`, Delete on
      // it, and this row in the palette are one row. Always available — ignoring is about
      // the request, and a chat asking from a tab this window does not hold is still asking.
      can(
        ignoreId(session),
        `Ignore ${name} until it asks again`,
        { verb: "ignoreNeedsYou", session },
        name,
      ),
    ];
  });
}

/**
 * **The rows of a chat whose Smart close stopped without its record** (SI-8f), for the chats
 * that are not already in the queue — a chat that is has its queue rows, and its row says why.
 * Go is the queue's own `needs.show:<session>`, and Dismiss takes the entry off the list: it is
 * the window's, so nothing is asked of the core.
 */
export function stoppedRows(
  stopped: Readonly<Record<number, string>>,
  needsYou: readonly number[],
  nameOf: (session: number) => string,
  tabs: Tabs,
): Offer[] {
  return Object.keys(stopped)
    .map(Number)
    .filter((session) => !needsYou.includes(session) && tabHolding(tabs, session) !== undefined)
    .flatMap((session) => {
      const name = nameOf(session);
      return [
        can(
          showId(session),
          `Show ${name}: ${stopped[session]}`,
          { verb: "showChat", session },
          name,
        ),
        can(
          dismissId(session),
          `Dismiss ${name}: ${stopped[session]}`,
          { verb: "dismissStopped", session },
          name,
        ),
      ];
    });
}

/** The catalogue's row for the oldest chat in the queue. */
const NEEDS_NEXT = "needs.next";

/** What the queue's rows are worded from, beside the queue itself. */
export type QueueNow = Pick<
  Now,
  | "tabs"
  | "nameOf"
  | "reportsTo"
  | "refusedIn"
  | "neededFor"
  | "listed"
  | "stoppedBelow"
  | "stopped"
>;

/**
 * **The catalogue with the needs-you queue put in** (#1034).
 *
 * The queue changes whenever a chat starts or stops asking for you, and the project view used to
 * read it at its top so that the catalogue could word these rows, which redrew every pane for a
 * change only the counts and the title bar's list show. So the view builds its catalogue with no
 * queue (`needsYou: []`, no `stopped`) and these rows go in when the queue is read, as the view
 * reports to the window, off the chats' store.
 *
 * `offers` is that queue-less catalogue. Its `needs.next` row stands where the queue's rows go,
 * which is where `catalogue` pushes them: the next chat's row, every queued chat's two rows, then
 * the rows of the chats whose Smart close stopped. The result is the catalogue `catalogue` would
 * have built with the queue (`actions.queue.test.ts` holds it to that). A catalogue that was not built
 * (a project not in front has none) is handed back as it is.
 */
export function withQueue(offers: Offer[], queue: readonly number[], now: QueueNow): Offer[] {
  const at = offers.findIndex((offer) => offer.id === NEEDS_NEXT);
  if (at < 0) return offers;
  const was = offers[at];
  const [oldest] = queue;
  // The queue-less row is the one that says nothing needs you, in the words `quiet` gives it; with
  // a chat in the queue it shows that chat.
  const next: Offer =
    oldest === undefined
      ? was
      : { ...was, available: true, reason: "", does: { verb: "showChat", session: oldest } };
  const listed = now.listed ?? [];
  return [
    ...offers.slice(0, at),
    next,
    ...needsYouRows(
      queue,
      now.nameOf,
      now.tabs,
      now.reportsTo,
      now.refusedIn,
      now.neededFor,
      (session) => listed.some((chat) => chat.session === session),
      now.stoppedBelow,
    ),
    ...stoppedRows(now.stopped ?? {}, queue, now.nameOf, now.tabs),
    ...offers.slice(at + 1),
  ];
}

/**
 * **A running chat's two Stop rows** (#1448): `chat.stop:<session>`, that chat alone, and
 * `chat.stop.below:<session>`, that chat and every chat nested under it. One pair per chat the
 * Chats section lists, so a task chat with no tab has them too.
 *
 * **Each asks first** (`stopChat`), and says there what it ends. A chat that is already
 * stopping is writing what it did, or waiting for the chats below it: its row then ends it
 * without waiting, and says so.
 * **They end chats, so they are last among a menu's rows**, under its line, as `End chat` is.
 */
export function stopRows(listed: readonly ListedChat[], stopping: readonly number[]): Offer[] {
  // What is below a chat is the tasks it asked for: a handoff is a session of its own (#1492).
  const started = tasksOf(listed);
  // A session's tasks, and not the chats it handed work off to: what Stop all tasks stops.
  const tasksOfIt = new Set(
    listed.flatMap((chat) =>
      chat.mode === "task" && chat.parent !== null && chat.parent !== chat.session
        ? [chat.parent]
        : [],
    ),
  );
  // **A task is not stopped by these** (#1488): it is ended one of its own two ways
  // (`taskEndRows`), which say what the chat that asked is told.
  return listed
    .filter((chat) => chat.mode !== "task")
    .flatMap((chat) => {
      const { session, name } = chat;
      const below = `Stop chat ${name} and everything below it`;
      const all = `Stop all tasks of ${name}`;
      if (stopping.includes(session))
        return [
          {
            ...can(
              stopId(session),
              `End chat ${name} now`,
              { verb: "stopChat", session, below: false },
              name,
            ),
            note: "It is being stopped. This ends it without waiting.",
          },
          // The chats under it are still running when it was stopped alone: this stops them
          // too, each in the ordinary way.
          ...(started.has(session)
            ? [
                {
                  ...can(
                    stopBelowId(session),
                    below,
                    { verb: "stopChat", session, below: true },
                    name,
                  ),
                  note: BELOW_TOO,
                },
              ]
            : []),
          // Stop all tasks stays beside it (#1498): stopped alone, its tasks still run.
          ...(tasksOfIt.has(session)
            ? [
                {
                  ...can(stopAllId(session), all, { verb: "stopAllTasks", session }, name),
                  note: STOPS_ALL,
                },
              ]
            : []),
        ];
      return [
        {
          ...can(
            stopId(session),
            `Stop chat ${name}`,
            { verb: "stopChat", session, below: false },
            name,
          ),
          note: STOPS_IT,
        },
        started.has(session)
          ? {
              ...can(stopBelowId(session), below, { verb: "stopChat", session, below: true }, name),
              note: BELOW_TOO,
            }
          : cannot(
              stopBelowId(session),
              below,
              `${name} started no chat that is still running.`,
              name,
            ),
        // **Stop all tasks** (#1498, V100-53): beside it, and the session keeps running.
        tasksOfIt.has(session)
          ? {
              ...can(stopAllId(session), all, { verb: "stopAllTasks", session }, name),
              note: STOPS_ALL,
            }
          : cannot(stopAllId(session), all, `${name} has no task open.`, name),
      ];
    });
}

/** Which of the two ways a person ends a task (the core's `Way`). */
export type TaskEndWay = "report" | "now";

/** The catalogue's id for a task's Stop and get its report row (#1488). */
export function taskStopId(session: number): string {
  return `task.stop:${session}`;
}

/** The catalogue's id for a task's Close now row (#1488). */
export function taskCloseId(session: number): string {
  return `task.close:${session}`;
}

/**
 * **The two rows that end a task, by id, in the order a menu lists them** (#1488): Stop and
 * get its report, then Close now. What a task's row menu lists under its line, what the
 * breadcrumb's two buttons are, and what a tab chip's menu mounts for a task (#1487). Both end
 * a task, so a menu draws them below its line.
 */
export function taskEndIds(session: number): [string, string] {
  return [taskStopId(session), taskCloseId(session)];
}

/** What Stop and get its report does that its title cannot fit. */
export const STOPS_THE_TASK =
  "Its turn is ended and it gets one short turn to say what it did, then it ends. The chat that asked is told you stopped it, with that report.";

/** What Close now does that its title cannot fit. */
export const CLOSES_THE_TASK =
  "Its program ends at once, with no report from it. The chat that asked is told you closed it.";

/**
 * **A task's two ending rows** (#1488, V100-5): `task.stop:<session>`, Stop and get its
 * report, and `task.close:<session>`, Close now. One pair per task the Chats section lists,
 * so a task with no tab has them, the palette finds them, and the key that asks to stop a
 * task (Delete on its row) presses the first.
 *
 * **Neither is the tab's close, and neither offers a Smart close**: a task writes its record
 * before it reports, and what a close would lose here is said by the row. **Neither ends
 * anything by being pressed**: the window asks the core what ending the task would do, and
 * then asks the person, in place for an idle task and in the one modal question for a task
 * mid-turn or with tasks at work below it (`endTask`).
 *
 * **On a harness purlis types nothing into, Stop is not offered** (V100-71): its row says
 * why, and Close now is the way. Where purlis may not type for a reason of the moment (a
 * prompt, the person's keys), the row is offered and the answer says so.
 *
 * **A task already being stopped is not stopped a second time**: its first row says why, and
 * Close now ends it without waiting for its turn.
 */
export function taskEndRows(listed: readonly ListedChat[], stopping: readonly number[]): Offer[] {
  return listed
    .filter((chat) => chat.mode === "task")
    .flatMap((chat) => {
      const { session, name } = chat;
      // Each begins with the words its button on the breadcrumb's line draws.
      const stop = `Stop and get its report: task ${name}`;
      const close = `Close now: task ${name}`;
      return [
        stopping.includes(session)
          ? cannot(
              taskStopId(session),
              stop,
              `${name} is being stopped already, and has one short turn to say what it did. Close now ends it without waiting.`,
              name,
            )
          : chat.typed === false
            ? // V100-71: purlis types nothing into this harness, so there is no turn to give
              // it. Said on the row, which is not offered; Close now is.
              cannot(
                taskStopId(session),
                stop,
                `purlis does not type into ${chat.harness ?? "the program this task runs"}, so it cannot ask ${name} for a report. Close now ends it.`,
                name,
              )
            : {
                ...can(
                  taskStopId(session),
                  stop,
                  { verb: "endTask", session, way: "report" },
                  name,
                ),
                note: STOPS_THE_TASK,
              },
        {
          ...can(taskCloseId(session), close, { verb: "endTask", session, way: "now" }, name),
          note: CLOSES_THE_TASK,
        },
      ];
    });
}

/**
 * **A row for each task** (#1499, V100-49), so the palette's search finds a task as it finds a
 * tab: `chat.show:<session>`, which shows the task as its row in the Chats list does, inside
 * the tab of the session that asked for it (#1486). Its title says where it works and who
 * asked, so either finds it. **Every task, shown or not**: a task on screen is in its
 * session's tab and has no `Switch to tab` row of its own, so leaving it out would make the
 * one the person is looking at the one the palette cannot find.
 */
export function taskRows(listed: readonly ListedChat[]): Offer[] {
  return listed
    .filter((chat) => chat.mode === "task")
    .map((chat) => {
      const asker = chat.from === null ? "" : `, asked by ${chat.from}`;
      return can(
        taskShowId(chat.session),
        `Show task ${chat.name} in ${chat.workspace}${asker}`,
        { verb: "showChat", session: chat.session },
        chat.name,
      );
    });
}

/**
 * **A row for each chat a chat's work was handed off to** (#1492, V100-69):
 * `chat.handed:<from>:<to>`, which goes to that chat as pressing its own row does. The
 * keyboard's way to where the work went: the words on the row of the chat it came from are a
 * pointer's, name only the newest, and are a tooltip on one line. The row's menu lists these
 * for its chat, the newest first, and the palette finds them by either chat's name.
 */
export function handedOffRows(listed: readonly ListedChat[]): Offer[] {
  const named = new Map(listed.map((chat) => [chat.session, chat.name]));
  return [...handedOff(listed)].flatMap(([from, went]) => {
    const by = named.get(from);
    // A chat that has closed has no row to hold a menu, and its handoffs say `from` it.
    if (by === undefined) return [];
    return went.map((chat) =>
      can(
        handedOffId(from, chat.session),
        `Go to ${chat.name} (handed off to by ${by})`,
        { verb: "showChat", session: chat.session },
        chat.name,
      ),
    );
  });
}

/** The catalogue's id for the row that goes to chat `to`, which chat `from` handed off to. */
export function handedOffId(from: number, to: number): string {
  return `chat.handed:${from}:${to}`;
}

/** The catalogue's id for the row that shows a task. */
export function taskShowId(session: number): string {
  return `chat.show:${session}`;
}

/** The catalogue's id for Restart chat on a chat's row in the Chats list. */
export function restartId(session: number): string {
  return `chat.restart:${session}`;
}

/**
 * **Restart chat, for a chat with no tab** (#1462): `chat.restart:<session>`, on its row in the
 * Chats list and in the palette. A task shown inside its session's tab, or not shown at all,
 * has no tab whose menu has the row (`tab.restart:<tab>`), so its own row offers it, with the
 * same words and the same wait for a turn to end. A chat with a tab is restarted from there,
 * so it is not offered twice. It ends a program, so it is under a menu's line.
 */
export function restartRows(
  listed: readonly ListedChat[],
  restartable: ((session: number) => boolean) | undefined,
): Offer[] {
  return listed
    .filter((chat) => !chat.tab && !chat.shell && (restartable?.(chat.session) ?? false))
    .map(({ session, name }) => ({
      ...can(restartId(session), `Restart chat ${name}`, { verb: "restartListed", session }, name),
      note: RESTART_NOTE,
      midTurn: { session, title: `Restart chat ${name} when this turn ends` },
      noState: { session, note: restartNoteNoState(name) },
    }));
}

/**
 * **Brief, for each task** (#1494, V100-45): `chat.brief:<session>`, which opens the panel
 * that shows the brief the task was sent, as it was sent. On the task's row menu in the Chats
 * list, on its tab's menu where it has a tab of its own, and in the palette. **A task only**:
 * a chat a person opened was sent nothing. It reads and changes nothing, so it is among a
 * menu's ordinary rows and never under its line.
 */
export function briefRows(listed: readonly ListedChat[]): Offer[] {
  return listed
    .filter((chat) => chat.mode === "task")
    .map((chat) => ({
      ...can(
        briefId(chat.session),
        `Brief of ${chat.name}`,
        { verb: "showBrief", session: chat.session },
        chat.name,
      ),
      note: BRIEF_SAYS,
    }));
}

/**
 * **Answer, for a task paused on a question to its asking chat** (#1496, #1551):
 * `chat.answer:<session>`, which opens a form that answers it as the person. On the task's row
 * menu in the Chats list, on its own tab's menu, and in the palette; a tab's menu offers it at
 * the end of the task's line. **Only while the task is asking**: the row is gone once it is
 * answered or has moved on, and a form opened just before says so itself. It sends nothing:
 * the form's Send does, so it is among a menu's ordinary rows.
 */
export function answerRows(listed: readonly ListedChat[]): Offer[] {
  return listed
    .filter((chat) => chat.mode === "task" && chat.asking !== null)
    .map((chat) => ({
      ...can(
        answerId(chat.session),
        `Answer ${chat.name}'s question`,
        { verb: "answerQuestion", session: chat.session },
        chat.name,
      ),
      note: ANSWER_SAYS,
    }));
}

/** The catalogue's id for a task's Answer row. */
export function answerId(session: number): string {
  return `chat.answer:${session}`;
}

/** What Answer says that its title cannot fit. */
export const ANSWER_SAYS =
  "Answer, as you, the question this task asked the chat that dispatched it. It carries on with your answer.";

/** The catalogue's id for a task's Brief row. */
export function briefId(session: number): string {
  return `chat.brief:${session}`;
}

/** What Brief shows that its title cannot fit. */
export const BRIEF_SAYS = "What this task was sent, as it was sent. Read-only.";

/** The catalogue's id for opening the task in front beside its session. */
export const BESIDE_ID = "chat.beside";

/** The catalogue's id for the row that opens task `session` beside its session (#1489):
 *  what Space on its row in the Chats list presses. */
export function besideId(session: number): string {
  return `chat.beside:${session}`;
}

/** The catalogue's id for the row that moves task `session` to a tab of its own (#1489). */
export function ownTabId(session: number): string {
  return `chat.own:${session}`;
}

/** The catalogue's id for the row that sends task `session` back out of its tab or pane. */
export function backId(session: number): string {
  return `chat.back:${session}`;
}

/**
 * **Whether a task's Start fresh runs** (#1609): the window half of lifting #1489's refusal.
 * On since the core's `Held::start_chat_fresh` hands a task its brief again (`rebrief::again`)
 * where the digest this launch kept confirms it, and refuses it otherwise, which the row's
 * question says as any refused act is.
 */
export const TASKS_START_FRESH = true;

/** What a task's Start fresh does that its title cannot fit (#1609). */
export const TASK_FRESH_NOTE =
  "A task starts again on the brief it was given, where purlis can confirm it, and still owes its report.";

/** Why a task is not started fresh: the core's own sentence (`A_TASK_IS_NOT_STARTED_FRESH`). */
export const TASK_NOT_FRESH =
  "This chat is a task, and a fresh start would drop the brief it was given. Restart chat keeps its conversation. To change what it was asked, stop it and ask again.";

/** What moving a task to its own tab does that its title cannot fit. */
export const OWN_TAB_NOTE =
  "It gets a tab on the strip with − in place of ×. − sends it back into its session's tab. Its − ends nothing.";

/** What opening a task beside its session does that its title cannot fit. */
export const BESIDE_NOTE =
  "Splits its session's tab: the session's chat on one side, the task on the other. Space on its row in the Chats list.";

/** What sending a task back does that its title cannot fit. */
export const BACK_NOTE = "The task goes on. Its session's tab still lists it.";

/** Why Space on a row of the Chats list does nothing for a chat that is not a task. */
export const ONLY_A_TASK_OPENS_BESIDE =
  "Only a task opens beside the chat that asked for it. Press Enter to show this chat.";

/**
 * **What sending a task back is called**, by where it is and where it goes (#1489): out of a
 * tab of its own, back into its session's tab, which is the name its minimise button says;
 * out of a pane beside its session, back among that session's tasks; and for a task whose
 * session has no tab in this window, back to the Chats list, which is the one place it is.
 */
export function backTitle(
  tabs: Tabs,
  task: number,
  nameOf: (session: number) => string,
  askedBy: AskedBy,
): string {
  const name = nameOf(task);
  const session = sessionOf(tabs, task, askedBy);
  if (session === undefined) return `Send ${name} back to the Chats list`;
  return placedOf(tabs, task, askedBy) === "beside"
    ? `Send ${name} back among ${nameOf(session.own)}'s tasks`
    : `Send ${name} back into ${nameOf(session.own)}'s tab`;
}

/**
 * **Where a task is drawn, as rows** (#1489, V100-38): for each task, a row that moves it to a
 * tab of its own, one that opens it beside its session, and, while it has a tab or a pane of
 * its own, one that sends it back. The rows a task's menu in the Chats list, its line in its
 * tab's menu and its pane's controls all press, and what the palette lists.
 *
 * A row that cannot run stays, with its reason: a task already in its own tab, one already
 * beside its session, one whose session has no tab to open it beside.
 */
export function placeRows(
  tabs: Tabs,
  listed: readonly ListedChat[],
  nameOf: (session: number) => string,
): Offer[] {
  const askedBy = askedByOf(listed);
  return listed
    .filter((chat) => chat.mode === "task")
    .flatMap((chat) => {
      const { session, name } = chat;
      const placed = placedOf(tabs, session, askedBy);
      const home = sessionOf(tabs, session, askedBy);
      const own = `Move ${name} to its own tab`;
      const beside =
        home === undefined
          ? `Open ${name} beside the chat that asked for it`
          : `Open ${name} beside ${nameOf(home.own)}`;
      const rows: Offer[] = [
        placed === "tab"
          ? cannot(ownTabId(session), own, `${name} is in a tab of its own.`, name)
          : {
              ...can(ownTabId(session), own, { verb: "ownTab", session }, name),
              note: OWN_TAB_NOTE,
            },
        home === undefined
          ? cannot(
              besideId(session),
              beside,
              `${chat.from ?? "The chat that asked for it"} has no tab in this window, so there is nothing to open ${name} beside.`,
              name,
            )
          : placed === "beside"
            ? cannot(besideId(session), beside, `${name} is open beside it.`, name)
            : {
                ...can(besideId(session), beside, { verb: "beside", session }, name),
                note: BESIDE_NOTE,
              },
      ];
      if (placed !== undefined)
        rows.push({
          ...can(
            backId(session),
            backTitle(tabs, session, nameOf, askedBy),
            { verb: "sendBack", session },
            name,
          ),
          note: BACK_NOTE,
        });
      return rows;
    });
}

/**
 * **Open the task in front beside its session** (`chat.beside`, #1499, #1489): the palette's
 * row for the task the focused pane shows. Space on a row of the Chats list presses that
 * row's own (`besideId`), so what opens is the row the key was pressed on.
 */
function besideInFront(
  tabs: Tabs,
  listed: readonly ListedChat[],
  nameOf: (session: number) => string,
): Offer {
  const title = "Open the task in front beside its session";
  const askedBy = askedByOf(listed);
  const shown = focusedChat(tabs);
  if (shown === undefined || askedBy(shown) === undefined)
    return cannot(
      BESIDE_ID,
      title,
      "The chat in front is not a task. Space on a task's row in the Chats list opens it beside the chat that asked for it.",
    );
  const home = sessionOf(tabs, shown, askedBy);
  if (home === undefined)
    return cannot(
      BESIDE_ID,
      title,
      `The chat that asked for ${nameOf(shown)} has no tab in this window, so there is nothing to open it beside.`,
    );
  if (placedOf(tabs, shown, askedBy) === "beside")
    return cannot(BESIDE_ID, title, `${nameOf(shown)} is open beside ${nameOf(home.own)}.`);
  return {
    ...can(BESIDE_ID, title, { verb: "beside", session: shown }),
    note: BESIDE_NOTE,
  };
}

/** What stopping everything below does that its title cannot fit. */
export const BELOW_TOO =
  "Every task it asked for, and every task those asked for, deepest first. A chat it handed work off to is not one of them, and nothing outside them is touched.";

/** What Stop does that its title cannot fit. */
export const STOPS_IT =
  "A chat another chat started gets one short turn to write what it did, then it ends. The chat that asked is told you stopped it.";

/** The catalogue's id for a chat's Stop row (#1448). */
export function stopId(session: number): string {
  return `chat.stop:${session}`;
}

/** The catalogue's id for a chat's Stop-and-everything-below row (#1448). */
export function stopBelowId(session: number): string {
  return `chat.stop.below:${session}`;
}

/** The catalogue's id for a session's Stop all tasks row (#1498). */
export function stopAllId(session: number): string {
  return `chat.stop.tasks:${session}`;
}

/** What Stop all tasks does that its title cannot fit. */
export const STOPS_ALL =
  "Every task it asked for that is still at work, and every task those asked for, deepest first. Each gets one short turn to say what it did, and is told of as stopped by you. The chat itself keeps running.";

/** The catalogue's id for a stopped smart close's Dismiss row (SI-8f). */
export function dismissId(session: number): string {
  return `needs.dismiss:${session}`;
}

/** The catalogue's id for a wrapping-up tab's Cancel smart close row (ADR 0064). */
/** What starts the id of every Move row of the memory `key` (#1190). */
const movePrefix = (key: string) => `memory.move:${key}:`;

/**
 * The catalogue's id for moving the memory `key` into the store `to` (#1190, D-1190-1):
 * `memory.move:<key>:<store>`, the store as {@link scopeKey} names it. A memory's key and a
 * store's are both slashes and names, never a colon, so the last colon is where the store
 * starts and the prefix up to it is this memory's alone.
 */
export function memoryMoveId(key: string, to: MemoryScope): string {
  return `${movePrefix(key)}${scopeKey(to)}`;
}

/**
 * One memory's rows (SI-9b, ADR 0065 Q12): Open, Edit and Delete, named for its store and slug
 * (`memory.<verb>:<key>`), and a Move into each of `stores` but its own (#1190). The catalogue
 * carries them for every open memory tab; a list of memories adds them for its own rows, whose
 * titles it has — a persona's tab, and SI-9c's workspace and shared lists.
 *
 * **A Move row says who reads the store it moves into** where the project publishes that store:
 * a persona's, shared memory, or the journal of one of the `live` workspaces
 * (`memoryMoves.publishedSaid`, worded with the tab's Move help). It asks nothing more than the
 * tab's Move asks: a pick of the store, then the move, with Undo after.
 */
export function memoryOffers(
  ref: MemoryRef,
  title: string,
  stores: readonly MemoryScope[] = [],
  /** The project's LIVE workspaces, whose journals it publishes (#1190). */
  live: readonly string[] = [],
): Offer[] {
  const key = memoryKey(ref);
  const here = scopeKey(ref.scope);
  const moves = stores
    .filter((to) => scopeKey(to) !== here)
    .map((to): Offer => {
      const row = can(
        memoryMoveId(key, to),
        `Move memory to ${memoryOf(to)}: ${title}`,
        { verb: "moveMemory", ref, title, to },
        title,
      );
      const said = publishedSaid(to, live);
      return said === undefined ? row : { ...row, note: said };
    });
  return [
    can(
      `memory.open:${key}`,
      `Open memory: ${title}`,
      { verb: "openMemory", ref, title, keep: false },
      title,
    ),
    can(`memory.edit:${key}`, `Edit memory: ${title}`, { verb: "editMemory", ref, title }, title),
    {
      ...can(
        `memory.delete:${key}`,
        `Delete memory: ${title}`,
        { verb: "archiveMemory", ref, title },
        title,
      ),
      note: "Moves it to the store's archive. Undo puts it back.",
    },
    ...moves,
  ];
}

/**
 * The memory `key`'s Move rows, in the catalogue's order: what a row's "Move to ▸" submenu
 * draws (#1190). A scan, as {@link curateRows} is and for its reason: which stores a project has
 * is not a list of ids known ahead, and the submenu is drawn only while its menu is open.
 */
export function moveRows(key: string, offers: Catalogued): Offer[] {
  const prefix = movePrefix(key);
  const rows: Offer[] = [];
  for (const [id, offer] of offers) if (id.startsWith(prefix)) rows.push(offer);
  return rows;
}

/** The memory a row opens — its key, out of the row's `memory.open:<key>` — or `undefined`. */
export function memoryKeyRun(runs: string | null | undefined): string | undefined {
  const prefix = "memory.open:";
  return runs?.startsWith(prefix) ? runs.slice(prefix.length) : undefined;
}

/**
 * **A memory list's own rows** (SI-9b, SI-9c): Open, Edit and Delete for each memory row in
 * `blocks`, named for its store and slug and titled with its row's words. The catalogue has no
 * list of every memory in the plane, and needs none: the list that draws the row supplies its
 * rows — a persona's tab, the shared list, a workspace's Memory section, all through this.
 */
export function listedMemoryOffers(
  blocks: readonly PanelBlock[],
  /** The project's stores, for each row's Move rows (#1190): none while they are unread. */
  stores: readonly MemoryScope[] = [],
  /** The project's LIVE workspaces, for what a Move row into one's journal says (#1190). */
  live: readonly string[] = [],
): Catalogued {
  return catalogued(
    blocks.flatMap((block) =>
      block.kind !== "list"
        ? []
        : block.rows.flatMap((row) => {
            const ref = memoryRefOf(memoryKeyRun(row.runs) ?? "");
            return ref === undefined ? [] : memoryOffers(ref, row.text, stores, live);
          }),
    ),
  );
}

/**
 * What a double-click on a row runs: the row a single click runs, **kept** where it opens a
 * preview (SI-9b, ADR 0065 Q1), and the same row otherwise — a row with no preview is its own
 * double-click.
 */
export function toKeep(offer: Offer): Offer {
  return offer.does.verb === "openMemory" && !offer.does.keep
    ? { ...offer, does: { ...offer.does, keep: true } }
    : offer;
}

/** How a chat's work item is shown: in its tab's tooltip, its pane's corner and its rows (V60). */
export function workItemSaid(key: string): string {
  return `Work item: ${key}`;
}

export function smartCloseCancelId(tab: number): string {
  return `tab.smartclose.cancel:${tab}`;
}

/** The catalogue's id for a queued chat's Go row, for a surface drawing that row. */
export function showId(session: number): string {
  return `needs.show:${session}`;
}

/** The catalogue's id for a queued chat's Ignore row, for a surface drawing that row. */
export function ignoreId(session: number): string {
  return `needs.ignore:${session}`;
}

/**
 * What the queue's row says when nothing has asked for the operator.
 *
 * "Nothing needs you." is a claim about every chat on the plane, and a harness that cannot
 * report a question asked mid-turn makes it one charter cannot stand behind (charter-app#52,
 * measured on codex-cli 0.147.0). So it is said only when every open chat can say what it is
 * doing; otherwise the row says what is actually known, which is less.
 */
function nothingSaidSoFar(quiet: readonly string[]): string {
  if (quiet.length === 0) return "Nothing needs you.";
  const who =
    quiet.length === 1
      ? `${quiet[0]} can be waiting on you without saying so`
      : `${quiet.length} chats can be waiting on you without saying so`;
  return `Nothing has said it needs you — and ${who}.`;
}

/**
 * The worktree the chat in front is working in, or why there is none.
 *
 * **The piece and the reason in one answer, rather than a reason and a second look at
 * `now.worktree`** (charter-app#174). A row about a worktree now carries the worktree, so
 * "can this row run" and "which piece does it mean" are the same question asked once — two
 * checks would let a row be available while carrying no piece, which is a defect nothing on
 * screen could show.
 */
function frontWorktree(now: Now, inFront: boolean): { cut: Cut } | { why: string } {
  if (!inFront) return { why: "No chat is in front." };
  if (now.plane === undefined)
    return { why: "purlis found no project, so it cannot reach a branch." };
  if (now.worktree === undefined)
    return { why: "The chat in front is not working in a branch purlis cut." };
  return { cut: now.worktree };
}

/**
 * **The close of one pane of the tab in front** (`pane.close`), decided for that pane: the
 * catalogue's row is the focused pane's, and a pane's own corner draws its own (#1486).
 *
 * A pane's close ends its chat exactly as a tab's does, so it says the same thing. A pane
 * showing a view closes and ends nothing, so it says so and is not asked about.
 *
 * **A pane showing a task has no close**: its close would end the session under the task,
 * which is not what is on screen, and ending a task is not a close. The reason names the
 * session that pane is. The tab's own close still ends the session, and says so.
 *
 * **A task's own pane is sent back** (#1489): the same id, a minimise, which ends nothing.
 */
export function paneCloseOf(
  tabs: Tabs,
  pane: number | undefined,
  nameOf: (session: number) => string,
  /** Who asked whom (`tabChats.askedByOf`): a task's own pane is sent back, never closed. */
  askedBy: AskedBy = () => undefined,
): Offer {
  const front = tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
  const content =
    front === undefined
      ? undefined
      : contentsOf(tabs, front.id).find((one) => one.pane === pane)?.content;
  if (content?.kind === "view")
    return can("pane.close", "Close this view", { verb: "closePane", ends: false });
  if (front === undefined || content === undefined || pane === undefined)
    return cannot(
      "pane.close",
      "End this pane's chat",
      "No chat is in front, so there is no pane to close.",
    );
  // **A task's own pane has a minimise and no close** (#1489): beside its session, or the one
  // pane of its own tab. It sends the task back, and ends nothing. Also while that pane shows
  // a task of the task: what goes back is the pane's own chat, with everything below it.
  if (askedBy(content.session) !== undefined)
    return {
      ...can(
        "pane.close",
        backTitle(tabs, content.session, nameOf, askedBy),
        { verb: "sendBack", session: content.session },
        nameOf(content.session),
      ),
      note: BACK_NOTE,
    };
  if (front.shows?.[pane] !== undefined) {
    const own = nameOf(content.session);
    return cannot(
      "pane.close",
      "End this pane's chat",
      `This pane shows a task of ${own}. Go back to ${own} to end its chat, or close the tab.`,
    );
  }
  return {
    ...can("pane.close", "End this pane's chat", { verb: "closePane", ends: true }),
    note: ENDS_IT,
  };
}

/** The four rows' ids, which their keys press (`taskKeys.TASK_KEY_ROW`). */
export const TAB_TASKS_ID = "tasks.menu";
export const NEXT_CHAT_ID = "tasks.next";
export const PREVIOUS_CHAT_ID = "tasks.previous";
export const OWN_CHAT_ID = "tasks.own";

/**
 * **The chats inside the tab in front** (#1487, V100-33, V100-36): its menu, the next and the
 * previous chat in it, and back to its session's own chat. Rows, so the palette lists them and
 * says their keys, and the keys press these rows and nothing of their own (`taskKeys.ts`).
 *
 * Next and previous are neighbours in the menu's order (`tabChats.chatsOfTab`), from the chat
 * the focused pane shows. Each is an ordinary `showChat`: the tab is switched and the chat's
 * terminal takes the keyboard, as a press on its row does. A tab with no tasks has all four,
 * unable to run and saying why.
 */
function tabChatRows(now: Now): Offer[] {
  const mac = onAMac();
  const titles = {
    tasks: "Show this tab's tasks",
    next: "Next chat in this tab",
    previous: "Previous chat in this tab",
    own: "Back to this tab's own chat",
  };
  const front = now.tabs.inFront === undefined ? undefined : now.tabs.byId[now.tabs.inFront];
  const pane = front && panesOf(now.tabs, front.id).find((one) => one.pane === front.focused);
  if (front === undefined || pane === undefined) {
    const why =
      front === undefined
        ? "No chat is in front."
        : "The pane in focus shows a view, not a chat, so it has no tasks.";
    return [
      cannot(TAB_TASKS_ID, titles.tasks, why),
      cannot(NEXT_CHAT_ID, titles.next, why),
      cannot(PREVIOUS_CHAT_ID, titles.previous, why),
      cannot(OWN_CHAT_ID, titles.own, why),
    ];
  }
  // A task in a tab of its own is listed by this tab's menu and is not a chat IN this tab:
  // next and previous stay inside the tab (#1489).
  const rows = chatsOfTab(now.tabs, front.id, now.listed ?? []).filter(
    (row) => row.placed !== "tab",
  );
  const shown = focusedChat(now.tabs);
  const showsTask = front.shows?.[pane.pane] !== undefined;
  const hasTasks =
    rows.some((row) => row.level > 1 || (now.finished?.get(row.session)?.length ?? 0) > 0) ||
    front.shows !== undefined;
  const none = `${front.name} has no tasks.`;
  const step = (id: string, title: string, by: 1 | -1, key: "next" | "previous"): Offer => {
    const to = hasTasks ? neighbour(rows, shown, by) : undefined;
    return to === undefined || to === shown
      ? cannot(id, title, hasTasks ? `${front.name} is the only chat in this tab.` : none)
      : {
          ...can(id, title, { verb: "showChat", session: to, inside: true }),
          note: `${taskKeySaid(key, mac)}.${taskKeyNote(key, mac)}`,
        };
  };
  return [
    hasTasks
      ? {
          ...can(TAB_TASKS_ID, titles.tasks, { verb: "showTabTasks", tab: front.id }),
          note: `${front.name} and the tasks it asked for, to switch between. ${taskKeySaid("menu", mac)}.`,
        }
      : cannot(TAB_TASKS_ID, titles.tasks, none),
    step(NEXT_CHAT_ID, titles.next, 1, "next"),
    step(PREVIOUS_CHAT_ID, titles.previous, -1, "previous"),
    showsTask
      ? {
          ...can(OWN_CHAT_ID, titles.own, { verb: "showChat", session: pane.session }),
          note: `Shows ${now.nameOf(pane.session)} again. The task goes on. ${taskKeySaid("own", mac)}.`,
        }
      : cannot(OWN_CHAT_ID, titles.own, "This tab is showing its own chat."),
  ];
}

/** The tab holding a session, or nothing when no tab does. */
function tabHolding(tabs: Tabs, session: number): number | undefined {
  return tabs.order.find((id) => panesOf(tabs, id).some((pane) => pane.session === session));
}

/** The rows the palette lists: the catalogue less the rows only a menu draws ({@link Offer}'s
 *  `menuOnly`). */
export function inPalette(offers: readonly Offer[]): Offer[] {
  return offers.filter((offer) => offer.menuOnly !== true);
}

/**
 * Whether a row survives what has been typed — a case-insensitive substring of what is
 * READABLE, which is the title and charter's own part of the id.
 *
 * **Never the reason.** That is charter's own sentence about why a row cannot run, and
 * matching it would make typing `plane` list every row that merely mentions one — a filter
 * answering a question nobody asked. `frame/palette.py` states it the same way about notes.
 *
 * **And the id only up to its colon.** Everything after it is a name — a tab's number, a
 * workspace's name, a session — which reaches the list through the TITLE, where the operator
 * can see it. Matching the whole id would make `7` list tab 17 and `alpha` list nothing it
 * does not already.
 *
 * JavaScript has no `casefold`, so this is `toLowerCase` on both sides: `ß` will not find
 * `ss`. Both sides get the same treatment, so a row is never unfindable by its own text.
 */
export function matches(query: string, offer: Offer): boolean {
  const want = query.toLowerCase();
  const verb = offer.id.split(":")[0].toLowerCase();
  return offer.title.toLowerCase().includes(want) || verb.includes(want);
}

/**
 * Whether the query found CHARTER'S OWN WORDS on this row, rather than only a name it
 * happens to carry.
 *
 * The title with the row's `name` taken out of it, plus charter's part of the id. `Switch to
 * tab release.3` answers no to `re` and yes to `switch`; `Remove the folder of this chat's
 * branch` has no name in it and answers yes to both. A row with no name is always its own words.
 */
function byItsWords(query: string, offer: Offer): boolean {
  const want = query.toLowerCase();
  if (offer.id.split(":")[0].toLowerCase().includes(want)) return true;
  const words = offer.name ? offer.title.split(offer.name).join(" ") : offer.title;
  return words.toLowerCase().includes(want);
}

/**
 * Whether this row is about the window AS IT STANDS, rather than about a thing it names.
 *
 * **It is the colon, and that is the same structural fact `matches` and `byItsWords` already
 * read.** An id with no colon is a verb with no object — `worktree.remove`, `pane.close`,
 * `chat.new`, `charter.quit` — and every one of those acts on what is in front of the
 * operator right now. An id with one names something else in the plane: a tab, a workspace, a
 * piece. This is not a tie-break invented for the ranking; it is the third use of the
 * distinction `Offer.id` is documented as carrying.
 *
 * **charter-app#174 is why it is used here.** Giving every piece of the focused workspace a
 * merge row put fifty rows of charter's own vocabulary between `re` and `Remove this chat's
 * worktree` — 4th of 62 to 54th of 164, measured. Every one of those fifty is a real row and
 * none of them is about the chat the operator is looking at. The rows in the way were not
 * somebody's chat names this time, which made it a different defect from #48 with the same
 * shape on screen; the operator does not get to care about the difference.
 */
function aboutWhatIsInFront(offer: Offer): boolean {
  return !offer.id.includes(":");
}

/**
 * The rows left after what has been typed, in the order they are shown.
 *
 * **Three stable groups, in this order, and inside each one the catalogue's own order:**
 *
 * 1. **The name you typed in FULL**, which is the row Enter runs.
 * 2. **A row your words found**, and inside that, **a row about what is in front before a row
 *    about something it names** (`aboutWhatIsInFront`).
 * 3. **A row that merely has those letters in somebody's name.**
 *
 * Still no score and still no cap: nothing is weighted, nothing moves relative to anything
 * else inside its group, and the same query always gives the same order. That matters at the
 * scale ADR 0026 writes the limits for, and both of the inner rules were bought with a
 * measurement:
 *
 * - **Group 2 before group 3 is charter-app#48.** At fifty chats `re` listed `Switch to tab
 *   release.3` and forty other names above `Remove this chat's worktree`.
 * - **The split inside group 2 is charter-app#174.** Giving every piece of the focused
 *   workspace its own merge row put fifty rows of charter's own vocabulary in front of the
 *   same target — 4th of 62 to 54th of 164, measured on the shape `actions.test.ts` builds.
 *   Those fifty are real rows about real worktrees and none of them is the one the operator
 *   is looking at. A regression with a respectable cause is still a regression, and #48's
 *   rule alone could not see this one: every row involved passes `byItsWords`.
 *
 * **It is ranking rather than filtering, and the measurement is why.** The tmux frame's
 * answer was to keep workspaces out of the browsable list; at fifty chats the rows burying
 * the verb are the TABS, which the frame kept, so dropping the workspaces would not have
 * moved the number it was meant to fix. The same holds one layer down: dropping the piece
 * rows would take a surface away to fix an ordering.
 */
export function narrow(query: string, offers: readonly Offer[]): Offer[] {
  const want = query.trim();
  if (want === "") return [...offers];
  const kept = offers.filter((offer) => matches(want, offer));
  const whole = want.toLowerCase();
  const exact = kept.filter((offer) => offer.title.toLowerCase() === whole);
  const rest = kept.filter((offer) => !exact.includes(offer));
  const found = rest.filter((offer) => byItsWords(want, offer));
  return [
    ...exact,
    ...found.filter(aboutWhatIsInFront),
    ...found.filter((offer) => !aboutWhatIsInFront(offer)),
    // Not split again: group 3 is by definition the rows the query found only inside a NAME,
    // so every row in it has one — and a row with a name has a colon.
    ...rest.filter((offer) => !byItsWords(want, offer)),
  ];
}

/**
 * The row Enter is aimed at: the first one that CAN run, or `-1` when none can.
 *
 * Aiming at the first row full stop would put Enter on a refused row whenever one sorts
 * first — the operator presses it, nothing happens, and the palette looks broken rather than
 * honest. The reason is on the row either way.
 */
export function aim(rows: readonly Offer[]): number {
  return rows.findIndex((row) => row.available);
}

// ----------------------------------------------------------------------------------------
// the context menus
// ----------------------------------------------------------------------------------------

/**
 * The thing a context menu was opened ON.
 *
 * A menu is about one item, and this is the item. It never carries what the rows would DO —
 * only enough to name them — because what they do is the catalogue's answer and a menu that
 * carried its own copy would be the second answer this module exists to not have.
 */
export type MenuOn =
  /** One chat, by the tab that holds it. Its rows are the same rows the tab strip draws. The
   *  chat's own number, where the tab holds one, is what its Stop rows are named by (#1448). */
  | { on: "chat"; tab: number; session?: number }
  /** One running chat, by its number: a row of the Chats section (#1447), which a task chat
   *  has before it has a tab. */
  | {
      on: "listed";
      session: number;
      /** The chats its work was handed off to, the newest first (#1492). */
      handed?: readonly number[];
    }
  | { on: "workspace"; workspace: string }
  /** The plane root's tab (SI-1): not a workspace, so a menu of its own. */
  | { on: "root" }
  | { on: "project"; plane: string }
  /** One worktree of the focused workspace, as the explorer's rows name it — the clone and
   *  the piece. Not a `Cut`: the workspace is the focused one on every surface that draws
   *  these, so carrying it would be a third copy of an answer the window already has. */
  | { on: "worktree"; repo: string; piece: string }
  | { on: "persona"; persona: string }
  /** One of the plane's vaults, by name — the Vaults panel's rows (SI-3). */
  | { on: "vault"; vault: string }
  /** One of the focused workspace's open todos, by slug — the Todos panel's rows (SI-3). The
   *  workspace is the focused one on the one surface that draws them. */
  | { on: "todo"; slug: string }
  /** One session record, by its plane-relative path — the Sessions panel's rows (SI-8d). */
  | { on: "session"; path: string }
  /** One memory, by its tab's key (`memories.memoryKey`) — a persona's memory rows (SI-9b). */
  | { on: "memory"; key: string }
  /** One clone of the focused workspace, by name — the explorer's clone heading and the bottom
   *  bar's repo row. The path is the catalogue's, so the menu does not carry it. */
  | { on: "clone"; repo: string }
  /** One repo the focused workspace names that is not cloned here — the explorer's "Not
   *  cloned here" rows and the bottom bar's (#1215). */
  | { on: "absent"; repo: string }
  /** One file or folder row of a branch's tree — the explorer's and the file tab's (FM-10):
   *  what it is as the tree drew it, and why it does not open when the tree said. */
  | FileOn
  /** The panes — the centre of the window, where a chat is. Not about any one pane: a split
   *  acts on the pane that has the keyboard, which is what the bar's buttons act on too. */
  | { on: "pane" }
  /** A refused read of the focused workspace, on the Changes view's line that says it
   *  (#1244; the bottom region's until #1676). */
  | { on: "refusal" }
  /** A finished task's row in the chats list, by its dispatch's id (#1534). */
  | { on: "finished"; id: string };

/**
 * Which rows a context menu on that item lists, **by catalogue id and in order**.
 *
 * This is the whole of a menu's content, and it is a list of NAMES. Nothing here knows what a
 * row says, whether it can run or what it does; [`menuRows`] looks each one up in the
 * catalogue the palette and the bar are already reading. That is the rule this module opens
 * with, applied to a third surface: an action is written down once, and a menu, a palette row
 * and a button cannot disagree about it because there is nothing for them to disagree with.
 *
 * **Destructive rows are `below`, and they are drawn under a separator.** The catalogue's own
 * rule is that they go last so that Enter in the palette is never one keystroke from ending a
 * chat; a menu pops up under the pointer, so the same rule matters more here, not less.
 *
 * **An id this catalogue does not have is simply not in the menu** — see [`menuRows`]. It is
 * how a row that is deleted from the catalogue takes its menu entry with it. The plane root,
 * which is not a workspace, has a menu of its own (`root`) rather than a workspace's with holes.
 */
export function menuOn(what: MenuOn): { above: string[]; below: string[] } {
  switch (what.on) {
    case "file":
      return { above: fileRows(what).map((row) => row.id), below: [] };
    case "chat":
      return {
        above: [
          smartCloseCancelId(what.tab),
          `tab.select:${what.tab}`,
          `tab.rename:${what.tab}`,
          `tab.pin:${what.tab}`,
          `tab.worklink:${what.tab}`,
          `tab.workunlink:${what.tab}`,
          // A task's own tab: beside its session instead (#1489). No row for a session.
          ...(what.session === undefined ? [] : [besideId(what.session)]),
          // A task: its Brief (#1494). An id the catalogue lacks is not in the menu.
          ...(what.session === undefined ? [] : [briefId(what.session)]),
          // A task asking its asking chat: the person may answer it (#1551).
          ...(what.session === undefined ? [] : [answerId(what.session)]),
          activityId(what.tab),
          networkId(what.tab),
        ],
        below: [
          `tab.restart:${what.tab}`,
          `tab.fresh:${what.tab}`,
          `tab.close:${what.tab}`,
          // A task has its own two rows and no Stop rows, and every other chat the reverse:
          // an id the catalogue does not have is not in the menu.
          ...(what.session === undefined
            ? []
            : [
                stopId(what.session),
                stopBelowId(what.session),
                stopAllId(what.session),
                ...taskEndIds(what.session),
              ]),
        ],
      };
    case "listed":
      return {
        above: [
          showId(what.session),
          briefId(what.session),
          answerId(what.session),
          ...(what.handed ?? []).map((to) => handedOffId(what.session, to)),
          // Where a task is drawn (#1489): a tab of its own, beside its session, or back.
          ownTabId(what.session),
          besideId(what.session),
          backId(what.session),
        ],
        below: [
          restartId(what.session),
          stopId(what.session),
          stopBelowId(what.session),
          stopAllId(what.session),
          ...taskEndIds(what.session),
        ],
      };
    case "workspace":
      return {
        above: [
          `workspace.focus:${what.workspace}`,
          `shell.new:${what.workspace}`,
          `workspace.pin:${what.workspace}`,
          `workspace.settings:${what.workspace}`,
          `workspace.live:${what.workspace}`,
          `workspace.rename:${what.workspace}`,
          "workspace.create",
        ],
        below: [`workspace.remove:${what.workspace}`],
      };
    case "root":
      return {
        above: [
          `workspace.focus:${OUTSIDE}`,
          "root.chat",
          `shell.new:${OUTSIDE}`,
          "workspace.create",
        ],
        below: [],
      };
    case "project":
      return {
        above: [
          `project.select:${what.plane}`,
          `project.pin:${what.plane}`,
          `project.settings:${what.plane}`,
          `project.saving:${what.plane}`,
          `project.window:${what.plane}`,
          `project.main:${what.plane}`,
          "project.create",
          "project.open",
        ],
        below: [`project.close:${what.plane}`],
      };
    case "worktree": {
      const at = `${what.repo}/${what.piece}`;
      return {
        above: [
          `worktree.focus:${at}`,
          `worktree.files:${at}`,
          `worktree.copypath:${at}`,
          `worktree.reveal:${at}`,
          `worktree.shell:${at}`,
          `worktree.merge:${at}`,
        ],
        // The discard row is listed and is almost never found: it exists only while a removal
        // of THIS piece has been refused and not answered. That is the whole reason a menu
        // lists ids rather than rows — nothing here has to know when it exists.
        below: [`worktree.remove:${at}`, `worktree.discard:${at}`],
      };
    }
    case "persona":
      // What charter can do to a persona from this window (SI-3): show it, hand its
      // definition to the operator's editor, make another, and delete it.
      //
      // **Curation is a group of its own**: the "Curate ▸" submenu (ADR 0061), which
      // `Menus.tsx` draws from `curateSubjectOf` and `curateRows` rather than from this list.
      return {
        above: [
          `persona.show:${what.persona}`,
          `persona.edit:${what.persona}`,
          `persona.profile:${what.persona}`,
          "persona.create",
        ],
        below: [`persona.remove:${what.persona}`],
      };
    case "vault":
      return {
        above: [`vault.open:${what.vault}`, "vault.create"],
        below: [`vault.remove:${what.vault}`],
      };
    case "todo":
      return {
        above: [todoOpenId(what.slug), `todo.done:${what.slug}`],
        below: [`todo.forget:${what.slug}`],
      };
    case "session":
      return { above: [`session.open:${what.path}`, `session.resume:${what.path}`], below: [] };
    case "memory":
      // Open and Edit, and under the line Delete (ADR 0065 Q12) — an archive with an Undo, but
      // it takes the memory out of every list, so it is drawn as the row that loses something.
      return {
        above: [`memory.open:${what.key}`, `memory.edit:${what.key}`],
        below: [`memory.delete:${what.key}`],
      };
    case "clone":
      // Where a chat can start, and a new branch beside it: a clone is the operator's own
      // checkout, and nothing in this window writes to its working tree. New branch adds a
      // branch and a folder of its own next to it (charter-app#174, GL-1).
      return {
        // And Read again, found only while a read stands refused (#1244): a tree purlis could not
        // read is said on the repo's own row, which this menu is on.
        above: [
          `clone.focus:${what.repo}`,
          `clone.chat:${what.repo}`,
          `clone.branch:${what.repo}`,
          `clone.pick:${what.repo}`,
          READ_AGAIN,
        ],
        below: [],
      };
    case "refusal":
      return { above: [READ_AGAIN], below: [] };
    case "finished":
      // Merge above; Discard, which removes the folder, below the line.
      return { above: [taskMergeId(what.id)], below: [taskDiscardId(what.id)] };
    case "absent":
      // Clone above; taking the repo out of the workspace below the line (#1228).
      return { above: [cloneMissingId(what.repo)], below: [dropMembershipId(what.repo)] };
    case "pane":
      return {
        above: ["chat.new", "shell.new", "pane.split.right", "pane.split.down", PASS_THROUGH_ID],
        below: ["pane.close"],
      };
  }
}

/**
 * The curation subject a context menu on that item offers a "Curate ▸" submenu for, in the
 * core's spelling, or none. A function of its own rather than a third list in [`menuOn`]: a
 * submenu is a group of rows the catalogue decides the length of, and `menuOn` is a list of
 * names it can know in advance.
 */
export function curateSubjectOf(what: MenuOn): string | undefined {
  switch (what.on) {
    case "workspace":
      return what.workspace === OUTSIDE ? undefined : `workspace:${what.workspace}`;
    case "persona":
      return `persona:${what.persona}`;
    // The plane root's tab is the plane's own place on the strip (SI-1), so the plane is
    // curated from it.
    case "root":
      return "plane";
    default:
      return undefined;
  }
}

/**
 * A subject's "Curate ▸" submenu, in the order it is drawn: charter's own actions, then a group
 * per declaring persona in the core's order, then every action the core left out.
 *
 * **A scan of the catalogue, and only when the submenu is drawn** — which is when its menu is
 * open. `Menus.tsx` calls it from inside the menu's content, which Radix mounts only while the
 * menu is up, so a strip of fifty tabs does not pay it per render.
 */
export function curateRows(
  subject: string,
  offers: Catalogued,
): { charter: Offer[]; personas: { persona: string; rows: Offer[] }[]; leftOut: Offer[] } {
  const prefix = curateId(subject);
  const charter: Offer[] = [];
  const personas: { persona: string; rows: Offer[] }[] = [];
  const leftOut: Offer[] = [];
  for (const [id, offer] of offers) {
    if (!id.startsWith(prefix)) continue;
    if (offer.group === LEFT_OUT || (offer.group === undefined && id.startsWith(`${prefix}!`))) {
      leftOut.push(offer);
    } else if (offer.group === undefined) {
      charter.push(offer);
    } else {
      const last = personas.at(-1);
      if (last?.persona === offer.group) last.rows.push(offer);
      else personas.push({ persona: offer.group, rows: [offer] });
    }
  }
  return { charter, personas, leftOut };
}

/** The row that says why no persona can be asked, where policy locks all dispatch. */
export const ASK_LOCKED_ID = "chat.ask";

/** What starts the id of every **Ask {persona}…** row of one tab. */
const askPrefix = (tab: number) => `tab.ask:${tab}:`;

/** The id of the row that asks `persona` from `tab`'s chat. */
export const askId = (tab: number, persona: string) => `${askPrefix(tab)}${persona}`;

/**
 * The **Ask {persona}…** rows of one chat tab's menu, in the catalogue's order: one per
 * persona the project has finished that this chat may ask, and none where dispatch is locked
 * or the tab is a shell.
 *
 * A scan, as {@link curateRows} is and for its reason: which personas a project has is not
 * something a list of ids can name ahead of time. It is made by a menu that is open, never by
 * every tab on a strip per render.
 */
export function askRows(tab: number, offers: Catalogued): Offer[] {
  const prefix = askPrefix(tab);
  const rows: Offer[] = [];
  // Only what can run: an ask policy locks is absent from the tab, and said in the palette.
  for (const [id, offer] of offers) if (id.startsWith(prefix) && offer.available) rows.push(offer);
  return rows;
}

/**
 * The catalogue with its rows reachable by id: what a menu, a strip and a button look one up
 * in.
 *
 * **Built once for the window, not once per surface that asks.** `menuRows` scanned the array,
 * and a scan is per menu per render: at ADR 0026's limits the chat strip draws fifty menus of
 * three ids each, over a catalogue that #174 grew from 183 rows to 291. Measured for one
 * render of that strip, min of ten batches with each arm in its own process:
 *
 * | one strip render | offers touched | scanning  | through this map |
 * |------------------|----------------|-----------|------------------|
 * | 183 rows         | 13,375         | 0.047 ms  | 0.017 ms         |
 * | 291 rows         | 16,275         | 0.049 ms  | 0.017 ms         |
 *
 * **31 µs is not a speed anybody feels, and that is not the argument.** CLAUDE.md says
 * optimise only against the spec's limits, and charter-app#133 measured a similar scan at
 * 22 µs and changed nothing. What is different here is the shape rather than the size: a scan
 * costs what the catalogue is long, and #174 is the change that made it longer — a third more
 * comparisons for the same fifty menus. This does not move when the list does. It is also
 * less code than the closure it replaces, which is the priority above speed.
 *
 * **It is not a memo and it is not a cache.** It is a value derived from `offers` and held
 * exactly where `offers` is held (`PlaneView`, `App`), so a surface handed one cannot be
 * holding a stale copy of a catalogue — the same reason an offer cannot: neither holds a copy
 * of anything. `Map` keeps insertion order, so nothing read back out of it is in a different
 * order from the array.
 */
export type Catalogued = ReadonlyMap<string, Offer>;

/** The catalogue, indexed. */
export function catalogued(offers: readonly Offer[]): Catalogued {
  return new Map(offers.map((offer) => [offer.id, offer]));
}

/**
 * The rows a context menu on that item draws: the catalogue's own offers, in menu order.
 *
 * **A row the catalogue does not have is dropped rather than invented**, which is `Doer`'s
 * rule for the bar's buttons and is the property that makes one catalogue enough. A menu on
 * the strip of chats outside every workspace has no pin row because the catalogue has no
 * `workspace.pin:outside/every/workspace`; nothing here had to be told about that case. It is
 * also how a worktree's `Discard that work…` row is in every worktree menu and is drawn in
 * none of them until the core has refused that piece's removal.
 *
 * **A row that cannot run is kept, with its reason**, exactly as the palette keeps it: an
 * operator cannot ask about an option they cannot see, and "It is already in front." on a
 * greyed row is an answer where a missing row is a mystery.
 */
export function menuRows(what: MenuOn, offers: Catalogued): { above: Offer[]; below: Offer[] } {
  // A file row's menu is its own short list, built for the row it was opened on: a branch
  // holds more files than any catalogue could list in advance (FM-10).
  if (what.on === "file") return { above: fileRows(what), below: [] };
  const found = (ids: readonly string[]) =>
    ids.map((id) => offers.get(id)).filter((row) => row !== undefined);
  const { above, below } = menuOn(what);
  // **A row that sends a task back ends nothing, so it is above the line** (#1489): a task's
  // own tab keeps the close's id for its minimise, and the close's id is listed below.
  const under = found(below);
  return {
    above: [...found(above), ...under.filter((row) => row.does.verb === "sendBack")],
    below: under.filter((row) => row.does.verb !== "sendBack"),
  };
}

// ----------------------------------------------------------------------------------------
// a branch's file rows (FM-10)
// ----------------------------------------------------------------------------------------

/** A file or folder row a menu was opened on. */
export type FileOn = {
  on: "file";
  at: BranchPath;
  /** What the tree drew it as: a folder, a file, or a link it never follows. */
  kind: "folder" | "file" | "link";
  /** Why the file does not open, in the core's sentence, when the tree said: an ignored file,
   *  a link out of the branch, a file the branch deleted. */
  refused?: string;
  /** The line last read in the file's preview, where it was read in this window (#1143,
   *  `editor/lastRead.ts`): where *Open in your editor* opens it. Line 1 when it was not. */
  line?: number;
};

/**
 * Every verb a file or folder row offers, and nothing else (FM-10, V86 F8).
 *
 * **Each changes nothing in the branch**: charter never creates, renames, moves or deletes a
 * file (ADR 0081), so it never races an agent writing the same tree. A copy, a reveal, your
 * editor and a shell are where a person changes files, and each is theirs. A test holds this
 * list to exactly these verbs, so a row that writes cannot be added to it quietly.
 */
export const FILE_VERBS = [
  "copyPath",
  "revealPath",
  "openInEditor",
  "shellInFolder",
  "startChatHere",
  "addToChat",
] as const;

/** The file manager's own name where the window runs. */
function fileManagerOf(platform: string): string {
  if (platform.startsWith("Mac")) return "Finder";
  if (platform.startsWith("Win")) return "File Explorer";
  return "Files";
}

/** What Reveal is called where the window runs: the file manager's own name. */
export function revealSaid(platform: string): string {
  return `Reveal in ${fileManagerOf(platform)}`;
}

/** The file manager on this platform. */
const FILE_MANAGER = fileManagerOf(typeof navigator === "undefined" ? "" : navigator.platform);

/** Reveal's words on this platform, said on its row. */
export const REVEAL_SAID = `Reveal in ${FILE_MANAGER}`;

/**
 * The rows of a branch's own folder (#1143), on its branch row and in the palette: its absolute
 * path copied, revealed, and a shell tab in it — each at the empty path, which the core places
 * as the branch's folder itself (`files::place_branch_folder`). No Copy relative path: the
 * folder's path relative to itself says nothing (D-1143-1). `what` names it, "branch fix/login"
 * or "folder fix-it".
 */
function branchFolderRows(cut: Cut, what: string): Offer[] {
  const at: BranchPath = { workspace: cut.workspace, repo: cut.repo, piece: cut.piece, path: "" };
  const id = idOf(cut);
  return [
    can(`worktree.copypath:${id}`, `Copy the absolute path of ${what}`, {
      verb: "copyPath",
      at,
      absolute: true,
    }),
    can(`worktree.reveal:${id}`, `Reveal ${what} in ${FILE_MANAGER}`, { verb: "revealPath", at }),
    can(`worktree.shell:${id}`, `Open a shell tab in ${what}`, { verb: "shellInFolder", at }),
  ];
}

/** Why a link's row copies only its relative path. */
export const NO_LINK_FOLLOWED =
  "purlis follows no link: it reveals and places only the branch's own files and folders.";

/**
 * The rows a file or folder's menu draws, in order (FM-10): copy its path, relative or
 * absolute; reveal it; your editor for a file or a shell tab for a folder; and a chat started
 * on the branch with the row already referenced (FM-9).
 *
 * **Built for the one row the menu was opened on**, never listed in the catalogue: a branch has
 * more files than any list could hold, and the menu is drawn only while it is open. Data like
 * every other offer — a `Does` that `perform` carries out — so the surfaces stay views of it.
 */
export function fileRows(what: FileOn): Offer[] {
  const { at, kind } = what;
  const key = `${at.workspace}/${at.repo}/${at.piece ?? ""}:${at.path}`;
  // The branch's own folder, its *Files* row (#1143): placed whole, with no relative path to
  // copy (D-1143-1) and no chat started on a reference to the whole branch (D-1143-2).
  if (at.path === "")
    return [
      can(`file.copyabsolute:${key}`, "Copy absolute path", {
        verb: "copyPath",
        at,
        absolute: true,
      }),
      can(`file.reveal:${key}`, REVEAL_SAID, { verb: "revealPath", at }),
      can(`file.shell:${key}`, "Open a shell tab here", { verb: "shellInFolder", at }),
    ];
  const placed = (id: string, title: string, does: Does): Offer =>
    kind === "link" ? cannot(id, title, NO_LINK_FOLLOWED) : can(id, title, does);
  const rows: Offer[] = [
    can(`file.copy:${key}`, "Copy relative path", { verb: "copyPath", at, absolute: false }),
    placed(`file.copyabsolute:${key}`, "Copy absolute path", {
      verb: "copyPath",
      at,
      absolute: true,
    }),
    placed(`file.reveal:${key}`, REVEAL_SAID, { verb: "revealPath", at }),
  ];
  if (kind === "folder") {
    rows.push(can(`file.shell:${key}`, "Open a shell tab here", { verb: "shellInFolder", at }));
  } else {
    // At the line its preview was last read at (#1143, D-1143-2), and line 1 when it was not.
    const line = what.line ?? 1;
    rows.push(
      what.refused !== undefined
        ? cannot(`file.editor:${key}`, "Open in your editor", what.refused)
        : line === 1
          ? can(`file.editor:${key}`, "Open in your editor", { verb: "openInEditor", at, line })
          : {
              ...can(`file.editor:${key}`, "Open in your editor", {
                verb: "openInEditor",
                at,
                line,
              }),
              note: `At line ${line}, where its preview was last read.`,
            },
    );
  }
  rows.push(placed(`file.chat:${key}`, "Start a chat here", { verb: "startChatHere", at }));
  // Into a chat that is open already (#1151): the preview's *Add to a chat's context*, from the
  // row, so the keyboard reaches it with the menu key. A link is refused as every placed row is.
  rows.push(
    placed(`file.addtochat:${key}`, "Add to a chat's context", {
      verb: "addToChat",
      at,
      folder: kind === "folder",
    }),
  );
  return rows;
}
