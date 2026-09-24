import { useEffect, useState, type CSSProperties } from "react";
import { commands, type TitleBarRoom } from "./bindings";
import { AboutCharter } from "./About";
import { type Ending } from "./QuitWarning";
import { UpdateItem, type Updates } from "./Updates";
import { NeedsYouMenu, type Needing, type Quiet } from "./NeedsYou";
import type { Offer } from "./actions";

/**
 * **The window's title bar**: where you are on the left, what charter is on the right.
 *
 * The operator asked for it because the bar was *"very very empty"*, and said exactly what
 * goes in it:
 *
 * > in left side lest just write
 * > `{selected project name} / {selected workspace name} / {N sessions running}`
 * >
 * > in right side lets show
 * > `{About Charter …}` `{Update Indicator …}`
 *
 * # It is a title bar on macOS and a top row everywhere else, and that is not a gate
 *
 * `tauri.conf.json` asks for `titleBarStyle: "Overlay"` with `hiddenTitle: true`. That is a
 * **macOS-only key**: there the system stops drawing a title bar of its own, leaves the
 * traffic lights floating over the webview, and this element IS the title bar — which is why
 * it reserves room for them on the leading edge (`title_bar_room`, and the constant lives in
 * Rust beside the `cfg!` that decides it). Windows and WebKitGTK ignore the key and go on
 * drawing their own title bar above the webview, so there this is the window's first row
 * instead, under the system's bar, with nothing reserved.
 *
 * **Nothing is gated, because the content is right either way.** Where you are and what
 * charter is are worth a row on every platform; the only thing that differs is 78 px of
 * leading padding and which bar the operator thinks of it as. A `#[cfg]` that drew the
 * breadcrumb on one platform and not another would be two windows to keep in step, and the
 * one it left out is the one charter is most often built for in CI.
 *
 * # It drags, and the parts that must not drag do not
 *
 * `data-tauri-drag-region="deep"` rather than the bare attribute. Tauri's handler
 * (`tauri/src/window/scripts/drag.js`) walks the composed path up from what was pressed: the
 * bare attribute drags only on a **direct** press of the element carrying it, which on a bar
 * made of text spans means the bar drags everywhere except on its own words. `deep` drags
 * anywhere in the subtree — and the same walk returns false at the first *clickable* element
 * it meets, where clickable is a `<button>`, a link, an `<input>`, an interactive `role`, or
 * anything carrying a `tabindex` other than `-1`.
 *
 * **Every control here is a `<button>`, and the TAG is what carries it.** `BUTTON` is in
 * Tauri's `CLICKABLE_TAGS`, so no attribute of ours is load-bearing for this: the `tabIndex={0}`
 * on About, the update item and the needs-you button (charter-app#249) is there for WebKit's
 * tab sequence (charter-app#186, charter-app#189), not for the drag. A scenario written
 * asserting a `tabindex` on every control was refuted by the real app, which is how that came
 * to be written down here rather than assumed.
 *
 * **What no test here proves is that the window then moves.** WebDriver dispatches a
 * synthetic event and performs no default action (`docs/ui-primitives.md`), and a synthetic
 * `mousedown` that did reach Tauri's listener would end in an IPC call rather than an
 * observable drag. What IS proved, in jsdom and again in the shipped app, is the two facts
 * that handler reads: the attribute is on the bar, and every control inside it is an element
 * the handler stops at. `core:window:allow-start-dragging` — which `core:default` does NOT
 * include — is named in `capabilities/default.json` for the same reason: without it the IPC
 * call is refused and the bar is one the operator cannot grab.
 */
export function TitleBar({
  crumbs,
  updates,
  room,
  chats,
  needing,
}: {
  /** Where the window is, for the left-hand side. */
  crumbs: Crumbs;
  /**
   * The updater, as the WINDOW knows it (`Updates.tsx`).
   *
   * **It moved here from the status line rather than being copied**, and the reason is in the
   * shape: an update offer is a fact about the app, the status line is a fact about a project,
   * and the window may hold eight projects. `useUpdates` was called once per `PlaneView`, so
   * eight projects meant eight updater clients listening for one app-wide event and eight
   * `update_channel` calls at open. It is called once, up in `App`, and drawn once, here.
   *
   * The plane's **pin** did not move and must not: `charter version`'s verdict is about one
   * plane's `[charter] version`, so it belongs beside the project it is about.
   */
  updates?: Updates;
  /**
   * What the operating system has already spent of this bar, or `undefined` until the core
   * has said. Absent reserves nothing, which is the right answer for every platform but one
   * and is corrected within a frame of the first paint on that one.
   */
  room?: TitleBarRoom;
  /**
   * Every chat the window holds, with its state — the list the quit warning is given. Restart
   * to update ends them all, so it names the ones that are mid-turn before it does
   * (charter-app#251).
   */
  chats?: readonly Ending[];
  /**
   * Every project's chats asking for the operator (charter-app#249) — the queue's one place.
   * Absent draws no button, which is also what an empty list draws.
   */
  needing?: {
    items: readonly Needing[];
    quiet: readonly Quiet[];
    onPress: (plane: string, offer: Offer) => void;
  };
}) {
  return (
    <header
      className="title-bar"
      data-testid="title-bar"
      data-tauri-drag-region="deep"
      data-overlaid={room?.overlaid ? "yes" : "no"}
      style={{ "--window-controls": `${room?.reserved ?? 0}px` } as CSSProperties}
    >
      <Breadcrumb crumbs={crumbs} />
      {/* The right-hand end, and the one part of the bar that never gives way. Both controls
          are about the app rather than the project, which is why they are up here and not on
          the status line: this bar is the window's. */}
      <span className="title-bar-doing">
        {/* First, because it is the one of the three that is about the operator's chats and
            not about the app — and it is nothing at all when nothing needs you. */}
        {needing && <NeedsYouMenu {...needing} />}
        <AboutCharter />
        {updates && <UpdateItem updates={updates} chats={chats} />}
      </span>
    </header>
  );
}

/** Where the window is, in the three parts the operator asked for. */
export type Crumbs = {
  /** The project in front, named as its tab names it. `undefined` when the window holds none. */
  project?: string;
  /**
   * Whether the window has finished deciding whether it holds a project at all.
   *
   * **The opener's own pair of conditions** (`App.tsx`'s `openerUp`): the core has said what
   * the launch resolved, and the cold-launch restore has finished opening what it remembered.
   * Until both, no project and eight projects look identical from here, and "No project open"
   * on the bar half a second before eight arrive is the same lie the opener is careful not to
   * tell in a larger font.
   */
  decided: boolean;
  /**
   * Whether that project's plane has been read at all.
   *
   * **Separate from {@link workspace}, because "not yet" and "nowhere" are different claims**
   * — the same split `StatusLine` makes, and for the same reason: a plane holding no
   * workspaces answers perfectly well and leaves the window on none of them.
   */
  read: boolean;
  /** The workspace the window is on, already read as it should be said (so `OUTSIDE` has
   *  become its title). `undefined` when it is on none. */
  workspace?: string;
  /** Whether that workspace has a colour (charter-app#281): its clause then carries a mark in the
   *  window's accent, which is tinted with that colour while it is in front. */
  coloured?: boolean;
  /**
   * How many of that project's chats are **running**, or `undefined` before the core has said
   * what it had open.
   *
   * **Running, not open**, which is the distinction charter draws and this operator lives by:
   * he keeps many chats at once and most of them are sitting still. A chat is running when a
   * hook has said so (`chatState.ts`) — a chat that is waiting for him, one that has finished,
   * one that failed and one that no hook has ever reported are each not running, and the
   * needs-you button at the other end of this bar, and the project tab, are where the waiting
   * ones are counted. A number here that meant "open" would say 50 all day.
   */
  running?: number;
};

/**
 * The left-hand side: `project / workspace / N chats running`.
 *
 * # Every degraded reading is a sentence, never a gap
 *
 * A breadcrumb of slashes with nothing between them is the failure this is written against.
 * So: no project at all replaces the whole row with one sentence rather than drawing two
 * empty segments; a plane that has not answered says so in the same words the status line
 * uses (*reading the plane…*), and one that answered with no workspace says *no workspace*,
 * which is an answer and not an absence.
 *
 * **Zero IS drawn here, and that is deliberately not the footer's rule.** `StatusLine` drops
 * a count at zero — presence is the signal, and a `todo 0` on the line every day is furniture
 * by Friday. This is not a cell in a row of counts: it is the third clause of one sentence the
 * operator asked for, and a sentence that ends at the second slash on a quiet morning reads as
 * charter having failed to count rather than as charter having counted none.
 *
 * # What gives way when the window is narrow
 *
 * The two names truncate, in that order, and the count never does — a project called
 * `charter-app` cut to `charter-a…` is still legible, and `3 chats running` cut to `3 cha…`
 * is a number with no unit. The controls on the right are `flex: none`, so a long name pushes
 * nothing off the window; it shortens itself instead. The whole reading is in `title`, so
 * anything the row had to cut is a hover away.
 */
export function Breadcrumb({ crumbs }: { crumbs: Crumbs }) {
  const { project, decided, read, workspace, coloured, running } = crumbs;
  if (project === undefined)
    return (
      <span className="crumbs" data-testid="title-crumbs">
        {/* One sentence, not an empty skeleton. The window holding no project is a state it
            draws the opener for, and the opener is the thing to read — this row says which
            state it is in and gets out of the way. */}
        {decided ? (
          <span className="none">No project open</span>
        ) : (
          <span className="pending">charter is opening…</span>
        )}
      </span>
    );

  const workspaceSaid = read ? (workspace ?? "no workspace") : "reading the plane…";
  const chatsSaid =
    running === undefined
      ? "counting chats…"
      : running === 0
        ? "no chats running"
        : `${running} ${running === 1 ? "chat" : "chats"} running`;
  return (
    <span
      className="crumbs"
      data-testid="title-crumbs"
      title={`${project} / ${workspaceSaid} / ${chatsSaid}`}
    >
      <span className="crumb crumb-project">{project}</span>
      <Slash />
      {coloured === true && workspace !== undefined && (
        <span className="crumb-mark" aria-hidden="true" data-testid="crumb-mark" />
      )}
      <span
        className={`crumb crumb-workspace${workspace === undefined ? (read ? " none" : " pending") : ""}`}
      >
        {workspaceSaid}
      </span>
      <Slash />
      {/* `flex: none` in the stylesheet: the names give way, the count does not. */}
      <span
        className={`crumb crumb-chats${running === undefined ? " pending" : running === 0 ? " none" : ""}`}
        data-running={running ?? "unknown"}
      >
        {chatsSaid}
      </span>
    </span>
  );
}

/** The separator the operator wrote, hidden from a screen reader — it reads the three clauses,
 *  and a slash between each of them is punctuation spoken aloud. */
function Slash() {
  return (
    <span className="crumb-sep" aria-hidden="true">
      /
    </span>
  );
}

/**
 * How many of a project's chats are running, out of the report it already sends the window.
 *
 * **The existing answer, asked of nothing.** Every project reports `ending` — its open chats,
 * each with what it is doing — because the quit warning has to list them; the state on each
 * one is `chatState.ts`'s, which is kept current by hooks rather than by polling. Counting
 * here is a filter over a list the window is already holding, so the bar costs no command of
 * its own. A second `chat_states` call would be fifty questions to learn what the window was
 * told.
 *
 * `undefined` until the project has settled, which is the core answering what it already had
 * open. Before that an empty `ending` means *not yet*, and reading it as zero would put
 * "no chats running" on the bar for the first moments of every launch — over a plane that is
 * about to put twenty chats back.
 */
export function runningIn(report?: { ending: { state: string }[]; settled: boolean }) {
  if (report === undefined || !report.settled) return undefined;
  return report.ending.filter((chat) => chat.state === "running").length;
}

/**
 * What the system has already spent of the title bar, asked once.
 *
 * **After the first paint, never before it.** `main.tsx` is written so that nothing stands
 * between the process starting and the first frame (ADR 0026's 2 s cold start), so this is an
 * effect and the bar reserves nothing until it answers. On macOS the breadcrumb shifts right
 * by the width of the traffic lights a frame later; everywhere else the answer is zero and
 * nothing moves at all. A window that cannot ask draws the bar with nothing reserved, which
 * is wrong on macOS by 78 px and is not a reason to draw no bar.
 */
export function useTitleBarRoom(): TitleBarRoom | undefined {
  const [room, setRoom] = useState<TitleBarRoom>();
  useEffect(() => {
    let gone = false;
    void commands
      .titleBarRoom()
      .then((said) => {
        if (!gone && typeof said?.reserved === "number") setRoom(said);
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
  }, []);
  return room;
}
