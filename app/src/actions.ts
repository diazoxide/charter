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
  MemoryScope,
  PanelBlock,
  RowAction,
  SubjectCurations,
} from "./bindings";
import { MAIN } from "./here";
import {
  DRAFT,
  MEMORY_VIEW,
  SHARED_MEMORY_TITLE,
  SHARED_MEMORY_VIEW,
  memoryKey,
  memoryRefOf,
  scopeKey,
  type MemoryRef,
} from "./memories";
import { pieceFilesTitle, pieceFilesView } from "./pieceViews";
import { SESSION_VIEW, sessionTitle, sessionTitleOf, sessionView } from "./sessions";
import { shellKeySaid } from "./shellKey";
import { switcherKeySaid } from "./switcherKey";
import { onAMac } from "./tabKeys";
import {
  changesTitle,
  changesView,
  chatOf,
  contentsOf,
  focusedContent,
  panesOf,
  viewKey,
  type Direction,
  type Tabs,
  type ViewRef,
} from "./tabs";

/** What running an action answered: one line to say, or a refusal in the words it came in. */
export type Ran = { ok: true; said?: string } | { ok: false; refused: string };

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
export const ROOT_TIP = "Plane — chats here start at the plane root";

/** The key that opens a shell tab, as this platform spells it — said on the row, so the palette
 *  is where an operator learns it (`shellKey.ts`). */
export const SHELL_KEY_SAID = shellKeySaid(onAMac());

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
  | { verb: "showChat"; session: number }
  /** Drops a chat's request for the operator until it asks again (charter-app#248). The chat
   *  itself is untouched; the core holds the ignore, so the window's queue is told, not kept. */
  | { verb: "ignoreNeedsYou"; session: number }
  /** Cancels a chat's smart close (ADR 0064): nothing is sent to it and nothing is closed — the
   *  chat stays open and running, and a record it writes afterwards closes nothing. */
  | { verb: "cancelSmartClose"; session: number }
  /** Takes a chat off the needs-you list where its Smart close stopped without a record
   *  (SI-8f). Nothing is asked of the core: the list's entry is the window's own. */
  | { verb: "dismissStopped"; session: number }
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
  /** Puts the app's own `charter` on a terminal's `PATH` — VS Code's "Install 'code' command
   *  in PATH". Only ever on this row: nothing links a command anywhere behind the operator's
   *  back (spec decision 21, `charter_core::clipath`). A refusal comes back as the core's
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
  /** Opens that project's Project settings tab (charter-app#252) — bringing the project to the
   *  front first when it is not. It writes nothing by itself: a save is the tab's, through the
   *  core's own checks. */
  | { verb: "openSettings"; plane: string }
  /** Opens that project's Saving tab (charter-app#294) — bringing the project to the front
   *  first when it is not. It saves nothing by itself: the save is the tab's button. */
  | { verb: "openSaving"; plane: string }
  /** Opens that workspace's Workspace settings tab (charter-app#280), on that workspace's strip.
   *  It writes nothing by itself: a save is the tab's, through the core's own checks. */
  | { verb: "openWorkspaceSettings"; workspace: string }
  /** Asks whether to make that workspace LIVE or LOCAL (charter-app#301): a confirmation that
   *  says what it publishes and where. Nothing changes until it is answered. */
  | { verb: "switchLive"; workspace: string }
  /** Asks for a workspace's new name. It renames nothing by itself: the name, the refusals and
   *  the move are the core's (`workspace_rename`, `wscmd::rename`), as they are for
   *  `charter workspace rename` (charter#367). */
  | { verb: "renameWorkspace"; workspace: string }
  /** Opens the Preferences tab (charter-app#283) — this machine's text sizes — on the project
   *  in front. It writes nothing by itself: a size is changed on the tab, or by its keys. */
  | { verb: "openPreferences" }
  /** Opens a new chat for one curation action on one subject, with the action's prompt typed
   *  into it and never sent (ADR 0061). It carries the action's id and nothing of its text: the
   *  core resolves the subject again (`curate`), so what is typed is the core's prompt now. */
  | { verb: "curate"; subject: string; action: string }
  | { verb: "quit" }
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
   * The group a submenu draws this row under, for a row that is in one: a curation action's
   * declaring persona, or [`LEFT_OUT`] for an action the core left out. None for charter's own
   * — they come first, ungrouped. The palette ignores it: its rows name their group in the
   * title already.
   */
  group?: string;
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
  /** Why each chat's Smart close stopped without its record (SI-8f), for its needs-you rows. */
  stopped?: Readonly<Record<number, string>>;
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
  showChat: (session: number) => void;
  /** Answers a `Ran`, because it is a command the core can refuse — a project closed meanwhile. */
  ignoreNeedsYou: (session: number) => Promise<Ran>;
  /** Answers a `Ran`, because the core can refuse it — a project closed meanwhile. */
  cancelSmartClose: (session: number) => Promise<Ran>;
  dismissStopped: (session: number) => void;
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
  /** Makes that clone where the next chat starts. It starts nothing, so it answers no `Ran`. */
  pickClone: (repo: string, path: string) => void;
  /** Opens the New branch dialog for that clone. It cuts nothing until it is answered. */
  newBranch: (repo: string) => void;
  /** Opens the picker for a new tab whose chat starts in that directory, and nowhere else. */
  newChatIn: (path: string) => void;
  sendKey: (key: string) => Promise<Ran>;
  openProject: () => void;
  /** Opens the new-project dialog. Nothing is scaffolded and nothing is opened until it is
   *  answered, and the open it ends in is the gated one. */
  createProject: () => void;
  showExtensions: () => void;
  installCli: () => Promise<Ran>;
  selectProject: (plane: string) => void;
  /** Opens the project switcher. Nothing is switched until a row in it is run. */
  switchProject: () => void;
  closeProject: (plane: string) => Promise<Ran>;
  /** Moves that project into another window, or a new one. The core can refuse. */
  moveProject: (plane: string, to: string | null) => Promise<Ran>;
  /** Brings that project to the front and opens its Project settings tab. */
  openSettings: (plane: string) => void;
  /** Brings that project to the front and opens its Saving tab. */
  openSaving: (plane: string) => void;
  /** Opens that workspace's settings tab on its strip, or brings forward the one already open. */
  openWorkspaceSettings: (workspace: string) => void;
  /** Asks whether to make that workspace LIVE or LOCAL, in a confirmation. */
  switchLive: (workspace: string) => void;
  /** Opens the rename dialog for one workspace. Nothing is renamed until it is answered. */
  renameWorkspace: (workspace: string) => void;
  /** Opens the Preferences tab, or brings forward the one already open. */
  openPreferences: () => void;
  /** Opens a curation chat. The core can refuse — the action gone, a harness that cannot be
   *  typed into — so it answers a `Ran`. */
  curate: (subject: string, action: string) => Promise<Ran>;
  quit: () => void;
};

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
      note: "Makes a plane in a directory of its own. It never writes into a repo you point at.",
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

/** Whether one of the operator's pins names this thing. */
function isPinned<T>(held: readonly T[], one: T): boolean {
  return held.includes(one);
}

/** An offer that can run, spelled once so `reason` cannot drift from `available`. */
function can(id: string, title: string, does: Does, name?: string): Offer {
  return { id, title, available: true, reason: "", does, name };
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
  offers.push(...needsYouRows(now.needsYou, now.nameOf, now.tabs, now.reportsTo, now.refusedIn));
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
          ? cannot("root.chat", "New chat at the plane root", noRoot)
          : {
              ...can("root.chat", "New chat at the plane root", {
                verb: "newChatIn",
                path: now.plane,
              }),
              note: "In no workspace: it looks after the plane and names a workspace with -w.",
            },
        now.plane === undefined
          ? cannot(`shell.new:${OUTSIDE}`, "New shell at the plane root", noRoot)
          : can(`shell.new:${OUTSIDE}`, "New shell at the plane root", {
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
    // Its settings (charter-app#280), under the words the project's row uses and told apart by
    // the name in the note, as `project.settings` rows are.
    offers.push({
      ...can(`workspace.settings:${workspace}`, "Workspace settings…", {
        verb: "openWorkspaceSettings",
        workspace,
      }),
      note: `${workspace}: its workspace.json, between charter.toml and charter.local.toml.`,
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
        ? "Stop publishing its charter, memory and todos with the plane."
        : "Publish its charter, memory and todos with the plane.",
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
          "charter found no plane, so there is nowhere to make a workspace.",
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
  // **This machine's preferences, beside the projects' settings** (charter-app#283): the text
  // sizes are the machine's and not a project's, so the row is there with no project open too,
  // as `extensions.show` is.
  offers.push({
    ...can("preferences.show", "Preferences…", { verb: "openPreferences" }),
    note: "This machine's window and terminal text sizes.",
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
  const personaPlane = "charter found no plane, so there is nowhere to keep a persona.";
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
    if (now.focused !== undefined && now.focused !== OUTSIDE) {
      offers.push(
        made(
          { kind: "workspace", name: now.focused },
          `New memory in ${now.focused}…`,
          `Recorded in ${now.focused}'s journal, as \`charter workspace remember\` records one.`,
        ),
      );
    }
    for (const persona of now.personas ?? []) {
      offers.push(
        made(
          { kind: "persona", name: persona },
          `New memory for ${persona}…`,
          `Kept in personas/${persona}/memory/, as \`charter persona remember\` keeps one.`,
        ),
      );
    }
    offers.push(
      made(
        { kind: "shared" },
        "New shared memory…",
        "Kept in personas/_shared/memory/, which every persona reads.",
      ),
      {
        ...can("memory.shared", "Open shared memory", {
          verb: "openView",
          view: SHARED_MEMORY_VIEW,
          title: SHARED_MEMORY_TITLE,
        }),
        note: "What every persona on this plane reads, in a tab of its own.",
      },
    );
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
  const noVaultPlane = "charter found no plane, so there are no vaults to reach.";
  offers.push(
    now.plane === undefined
      ? cannot("vault.pick", "Open vault…", noVaultPlane)
      : vaults.length === 0
        ? cannot("vault.pick", "Open vault…", "This plane has no vaults yet. New vault… makes one.")
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
      offers.push(...memoryOffers(ref, now.tabs.byId[id]?.name ?? ref.slug));
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
    now.plane === undefined ? "charter found no plane, so it cannot reach a branch." : undefined;
  for (const cut of pieces) {
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
  offers.push(
    focusedOn?.kind === "view"
      ? can("pane.close", "Close this view", { verb: "closePane", ends: false })
      : front
        ? {
            ...can("pane.close", "End this pane's chat", { verb: "closePane", ends: true }),
            note: ENDS_IT,
          }
        : cannot(
            "pane.close",
            "End this pane's chat",
            "No chat is in front, so there is no pane to close.",
          ),
  );

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
    ...can("charter.installCli", "Install `charter` command in PATH", { verb: "installCli" }),
    note: "Links the charter this app ships into /usr/local/bin, so a terminal finds it. macOS asks for your password when that directory is not yours.",
  });

  offers.push(can("charter.quit", "Quit charter", { verb: "quit" }));

  return offers;
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
      doing.showChat(does.session);
      return DID;
    case "ignoreNeedsYou":
      return doing.ignoreNeedsYou(does.session);
    case "cancelSmartClose":
      return doing.cancelSmartClose(does.session);
    case "dismissStopped":
      doing.dismissStopped(does.session);
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
    case "removePersona":
      doing.removePersona(does.persona);
      return DID;
    case "closeTodo":
      return doing.closeTodo(does.workspace, does.slug);
    case "forgetTodo":
      return doing.forgetTodo(does.workspace, does.slug);
    case "removeWorktree":
      return doing.removeWorktree(does.cut, does.force);
    case "mergeWorktree":
      return doing.mergeWorktree(does.cut);
    case "declareWorktreeDone":
      return doing.declareWorktreeDone(does.cut);
    case "pickClone":
      doing.pickClone(does.repo, does.path);
      return DID;
    case "newBranch":
      doing.newBranch(does.repo);
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
    case "openPreferences":
      doing.openPreferences();
      return DID;
    case "curate":
      return doing.curate(does.subject, does.action);
    case "quit":
      doing.quit();
      return DID;
    case "nothing":
      return DID;
  }
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
): Offer[] {
  return needsYou.flatMap((session) => {
    const name = nameOf(session);
    const reported = reportsTo(session);
    const refused = refusedIn(session);
    const title =
      reported.length > 0
        ? `Show ${name}: ${reported.join(", ")} reported back`
        : refused.length > 0
          ? `Show ${name}: ${refused[refused.length - 1]}`
          : `Show ${name}, which needs you`;
    return [
      tabHolding(tabs, session) === undefined
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

/** The catalogue's id for a stopped smart close's Dismiss row (SI-8f). */
export function dismissId(session: number): string {
  return `needs.dismiss:${session}`;
}

/** The catalogue's id for a wrapping-up tab's Cancel smart close row (ADR 0064). */
/**
 * One memory's three rows (SI-9b, ADR 0065 Q12): Open, Edit and Delete, named for its store and
 * slug (`memory.<verb>:<key>`). The catalogue carries them for every open memory tab; a list of
 * memories adds them for its own rows, whose titles it has — a persona's tab, and SI-9c's
 * workspace and shared lists.
 */
export function memoryOffers(ref: MemoryRef, title: string): Offer[] {
  const key = memoryKey(ref);
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
  ];
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
export function listedMemoryOffers(blocks: readonly PanelBlock[]): Catalogued {
  return catalogued(
    blocks.flatMap((block) =>
      block.kind !== "list"
        ? []
        : block.rows.flatMap((row) => {
            const ref = memoryRefOf(memoryKeyRun(row.runs) ?? "");
            return ref === undefined ? [] : memoryOffers(ref, row.text);
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
    return { why: "charter found no plane, so it cannot reach a branch." };
  if (now.worktree === undefined)
    return { why: "The chat in front is not working in a branch charter cut." };
  return { cut: now.worktree };
}

/** The tab holding a session, or nothing when no tab does. */
function tabHolding(tabs: Tabs, session: number): number | undefined {
  return tabs.order.find((id) => panesOf(tabs, id).some((pane) => pane.session === session));
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
  /** One chat, by the tab that holds it. Its rows are the same rows the tab strip draws. */
  | { on: "chat"; tab: number }
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
  /** The panes — the centre of the window, where a chat is. Not about any one pane: a split
   *  acts on the pane that has the keyboard, which is what the bar's buttons act on too. */
  | { on: "pane" };

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
    case "chat":
      return {
        above: [
          smartCloseCancelId(what.tab),
          `tab.select:${what.tab}`,
          `tab.rename:${what.tab}`,
          `tab.pin:${what.tab}`,
          `tab.worklink:${what.tab}`,
          `tab.workunlink:${what.tab}`,
        ],
        below: [`tab.close:${what.tab}`],
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
        above: [`worktree.files:${at}`, `worktree.merge:${at}`],
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
        above: [`persona.show:${what.persona}`, `persona.edit:${what.persona}`, "persona.create"],
        below: [`persona.remove:${what.persona}`],
      };
    case "vault":
      return {
        above: [`vault.open:${what.vault}`, "vault.create"],
        below: [`vault.remove:${what.vault}`],
      };
    case "todo":
      return { above: [`todo.done:${what.slug}`], below: [`todo.forget:${what.slug}`] };
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
        above: [`clone.chat:${what.repo}`, `clone.branch:${what.repo}`, `clone.pick:${what.repo}`],
        below: [],
      };
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
  const found = (ids: readonly string[]) =>
    ids.map((id) => offers.get(id)).filter((row) => row !== undefined);
  const { above, below } = menuOn(what);
  return { above: found(above), below: found(below) };
}
