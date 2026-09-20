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
import type { ChatWorktree } from "./bindings";
import { panesOf, type Direction, type Tabs } from "./tabs";

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

/** What a row does, as a value the window can carry out. */
export type Does =
  | { verb: "chat.new" }
  | { verb: "split"; direction: Direction }
  /** Hands a key the palette claimed to the chat in front, rather than swallowing it. */
  | { verb: "sendKey"; key: string }
  | { verb: "closePane" }
  | { verb: "closeTab"; tab: number }
  | { verb: "selectTab"; tab: number }
  | { verb: "focusWorkspace"; workspace: string }
  | { verb: "showChat"; session: number }
  | { verb: "removeWorktree"; force: boolean }
  | { verb: "mergeWorktree" }
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
  /** The plane's root. Every worktree command needs it, and there may not be one. */
  plane?: string;
  /** A refusal `worktree.remove` gave and the operator has not answered yet. */
  refusal?: string;
  /** The chats asking for you, oldest first. */
  needsYou: readonly number[];
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
  focusWorkspace: (workspace: string) => void;
  showChat: (session: number) => void;
  removeWorktree: (force: boolean) => Promise<Ran>;
  mergeWorktree: () => Promise<Ran>;
  sendKey: (key: string) => Promise<Ran>;
  quit: () => void;
};

/** Nothing happened worth saying, which is the ordinary answer. */
const DID: Ran = { ok: true };

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
  const offers: Offer[] = [];

  // A chat starts through the picker, which is the only path ADR 0022 admits. The palette
  // opens that question; it never answers it.
  offers.push(can("chat.new", "New tab", { verb: "chat.new" }));

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
    front
      ? can(PASS_THROUGH_ID, sendKey, { verb: "sendKey", key: PASS_THROUGH_KEY })
      : cannot(PASS_THROUGH_ID, sendKey, "No chat is in front, so there is nowhere to send it."),
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
  }

  for (const tab of now.tabs.order) {
    const name = now.tabs.byId[tab].name;
    const title = `Switch to tab ${name}`;
    offers.push(
      tab === now.tabs.inFront
        ? cannot(`tab.select:${tab}`, title, "It is already in front.", name)
        : can(`tab.select:${tab}`, title, { verb: "selectTab", tab }, name),
    );
  }

  for (const workspace of now.workspaces) {
    const title = `Focus workspace ${workspace}`;
    offers.push(
      workspace === now.focused
        ? cannot(`workspace.focus:${workspace}`, title, "It is already focused.", workspace)
        : can(
            `workspace.focus:${workspace}`,
            title,
            { verb: "focusWorkspace", workspace },
            workspace,
          ),
    );
  }

  // The worktree of the chat in front. Merging is not destructive — it is fast-forward only
  // and never pushes — so it sits above the line; removing is below it.
  const why = whyNoWorktree(now, front !== undefined);
  const merge = "Merge this chat's worktree into its clone";
  offers.push(
    why === undefined
      ? can("worktree.merge", merge, { verb: "mergeWorktree" })
      : cannot("worktree.merge", merge, why),
  );

  // ----- destructive, and therefore last -----

  offers.push(
    front
      ? can("pane.close", "Close pane", { verb: "closePane" })
      : cannot("pane.close", "Close pane", "No chat is in front, so there is no pane to close."),
  );

  for (const tab of now.tabs.order) {
    const name = now.tabs.byId[tab].name;
    offers.push(can(`tab.close:${tab}`, `Close tab ${name}`, { verb: "closeTab", tab }, name));
  }

  const remove = "Remove this chat's worktree";
  offers.push(
    why === undefined
      ? can("worktree.remove", remove, { verb: "removeWorktree", force: false })
      : cannot("worktree.remove", remove, why),
  );

  // **The one row that is absent rather than refused.** Every other unavailable action is
  // listed with its reason, because an operator cannot ask about an option they cannot see.
  // This one is different in kind: it discards work the core has just refused to discard, and
  // it is the operator's answer to a sentence they have read. A row permanently offering to
  // force is a destructive action nobody was warned about; there is nothing to warn about
  // until the refusal exists, and then the row appears beside it.
  if (now.refusal !== undefined && why === undefined) {
    offers.push(
      can("worktree.discard", "Discard that work and remove the worktree anyway", {
        verb: "removeWorktree",
        force: true,
      }),
    );
  }

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
    case "focusWorkspace":
      doing.focusWorkspace(does.workspace);
      return DID;
    case "showChat":
      doing.showChat(does.session);
      return DID;
    case "removeWorktree":
      return doing.removeWorktree(does.force);
    case "mergeWorktree":
      return doing.mergeWorktree();
    case "sendKey":
      return doing.sendKey(does.key);
    case "quit":
      doing.quit();
      return DID;
    case "nothing":
      return DID;
  }
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

/** Why there is no worktree to act on, or nothing when there is one. */
function whyNoWorktree(now: Now, inFront: boolean): string | undefined {
  if (!inFront) return "No chat is in front.";
  if (now.plane === undefined) return "charter found no plane, so it cannot reach a worktree.";
  if (now.worktree === undefined)
    return "The chat in front is not working in a worktree charter cut.";
  return undefined;
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
 * The rows left after what has been typed, in the order they are shown.
 *
 * **The name you typed in FULL is the row Enter runs**, and after that **a row your words
 * found comes before a row that merely has those letters in somebody's name.** Within each
 * of those two groups everything keeps the place the catalogue gave it — still no score and
 * still no cap. That matters at the scale ADR 0026 writes the limits for: with fifty chats
 * open the catalogue is 119 rows, and `re` used to list `Switch to tab release.3` and forty
 * other names above `Remove this chat's worktree` (measured, charter-app#48). Two stable
 * groups is not a score — nothing is weighted, nothing moves relative to anything else
 * inside its group, and the same query always gives the same order.
 *
 * **It is ranking rather than filtering, and the measurement is why.** The tmux frame's
 * answer was to keep workspaces out of the browsable list; at fifty chats the rows burying
 * the verb are the TABS, which the frame kept, so dropping the workspaces would not have
 * moved the number it was meant to fix.
 */
export function narrow(query: string, offers: readonly Offer[]): Offer[] {
  const want = query.trim();
  if (want === "") return [...offers];
  const kept = offers.filter((offer) => matches(want, offer));
  const whole = want.toLowerCase();
  const exact = kept.filter((offer) => offer.title.toLowerCase() === whole);
  const rest = kept.filter((offer) => !exact.includes(offer));
  return [
    ...exact,
    ...rest.filter((offer) => byItsWords(want, offer)),
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
