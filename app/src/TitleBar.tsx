import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { commands, type PlaneSaving, type RepoSaving, type TitleBarRoom } from "./bindings";
import { SaveIndicator } from "./SavingView";
import { AboutCharter } from "./About";
import { type Ending } from "./QuitWarning";
import { UpdateItem, type Updates } from "./Updates";
import { NeedsYouMenu, type Needing, type Quiet } from "./NeedsYou";
import type { Offer } from "./actions";

/**
 * **The window's title bar**: the project strip on the left, what charter is on the right.
 *
 * ```
 * ● ● ●  [charter ²] [volaticloud] [▾ 3 more ¹] [+]  ···drag···  ✋2  About  ⟳
 * ```
 *
 * **ADR 0054 supersedes the operator's first spec for this bar**, which was a breadcrumb —
 * *"`{selected project name} / {selected workspace name} / {N sessions running}`"* on the
 * left and About and the update indicator on the right. The right-hand end is still his. The
 * left is the project strip now, because the bar is the window's row and the project strip is
 * the window's strip: `App` holds it for the window, not for a `PlaneView`. The crumb went
 * rather than squeezing in beside the tabs — the project tab in front says which project, the
 * workspace strip says which workspace, and a crumb repeating both spent the width the tabs
 * need. `N chats running` moved to the status line, which is per-project (`StatusLine.tsx`).
 *
 * # What gives way when the window is narrow
 *
 * **The right-hand end never does, and the project tabs give way before it**: the strip takes
 * what is left and collapses what it has no room for into its show-more menu, by the rules it
 * has always had (`fits.ts`, ADR 0039). Width the tabs cannot use is drag region, and **a
 * minimum stretch of it is always reserved** (`--title-bar-drag` in `App.css`), so a window
 * full of projects can still be grabbed. It is the strip's margin, outside the box the strip
 * measures, so the tabs are fitted to what is left after it and nothing here has to know it.
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
 * **Nothing is gated, because the content is right either way.** The projects and what
 * charter is are worth a row on every platform; the only thing that differs is 78 px of
 * leading padding and which bar the operator thinks of it as. A `#[cfg]` that drew the
 * strip here on one platform and in a row of its own on another would be two windows to keep
 * in step, and the one it left out is the one charter is most often built for in CI.
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
 * tab sequence (charter-app#186, charter-app#189), not for the drag. **The project tabs need no
 * drag rule of their own for the same reason**: each is a `<button>`, and so are its `×`, the
 * `+` and the show-more button. The strip itself carries a `tabindex` for its one Tab stop
 * (`roving.ts`), which the walk also stops at, so a press in a gap between its controls does
 * not drag either; the stretch after the strip does. A scenario written asserting a
 * `tabindex` on every control was refuted by the real app, which is how that came to be
 * written down here rather than assumed.
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
  projects,
  updates,
  room,
  chats,
  needing,
  save,
}: {
  /**
   * The project strip (ADR 0054), drawn after the room the window controls take. `App` builds
   * it, because the strip's rows, its pins and its show-more are the window's. Absent — a
   * window holding no project — draws none, and the right-hand end stays at the right.
   */
  projects?: ReactNode;
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
  /**
   * The project in front's save standing (charter-app#294, ADR 0051) and what its two buttons
   * do. Absent — no project in front, or none read yet — draws nothing.
   */
  save?: {
    saving: PlaneSaving;
    /** The active workspace's repos, counted into the indicator (charter-app#299). */
    repos?: readonly RepoSaving[];
    busy: boolean;
    onOpen: () => void;
    onSave: () => void;
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
      {projects}
      {/* The right-hand end, and the part of the bar that never gives way: the strip before it
          gives way first. Both controls
          are about the app rather than the project, which is why they are up here and not on
          the status line: this bar is the window's. */}
      <span className="title-bar-doing">
        {/* First, because it is the one of the three that is about the operator's chats and
            not about the app — and it is nothing at all when nothing needs you. */}
        {needing && <NeedsYouMenu {...needing} />}
        {/* Then the project in front's unsaved work: about the project, not the app, and the
            one thing on the bar the operator acts on as often as a chat that asks. */}
        {save && <SaveIndicator {...save} />}
        <AboutCharter />
        {updates && <UpdateItem updates={updates} chats={chats} />}
      </span>
    </header>
  );
}

/**
 * What the system has already spent of the title bar, asked once.
 *
 * **After the first paint, never before it.** `main.tsx` is written so that nothing stands
 * between the process starting and the first frame (ADR 0026's 2 s cold start), so this is an
 * effect and the bar reserves nothing until it answers. On macOS the project strip shifts right
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
