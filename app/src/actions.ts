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
import type { ChatWorktree, ExtensionView } from "./bindings";
import {
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
 * The strip a chat working outside every workspace appears on.
 *
 * The sidebar has always shown those chats rather than dropping them, and a strip that shows
 * one workspace's chats has to have somewhere to put them or they become unreachable — which
 * is the defect being fixed, not one to introduce (charter-app#130).
 *
 * Slashes, because this stands where a workspace name stands and a workspace name is a
 * directory name: no directory can contain one, so it can never collide with a real
 * workspace. It never reaches the operator — `catalogue` gives its row its own words.
 */
export const OUTSIDE = "outside/every/workspace";

/** What the strip and the palette call that one. */
export const OUTSIDE_TITLE = "Outside every workspace";

/** What a row does, as a value the window can carry out. */
export type Does =
  | { verb: "chat.new" }
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
  /** Opens a view in a tab of its own, or brings forward the tab already showing it.
   *
   *  **One verb for charter's views and an extension's** — the persona view is
   *  `{ from: null, view: "persona", key }` and persona statistics is the extension's id and
   *  its view's. It reads and changes nothing by itself: what an extension's view shows is
   *  asked of its program when the tab draws it, through the core's gate. A persona's row is
   *  here rather than only a click on the panel because a menu is a third reader of this list
   *  (`Menus.tsx`) and the persona rows had nothing in it to read (charter-app#174). */
  | { verb: "openView"; view: ViewRef; title: string }
  /** Asks which of the plane's vaults to open (charter-app#235). It opens nothing by itself:
   *  what the picker's row runs is that vault's own `vault.open:<name>`. */
  | { verb: "pickVault" }
  /** Asks for a new vault's name and provider. It makes nothing by itself: what may be called
   *  what is `charter vault add`'s to say, through `vault_create`. */
  | { verb: "createVault" }
  /** **Names the worktree it acts on**, and never "whichever one is in front".
   *
   *  It used to carry only `force`, which made `worktree.remove` a row about the chat in
   *  front and left the explorer's own rows with nothing to offer (charter-app#174). The
   *  front chat's row still exists and still says "this chat's" — it now simply spells out
   *  the piece it means, the same way `tab.close:<id>` spells out its tab. */
  | { verb: "removeWorktree"; cut: Cut; force: boolean }
  | { verb: "mergeWorktree"; cut: Cut }
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
  /** Lets go of one project, which ends its chats and takes its tab out. Nothing of the
   *  project on disk goes. */
  | { verb: "closeProject"; plane: string }
  /** Opens that project's Project settings tab (charter-app#252) — bringing the project to the
   *  front first when it is not. It writes nothing by itself: a save is the tab's, through the
   *  core's own checks. */
  | { verb: "openSettings"; plane: string }
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
   * tab release.3` and `Remove this chat's worktree` both contain `re`. `narrow` uses it to
   * put a row the operator's words FOUND ahead of a row that merely has those letters in
   * somebody's name — which at fifty chats is the whole difference (charter-app#48).
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
};

/** The window as it now stands: everything an offer's availability is decided from. */
export type Now = {
  tabs: Tabs;
  /** The plane's workspaces, in the order the sidebar lists them. */
  workspaces: readonly string[];
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
   * The views approved extensions offer this window (`extension_views`), one row each. The
   * palette is how a keyboard reaches them; the personas panel's heading is how a pointer does.
   */
  views?: readonly ExtensionView[];
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
};

/** What the window does when a row is run. One function per verb, whichever surface asked. */
export type Doing = {
  newChat: () => void;
  split: (direction: Direction) => void;
  closePane: () => void;
  closeTab: (tab: number) => void;
  selectTab: (tab: number) => void;
  /** Brings the tab forward with its name open for editing. Nothing is renamed until the
   *  operator says the name, so it answers no `Ran`. */
  renameTab: (tab: number) => void;
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
  /** Opens a view's tab, or brings forward the one showing it. It reads and changes nothing
   *  by itself, so it answers no `Ran`. */
  openView: (view: ViewRef, title: string) => void;
  /** Opens the vault picker. Nothing is opened until a vault in it is. */
  pickVault: () => void;
  /** Opens the new-vault dialog. Nothing is made until it is answered. */
  createVault: () => void;
  /** Each takes the piece it acts on. The window no longer decides which worktree a removal
   *  meant by looking at what happens to be in front (charter-app#174). */
  removeWorktree: (cut: Cut, force: boolean) => Promise<Ran>;
  mergeWorktree: (cut: Cut) => Promise<Ran>;
  sendKey: (key: string) => Promise<Ran>;
  openProject: () => void;
  /** Opens the new-project dialog. Nothing is scaffolded and nothing is opened until it is
   *  answered, and the open it ends in is the gated one. */
  createProject: () => void;
  showExtensions: () => void;
  installCli: () => Promise<Ran>;
  selectProject: (plane: string) => void;
  closeProject: (plane: string) => Promise<Ran>;
  /** Brings that project to the front and opens its Project settings tab. */
  openSettings: (plane: string) => void;
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
export type Cut = { workspace: string; repo: string; piece: string };

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
): {
  open: Offer;
  create: Offer;
  switchTo: Offer[];
  pin: Offer[];
  settings: Offer[];
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
  "Deletes its clones, worktrees, memory and todos. There is no undo.";

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
export const KEEPS_THE_BRANCH = "Takes the tree, not the branch. The branch stays where it is.";

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
  for (const session of now.needsYou) {
    const name = now.nameOf(session);
    const title = `Show ${name}, which needs you`;
    offers.push(
      tabHolding(now.tabs, session) === undefined
        ? cannot(`needs.show:${session}`, title, "That chat has no tab in this window.", name)
        : can(`needs.show:${session}`, title, { verb: "showChat", session }, name),
    );
    // **Ignore, until the chat asks again** (charter-app#248): the item's `✕`, Delete on it,
    // and this row in the palette are one row. Always available — ignoring is about the
    // request, and a chat asking from a tab this window does not hold is still asking.
    offers.push(
      can(
        ignoreId(session),
        `Ignore ${name} until it asks again`,
        { verb: "ignoreNeedsYou", session },
        name,
      ),
    );
  }

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

  // The workspaces of this project, which is the axis the tmux frame had and the port lost
  // (ADR 0036). These rows are the workspace strip as well as palette rows — one place the
  // words and the availability are written down, the same rule the tab strip follows.
  for (const workspace of now.workspaces) {
    const [title, name] =
      workspace === OUTSIDE
        ? [`Focus the chats ${OUTSIDE_TITLE.toLowerCase()}`, undefined]
        : [`Focus workspace ${workspace}`, workspace];
    offers.push(
      workspace === now.focused
        ? cannot(`workspace.focus:${workspace}`, title, "It is already focused.", name)
        : can(`workspace.focus:${workspace}`, title, { verb: "focusWorkspace", workspace }, name),
    );
    // Not for the strip of chats outside every workspace: it is not a workspace on the plane
    // and there is nothing on disk for a pin to name.
    if (workspace === OUTSIDE) continue;
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
  const projects = projectRows(now.projects ?? [], now.plane, pinned.projects);
  offers.push(
    projects.open,
    projects.create,
    ...projects.switchTo,
    ...projects.pin,
    ...projects.settings,
  );

  // **The plane's personas, one row each** (charter-app#174). What the row opens is the
  // persona's view — its own tab — and this is the whole of what charter can do to a persona today:
  // a definition is a file an operator edits, and nothing in this window writes one. A row
  // that only READS is still a row — it is how the persona rows get a menu without a second
  // list being invented for them, and it is how a persona is reachable from the palette.
  //
  // **It opens the persona's own tab** — the operator's ruling of 2026-09-23, *"Its own tab"* —
  // which is charter's first built-in view, and the same verb an extension's view is opened by.
  for (const persona of now.personas ?? []) {
    offers.push(
      can(
        `persona.show:${persona}`,
        `Show what ${persona} is`,
        { verb: "openView", view: { from: null, view: "persona", key: persona }, title: persona },
        persona,
      ),
    );
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

  // The worktree of the chat in front. Merging is not destructive — it is fast-forward only
  // and never pushes — so it sits above the line; removing is below it.
  const inFront = frontWorktree(now, chatInFocus);
  const merge = "Merge this chat's worktree into its clone";
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
    now.plane === undefined ? "charter found no plane, so it cannot reach a worktree." : undefined;
  for (const cut of pieces) {
    const title = `Merge worktree ${cut.piece} into ${cut.repo}`;
    offers.push(
      noPlane === undefined
        ? can(`worktree.merge:${idOf(cut)}`, title, { verb: "mergeWorktree", cut }, cut.piece)
        : cannot(`worktree.merge:${idOf(cut)}`, title, noPlane, cut.piece),
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

  const remove = "Remove this chat's worktree";
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
    const title = `Remove worktree ${cut.piece} in ${cut.repo}`;
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
      can("worktree.discard", "Discard that work and remove the worktree anyway", {
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
    case "openView":
      doing.openView(does.view, does.title);
      return DID;
    case "pickVault":
      doing.pickVault();
      return DID;
    case "createVault":
      doing.createVault();
      return DID;
    case "removeWorktree":
      return doing.removeWorktree(does.cut, does.force);
    case "mergeWorktree":
      return doing.mergeWorktree(does.cut);
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
    case "closeProject":
      return doing.closeProject(does.plane);
    case "openSettings":
      doing.openSettings(does.plane);
      return DID;
    case "quit":
      doing.quit();
      return DID;
    case "nothing":
      return DID;
  }
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
    return { why: "charter found no plane, so it cannot reach a worktree." };
  if (now.worktree === undefined)
    return { why: "The chat in front is not working in a worktree charter cut." };
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
 * tab release.3` answers no to `re` and yes to `switch`; `Remove this chat's worktree` has no
 * name in it and answers yes to both. A row with no name is always its own words.
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
  | { on: "project"; plane: string }
  /** One worktree of the focused workspace, as the explorer's rows name it — the clone and
   *  the piece. Not a `Cut`: the workspace is the focused one on every surface that draws
   *  these, so carrying it would be a third copy of an answer the window already has. */
  | { on: "worktree"; repo: string; piece: string }
  | { on: "persona"; persona: string }
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
 * how `Outside every workspace` gets a menu with no pin and no delete in it without anything
 * here knowing that strip exists, and how a row that is deleted from the catalogue takes its
 * menu entry with it.
 */
export function menuOn(what: MenuOn): { above: string[]; below: string[] } {
  switch (what.on) {
    case "chat":
      return {
        above: [`tab.select:${what.tab}`, `tab.rename:${what.tab}`, `tab.pin:${what.tab}`],
        below: [`tab.close:${what.tab}`],
      };
    case "workspace":
      return {
        above: [
          `workspace.focus:${what.workspace}`,
          `workspace.pin:${what.workspace}`,
          "workspace.create",
        ],
        below: [`workspace.remove:${what.workspace}`],
      };
    case "project":
      return {
        above: [
          `project.select:${what.plane}`,
          `project.pin:${what.plane}`,
          `project.settings:${what.plane}`,
          "project.create",
          "project.open",
        ],
        below: [`project.close:${what.plane}`],
      };
    case "worktree": {
      const at = `${what.repo}/${what.piece}`;
      return {
        above: [`worktree.merge:${at}`],
        // The discard row is listed and is almost never found: it exists only while a removal
        // of THIS piece has been refused and not answered. That is the whole reason a menu
        // lists ids rather than rows — nothing here has to know when it exists.
        below: [`worktree.remove:${at}`, `worktree.discard:${at}`],
      };
    }
    case "persona":
      // One row, because reading the definition is the only thing charter can do to a persona
      // from this window: a persona is a file on disk that `charter persona create` writes and
      // an operator edits. A menu with one honest row is the answer; a menu with three
      // invented ones is the defect this module exists to prevent (charter-app#174).
      return { above: [`persona.show:${what.persona}`], below: [] };
    case "pane":
      return {
        above: ["chat.new", "pane.split.right", "pane.split.down", PASS_THROUGH_ID],
        below: ["pane.close"],
      };
  }
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
