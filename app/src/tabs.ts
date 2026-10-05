/**
 * The tabs the window shows, and how each one is split into panes.
 *
 * **A tab is a layout of panes, and a pane holds a session or a view** (ADR 0043, as
 * amended 2026-09-23). A session is a chat's terminal; a view is anything else a tab can show —
 * a persona, an extension's statistics — named by data ({@link ViewRef}) rather than by a
 * component, so charter's own views and an extension's take the same path. The operator's
 * words, choosing a tab for the persona card: *"we dont have other tabs then sessions, and this
 * can be good example for us - that in tabs we can have what we want - not only harnesses"*.
 *
 * **The pane is what was generalised, not the tab.** A tab stays one thing — a layout — so a
 * view can be split beside a chat, and every rule a tab already had (the fixed order, pinning,
 * the overflow menu, closing the pane in focus) holds for a tab that shows no chat without a
 * second copy of it. What a tab does NOT have any more is a chat of its own by definition: the
 * chat's name lives on the session a pane shows, so a tab holding only a view has no chat to
 * pretend about ({@link chatOf}).
 *
 * Only the tab in front has panes on screen, which is what keeps fifty sessions cheap: the core
 * holds every session's terminal, and the panes that are visible are the only ones drawing.
 *
 * **A tab belongs to a workspace, and the strip shows one workspace's tabs** (ADR 0036). It is
 * not a field on the tab: which workspace a chat is in is the plane's answer, read off the
 * sidebar, so the functions that need it take a `FiledIn` and ask.
 *
 * **`order` is fixed, and that is now a decision rather than an accident** (ADR 0039). A tab
 * is appended when it opens, taken out when it closes, and nothing else ever touches its
 * place. A tab that moves under the cursor breaks aiming: an operator going back to the chat
 * that was third from the left goes there with their hand, not by reading, and a strip that
 * re-sorted on activity would turn every click into a read. This is what browsers do, and it
 * is the one interaction convention in this window every user already has.
 *
 * The one place activity DOES order anything is the overflow menu — `byLastActivity` below,
 * and nowhere else. The rule and its opposite are the same rule from two sides: the menu is a
 * list you read, the strip is a surface you aim at, and the boundary is whether the thing
 * moves under your hand.
 *
 * Everything here is a plain value, so the window's whole arrangement is one state to test.
 */

/** Which way a split divides its two children: `row` side by side, `column` one above the other. */
export type Direction = "row" | "column";

/**
 * A view, named by data: **who draws it, which of theirs, and what it is about.**
 *
 * `from` is `null` for a view charter draws itself and an approved extension's id for one it
 * offers, which is `charter_core::panel::By` on the wire; `view` is which of theirs; `key` is
 * what it is about inside that — a persona's name, or `""` for the whole plane. The persona view
 * is `{ from: null, view: "persona", key: "steward" }` and persona statistics is
 * `{ from: "persona-statistics", view: "statistics", key: "" }`: **the same shape, the same
 * command (`open_view`), the same renderer** — nothing about a built-in view is a path an
 * extension's cannot take.
 */
export type ViewRef = { from: string | null; view: string; key: string };

/**
 * **The Saving view** (charter-app#294, ADR 0051): where a plane's unsaved work sits, what the
 * next save takes, the save button and the last saves. One per plane.
 */
export const SAVING_VIEW: ViewRef = { from: null, view: "saving", key: "" };

/** What the Saving tab is called. */
export const SAVING_TITLE = "Saving";

/**
 * **Settings at a workspace's level** (SE-20, #1170; first a page of its own, charter-app#280):
 * the Settings tab keyed by the level and its target — the workspace — so there is one per
 * workspace, deduplicated by the same `viewKey` and filed on that workspace's strip. It keeps the
 * address the old Workspace settings page had (D-SE20a): a launch's record, a pin and a rename
 * (`followRename`, and the core's `wscmd::rename`) already follow it, so a tab put back from
 * before SE-20 opens the Settings tab at that workspace's level.
 */
export function workspaceSettingsView(workspace: string): ViewRef {
  return { from: null, view: "workspace-settings", key: workspace };
}

/**
 * **A workspace's cross-repo changes** (charter#470, ADR 0060): charter's own view, answered in
 * panel blocks by `change::view`. Keyed by the workspace, like its settings, so there is one tab
 * per workspace and it is filed on that workspace's strip.
 */
export function changesView(workspace: string): ViewRef {
  return { from: null, view: "changes", key: workspace };
}

/** What a workspace's changes tab is called. */
export function changesTitle(workspace: string): string {
  return `Changes · ${workspace}`;
}

/**
 * **The agent instructions a workspace's repo carries** (FR-18a, #612): `CLAUDE.md`, `AGENTS.md`
 * and `.cursor/rules`, previewed whole and added to the workspace's memory only on the tab's
 * press. Keyed by the workspace, like its changes, so there is one tab per workspace.
 */
export function repoInstructionsView(workspace: string): ViewRef {
  return { from: null, view: "repo-instructions", key: workspace };
}

/** What a workspace's repo instructions tab is called. */
export function repoInstructionsTitle(workspace: string): string {
  return `Memory from the repo · ${workspace}`;
}

/**
 * **The first task** (FR-28, #621): the guided task FR-1 measures, run twice on the repo the first
 * run opened — each run a chat on a branch of its own, with the task typed and unsent — and both
 * diffs one press away. Keyed by the repo's clone, which is where each run's branch is cut, and
 * filed on its workspace's strip.
 */
export function firstTaskView(clone: string): ViewRef {
  return { from: null, view: "first-task", key: clone };
}

/** What the first task's tab is called. */
export function firstTaskTitle(workspace: string): string {
  return `First task · ${workspace}`;
}

/**
 * **Setting up a harness** (FR-29, W10's "no harness found"): each harness's official installer,
 * run in a shell tab on a press, and a local model server already on the machine. Keyed by the
 * directory the first chat would start in — the repo's clone — so the shell and the chat it
 * leads to start there, and filed on that workspace's strip.
 */
export function harnessSetupView(cwd: string): ViewRef {
  return { from: null, view: "harness-setup", key: cwd };
}

/** What the harness setup tab is called. */
export function harnessSetupTitle(workspace: string): string {
  return `Set up a harness · ${workspace}`;
}

/**
 * **A harness's capability card** (HP-19, W10): what one harness can do here, each thing it lacks
 * said in a line, drawn by the core off the harness's declaration and adapter
 * (`charter_core::harness_card`). Keyed by the harness's name, so there is one tab per harness,
 * opened from a chat's header and called what the card is labelled, *What Codex can do here*.
 */
export function harnessCardView(harness: string): ViewRef {
  return { from: null, view: "harness", key: harness };
}

/** What Settings at a workspace's level is called (the core's `wscmd::rename` says the same). */
export function workspaceSettingsTitle(workspace: string): string {
  return `Workspace settings · ${workspace}`;
}

/**
 * **The Settings tab** (SE-16, #1166; V89b): every setting, one level at a time. **Keyed by the
 * level it shows** — `you`, `project` (SE-17); a workspace's is keyed by the workspace too, and
 * is {@link workspaceSettingsView} (SE-20) — so opening
 * Settings at a level that already has its tab brings that tab forward. The project is the one
 * whose tabs hold it. Its level switcher moves the tab itself to another level
 * ({@link showInstead}, D-SE17a), so the key always says what the tab shows. It is the view
 * `settings` because it is what Settings means from here on. The old Project settings page was
 * the same view with the empty key; SE-19 retired it, and a tab of it put back from an older
 * launch is Settings at the Project level ({@link viewNamedNow}).
 *
 * The You level is the machine's and not the project's, so it is the same tab whichever project's
 * strip it was opened on: what it edits is the machine's layout file, never a file in the project.
 */
export function settingsView(level: Exclude<SettingsLevel, "workspace">): ViewRef {
  return { from: null, view: "settings", key: level };
}

/** The levels a Settings tab can be at (`settings/groups.ts`'s `Level`, as far as it is offered). */
export type SettingsLevel = "you" | "project" | "workspace";

/** The level a view is the Settings tab at, or `undefined` for any other view. */
export function settingsLevelOf(view: ViewRef): SettingsLevel | undefined {
  if (view.from !== null) return undefined;
  if (view.view === "workspace-settings") return "workspace";
  if (view.view !== "settings") return undefined;
  return view.key === "you" || view.key === "project" ? view.key : undefined;
}

/** What the Settings tab is called. */
export const SETTINGS_TAB_TITLE = "Settings";

/**
 * A view tab as a record from an older charter names it, as it is named now, so a launch that
 * put one back puts Settings back instead of a tab nothing draws:
 *
 * - the Preferences tab (charter-app#283) is Settings at the You level since SE-16;
 * - the Project settings page (charter-app#252, the view `settings` keyed `""`) is Settings at
 *   the Project level since SE-19 retired it (D-SE19b).
 *
 * Every other view is itself.
 */
export function viewNamedNow(view: ViewRef, title: string): { view: ViewRef; title: string } {
  if (view.from !== null) return { view, title };
  if (view.view === "preferences") return { view: settingsView("you"), title: SETTINGS_TAB_TITLE };
  if (view.view === "settings" && view.key === "")
    return { view: settingsView("project"), title: SETTINGS_TAB_TITLE };
  return { view, title };
}

/** What a pane shows. */
export type Content =
  /**
   * A chat's terminal. `chat` is the chat's own name, as the plane records it and as the core
   * was told it — separate from the tab's `name`, because a split starts a second chat under
   * the first one's name and the core must be told that name, not the sentence the tab draws.
   */
  | { kind: "session"; session: number; chat: string }
  /**
   * A view. **It carries its workspace, because it has nothing else to be filed by**: a chat is
   * on the strip of the workspace it works in (the plane's answer, `FiledIn`), and a view works
   * nowhere. So it is on the strip it was opened from — the one in front — and says so.
   */
  | {
      kind: "view";
      view: ViewRef;
      workspace: string;
      /**
       * Put back by a launch, and **not asked anything until the operator presses for it.**
       *
       * An extension's view runs that extension's program when it is asked (ADR 0041 stage 2,
       * *one round trip per deliberate human action*), and a tab the record put back was
       * opened by nobody at this launch — the record is a file in the plane, and a line in it
       * must not become a program run at every start. So a put-back tab waits, says so, and
       * asks on a press; one the operator opens is asked at once. charter's own views run no
       * program and ignore it.
       */
      waits?: boolean;
      /**
       * **The strip's preview tab** (SI-9b, ADR 0065 Q1), VS Code's model: what a single click
       * on a memory row shows, replaced in place by the next single click. A double-click or
       * starting an edit keeps it ({@link keepView}), and a kept tab is an ordinary tab. Drawn
       * in italics on the strip, so the operator can tell which tab the next click reuses.
       */
      preview?: boolean;
    };

export type Layout =
  | { kind: "pane"; pane: number; content: Content }
  | { kind: "split"; direction: Direction; children: [Layout, Layout] };

export type Tab = {
  id: number;
  /**
   * What the tab bar shows: **the name the operator gave the chat, or its default** — or the
   * view's title for a tab that opened on a view.
   *
   * The default is the persona the chat adopted and then its number, `steward 3` rather than
   * `3` (charter-app#130), persona first because that is what the operator reads the strip
   * for (charter-app#254). A number identifies a chat to charter and tells the operator
   * nothing, and the strip is where an operator with fifty of them works out which is which.
   * What goes before the number is known at the moment a tab opens on every path — the picker
   * carries the operator's choice, and a chat put back at a launch carries its own — so this
   * is never filled in later.
   *
   * **A name given is charter's label and nothing else** (charter-app#254). The chat's own
   * name — what its harness was started with, and what a split's chat is started under — is
   * `Content.chat`, and a rename never touches it: a running harness is not disturbed.
   */
  name: string;
  /** What {@link name} goes back to when a given name is taken off: the default above. */
  defaultName: string;
  layout: Layout;
  /** The pane a split or a close acts on. */
  focused: number;
};

/**
 * Which workspace a chat is filed under — the strip its tab appears on.
 *
 * A function rather than a field on the tab, because **the plane is what files a chat**: what
 * relates a chat to a workspace is the directory it works in, and the sidebar the core reads
 * off the plane is the answer to that. A copy on the tab would be a second answer that
 * nothing invalidates when the plane changes under it.
 */
export type FiledIn = (session: number) => string;

export type Tabs = {
  byId: Record<number, Tab>;
  /** Left to right, as the tab bar shows them. */
  order: number[];
  /** The tab on screen, or none when every tab has closed. */
  inFront?: number;
  /** Ids already handed out, so a new tab or pane never reuses one. */
  named: { tabs: number; panes: number };
  /**
   * Where each view's divider was, by {@link viewKey}: its first side's share of the pane in
   * percent — a file tab's tree beside its preview (FM-2). **Kept for a view whose tab closed**,
   * so opening it again brings the divider back where it was; the record keeps it for the tabs
   * open at a quit (`reopen::View::split`).
   */
  splits?: Readonly<Record<string, number>>;
};

/** Where `view`'s divider was, or `undefined` when it never moved. */
export function splitOf(tabs: Tabs, view: ViewRef): number | undefined {
  return tabs.splits?.[viewKey(view)];
}

/** `view`'s divider was moved to `split`. Answers `tabs` itself when it was already there. */
export function setSplit(tabs: Tabs, view: ViewRef, split: number): Tabs {
  if (splitOf(tabs, view) === split) return tabs;
  return { ...tabs, splits: { ...tabs.splits, [viewKey(view)]: split } };
}

export function noTabs(): Tabs {
  return { byId: {}, order: [], named: { tabs: 0, panes: 0 } };
}

/**
 * Opens a tab with one pane showing `session`, in front.
 *
 * `who` is what the default name puts before the chat's number: the persona it adopted, or —
 * with none — the program it runs. `label` is the name the operator gave it, where they gave
 * one, and the tab says that instead.
 */
export function openTab(
  tabs: Tabs,
  session: number,
  chat = "",
  who: string | null = null,
  label: string | null = null,
): Tabs {
  const named = chat || String(tabs.named.tabs + 1);
  return withTab(
    tabs,
    who ? `${who} ${named}` : named,
    { kind: "session", session, chat: named },
    tabs.order.length,
    label,
  );
}

/**
 * Gives a chat's tab the name `label`, or its default back with `null` (charter-app#254).
 *
 * **The tab's name and nothing else**: the chat's own name, which its harness was started
 * with, stays what it was. A tab that opened on a view is named after what it shows, and is
 * left as it is; so is a tab that is not there.
 *
 * **A split tab's name is its own chat's** — the first pane's, {@link chatOf} — whichever pane
 * has the keyboard, because that is the chat the tab is (its state mark and its pin are that
 * chat's too). A relaunch brings each chat back as a tab of its own, so the name comes back on
 * that chat's tab and the chat that was split beside it comes back under its default.
 */
export function renameTab(tabs: Tabs, id: number, label: string | null): Tabs {
  const tab = tabs.byId[id];
  if (!tab || chatOf(tabs, id) === undefined) return tabs;
  return { ...tabs, byId: { ...tabs.byId, [id]: { ...tab, name: label ?? tab.defaultName } } };
}

/**
 * The one place a tab is minted: `content` in one pane, at `at` in the order, in front, named
 * `label` where the operator gave it one and `name` otherwise.
 */
function withTab(
  tabs: Tabs,
  name: string,
  content: Content,
  at: number,
  label: string | null = null,
): Tabs {
  const id = tabs.named.tabs + 1;
  const pane = tabs.named.panes + 1;
  const order = [...tabs.order];
  order.splice(Math.max(0, Math.min(at, order.length)), 0, id);
  return {
    ...tabs,
    byId: {
      ...tabs.byId,
      [id]: {
        id,
        name: label ?? name,
        defaultName: name,
        layout: { kind: "pane", pane, content },
        focused: pane,
      },
    },
    order,
    inFront: id,
    named: { tabs: id, panes: pane },
  };
}

/** What one view is called as a key: unique per plane, and the same for the same view. */
export function viewKey(view: ViewRef): string {
  // Namespaced as `charter_core::panel::Panel::key` is, so an extension that calls itself
  // `charter` cannot answer to charter's own view's name.
  return view.from === null
    ? `charter/${view.view}/${view.key}`
    : `ext/${view.from}/${view.view}/${view.key}`;
}

/** The tab and pane already showing `view`, where one is. */
export function findView(tabs: Tabs, view: ViewRef): { tab: number; pane: number } | undefined {
  const wanted = viewKey(view);
  for (const tab of tabs.order) {
    const found = contents(tabs.byId[tab].layout).find(
      (one) => one.content.kind === "view" && viewKey(one.content.view) === wanted,
    );
    if (found) return { tab, pane: found.pane };
  }
  return undefined;
}

/**
 * Opens `view` in a tab of its own, in front — **or brings forward the one already showing it.**
 *
 * One view, one surface: a second tab for the persona that already has one is two answers to
 * "where is steward" that can drift apart (one scrolled, one searched), and the operator asked
 * for a tab, not a tab per click. The pane showing it takes the focus too, so a view that is
 * one side of a split is the side that answers the keyboard.
 *
 * `workspace` is the strip it goes on: the one in front, which is where it was opened from.
 */
export function openView(tabs: Tabs, view: ViewRef, name: string, workspace: string): Tabs {
  const open = findView(tabs, view);
  if (open) {
    const tab = tabs.byId[open.tab];
    // Opening it is the operator asking for it, so a tab that was waiting for a press has had
    // one (`Content.waits`).
    const layout = replace(tab.layout, open.pane, (found) =>
      found.content.kind === "view" && found.content.waits
        ? { ...found, content: { ...found.content, waits: false } }
        : found,
    );
    return {
      ...tabs,
      byId: { ...tabs.byId, [tab.id]: { ...tab, layout, focused: open.pane } },
      inFront: tab.id,
    };
  }
  return withTab(tabs, name, { kind: "view", view, workspace }, tabs.order.length);
}

/**
 * Opens `view` in a tab of its own **behind the tab in front** — an offer beside what the operator
 * is doing, which asks them nothing until they go to it (FR-18a, W10's interrupt budget). With
 * nothing in front it is in front, as there is nothing else to show. A view already open is left
 * where it is.
 */
export function offerView(tabs: Tabs, view: ViewRef, name: string, workspace: string): Tabs {
  if (findView(tabs, view)) return tabs;
  const opened = withTab(tabs, name, { kind: "view", view, workspace }, tabs.order.length);
  return tabs.inFront === undefined ? opened : { ...opened, inFront: tabs.inFront };
}

/**
 * Shows `view` in the strip's preview tab (SI-9b, ADR 0065 Q1) — **replacing what that tab
 * previewed**, in its place on the strip — or opens a preview tab when the strip has none. A
 * view already open anywhere is brought forward instead, as {@link openView} does, and a kept
 * tab is never replaced: previewing is what a single click does, and it must not take away a
 * tab the operator chose to keep.
 *
 * One per strip: `workspace` is the strip in front, and a preview on another strip is that
 * strip's.
 */
export function openPreview(tabs: Tabs, view: ViewRef, name: string, workspace: string): Tabs {
  if (findView(tabs, view)) return openView(tabs, view, name, workspace);
  const reused = previewOf(tabs, workspace);
  if (reused === undefined) {
    return withTab(tabs, name, { kind: "view", view, workspace, preview: true }, tabs.order.length);
  }
  const tab = tabs.byId[reused];
  const layout = replace(tab.layout, tab.focused, (found) => ({
    ...found,
    content: { kind: "view", view, workspace, preview: true },
  }));
  return {
    ...tabs,
    byId: { ...tabs.byId, [reused]: { ...tab, layout, name, defaultName: name } },
    inFront: reused,
  };
}

/**
 * The strip's preview tab, where it has one: a tab showing one view and nothing else, marked
 * as a preview. A tab split beside a chat is not one, because replacing it would replace the
 * chat's tab.
 */
export function previewOf(tabs: Tabs, workspace: string): number | undefined {
  return tabs.order.find((id) => {
    const layout = tabs.byId[id].layout;
    return (
      layout.kind === "pane" &&
      layout.content.kind === "view" &&
      layout.content.preview === true &&
      layout.content.workspace === workspace
    );
  });
}

/**
 * Keeps the tab showing `view` (SI-9b): it stops being the preview, so the next single click
 * previews in a tab of its own. A double-click on the row or the tab, and starting an edit,
 * keep it. Answers `tabs` itself when nothing changed.
 */
export function keepView(tabs: Tabs, view: ViewRef): Tabs {
  const open = findView(tabs, view);
  if (!open) return tabs;
  const tab = tabs.byId[open.tab];
  let changed = false;
  const layout = replace(tab.layout, open.pane, (found) => {
    if (found.content.kind !== "view" || !found.content.preview) return found;
    changed = true;
    return { ...found, content: { ...found.content, preview: false } };
  });
  return changed ? { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, layout } } } : tabs;
}

/**
 * The pane showing `from` shows `to` instead, and its tab is called `name` — a new memory's tab
 * once it is saved (it shows the memory it made), and a memory's tab once it is retitled. The
 * tab keeps its place and whether it is kept. Answers `tabs` itself when `from` is not shown.
 *
 * **One view, one surface**, as {@link openView} keeps it: when another pane already shows `to`,
 * that one is brought forward and `from` is left as it was — the Settings tab moved to a level
 * whose tab is open (SE-17), a Search asked what another Search tab already shows.
 */
export function showInstead(tabs: Tabs, from: ViewRef, to: ViewRef, name: string): Tabs {
  const open = findView(tabs, from);
  if (!open) return tabs;
  const there = findView(tabs, to);
  if (there && (there.tab !== open.tab || there.pane !== open.pane))
    return openView(tabs, to, name, "");
  const tab = tabs.byId[open.tab];
  const layout = replace(tab.layout, open.pane, (found) =>
    found.content.kind === "view" ? { ...found, content: { ...found.content, view: to } } : found,
  );
  // The tab's name is its lead view's: a view on the far side of a split names nothing.
  const leads = contents(tab.layout)[0]?.pane === open.pane;
  const renamed = leads ? { name, defaultName: name } : {};
  return { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, layout, ...renamed } } };
}

/**
 * Puts back a view tab the last launch recorded, at the place it had — **behind** whatever is in
 * front, because a launch decides what is in front once, from the whole record.
 *
 * `at` is where it was on the strip, over chats and views together. It is a place and not a
 * promise: a chat that did not come back moves it by one, and a place past the end is the end.
 * A view already open is left where it is rather than drawn twice.
 */
export function putViewBack(
  tabs: Tabs,
  view: ViewRef,
  name: string,
  workspace: string,
  at: number,
  split?: number,
): Tabs {
  if (findView(tabs, view)) return tabs;
  const opened = withTab(tabs, name, { kind: "view", view, workspace, waits: true }, at);
  const back = { ...opened, inFront: tabs.inFront };
  return split === undefined ? back : setSplit(back, view, split);
}

/**
 * The operator pressed to have a waiting view asked (`Content.waits`): that pane of the tab in
 * front stops waiting. Anything else is left as it is.
 */
export function stopWaiting(tabs: Tabs, pane: number): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  let changed = false;
  const layout = replace(tab.layout, pane, (found) => {
    if (found.content.kind !== "view" || !found.content.waits) return found;
    changed = true;
    return { ...found, content: { ...found.content, waits: false } };
  });
  return changed ? { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, layout } } } : tabs;
}

/**
 * The tab's own chat: **the session in its first pane, and nothing when that pane is a view.**
 *
 * What a tab's state mark, its pin and the core's "chat in front" are about. A tab that opened
 * on a view and was split to start a chat beside it is still the view's tab — its first pane
 * says what it is — so it draws no chat state and has no chat to pin.
 */
export function chatOf(tabs: Tabs, id: number): number | undefined {
  const first = contentsOf(tabs, id)[0]?.content;
  return first?.kind === "session" ? first.session : undefined;
}

/**
 * The name a new chat started beside this tab's panes is given: the name of the first chat in
 * it, or nothing when it has none — a view's tab split to start a chat starts one with a name of
 * its own, not the view's title.
 */
export function chatNameOf(tabs: Tabs, id: number): string | undefined {
  const tab = tabs.byId[id];
  if (!tab) return undefined;
  const first = contents(tab.layout).find((one) => one.content.kind === "session");
  return first?.content.kind === "session" ? first.content.chat : undefined;
}

/** What each pane of a tab shows, left to right and top to bottom. */
export function contentsOf(tabs: Tabs, id: number): { pane: number; content: Content }[] {
  const tab = tabs.byId[id];
  return tab ? contents(tab.layout) : [];
}

/** What the focused pane of the tab in front shows, when anything is in front. */
export function focusedContent(tabs: Tabs): Content | undefined {
  const tab = frontTab(tabs);
  return tab && contents(tab.layout).find((one) => one.pane === tab.focused)?.content;
}

/**
 * Every view tab follows a workspace renamed from `from` to `to` (charter#367): a view on
 * its strip moves to the new strip, and the workspace's own settings tab is keyed and titled by
 * the new name. The core does the same to the record (`wscmd::rename::Move::view`), so the two
 * agree on what comes back at the next launch.
 *
 * Answers `tabs` itself when nothing moved, so a caller can tell. A chat needs nothing here:
 * its strip is where it works, which the plane answers (`FiledIn`).
 */
export function followRename(tabs: Tabs, from: string, to: string): Tabs {
  let moved = false;
  const oldSettings = viewKey(workspaceSettingsView(from));
  const follow = (layout: Layout): Layout => {
    if (layout.kind === "split")
      return { ...layout, children: layout.children.map(follow) as [Layout, Layout] };
    const content = layout.content;
    if (content.kind !== "view") return layout;
    const strip = content.workspace === from;
    const settings = viewKey(content.view) === oldSettings;
    if (!strip && !settings) return layout;
    moved = true;
    return {
      ...layout,
      content: {
        ...content,
        workspace: strip ? to : content.workspace,
        view: settings ? workspaceSettingsView(to) : content.view,
      },
    };
  };
  const byId = Object.fromEntries(
    tabs.order.map((id) => {
      const tab = tabs.byId[id];
      const layout = follow(tab.layout);
      const lead = contents(layout)[0]?.content;
      const retitle =
        lead?.kind === "view" &&
        viewKey(lead.view) === viewKey(workspaceSettingsView(to)) &&
        tab.name === workspaceSettingsTitle(from);
      const name = retitle ? workspaceSettingsTitle(to) : tab.name;
      return [id, { ...tab, layout, name, defaultName: retitle ? name : tab.defaultName }];
    }),
  );
  return moved ? { ...tabs, byId } : tabs;
}

/**
 * Every view tab whose strip is not a workspace any more, moved to `outside`.
 *
 * A chat's strip is re-read off the plane every time (`FiledIn`), so a workspace that goes takes
 * its chats' strip with it. A view carries its own, so nothing would move it: it would be on a
 * strip that is never drawn, unreachable. This is the one place that says what a view's strip is
 * when the plane stops having it — the same strip a chat that works in no workspace is on.
 *
 * Answers `tabs` itself when nothing moved, so a caller can tell.
 */
export function refileViews(
  tabs: Tabs,
  stands: (workspace: string) => boolean,
  outside: string,
): Tabs {
  let moved = false;
  const refile = (layout: Layout): Layout => {
    if (layout.kind === "split")
      return { ...layout, children: layout.children.map(refile) as [Layout, Layout] };
    const content = layout.content;
    if (content.kind !== "view" || content.workspace === outside || stands(content.workspace))
      return layout;
    moved = true;
    return { ...layout, content: { ...content, workspace: outside } };
  };
  const byId = Object.fromEntries(
    tabs.order.map((id) => [id, { ...tabs.byId[id], layout: refile(tabs.byId[id].layout) }]),
  );
  return moved ? { ...tabs, byId } : tabs;
}

/**
 * Opens a tab for a chat the operator did not open from this window: one a handoff opened
 * (charter-app#204). **It does not take the front.**
 *
 * A handoff is work the operator sent away from the chat they are reading, and the chat it
 * opened is often on another workspace's strip. Taking the front would interrupt the chat on
 * screen or move the window to a workspace nobody asked to look at. The tab is on its strip,
 * and that is the whole of how it is seen.
 *
 * The one exception is a window with nothing in front: there is nothing to interrupt, and a
 * tab on a strip with nothing in front of it is a blank pane.
 */
export function openTabBehind(
  tabs: Tabs,
  session: number,
  chat = "",
  who: string | null = null,
  label: string | null = null,
): Tabs {
  const opened = openTab(tabs, session, chat, who, label);
  return tabs.inFront === undefined ? opened : { ...opened, inFront: tabs.inFront };
}

/**
 * Closes a tab. **The tab beside it IN ITS OWN WORKSPACE** comes to the front if it was the
 * one in front, and nothing is in front when its workspace held nothing else.
 *
 * The workspace is what the strip shows (ADR 0036), so a close that reached across to a tab
 * on another strip would move the operator to a workspace they did not ask for — and leave
 * the strip they are looking at with no selected tab. A plane charter has not read yet files
 * every chat the same way, and then this is the old rule exactly: the tab to the left.
 *
 * **"Beside" means beside ON THE STRIP**, which is why this takes `pinned` too: a pin draws a
 * tab first, and an operator closing the third tab means the tab they can see to its left,
 * not the one that happens to be before it in the order the chats were opened.
 */
export function closeTab(
  tabs: Tabs,
  id: number,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
  background: Backgrounded = nothingBackgrounded,
): Tabs {
  if (!(id in tabs.byId)) return tabs;
  const order = tabs.order.filter((tab) => tab !== id);
  const byId = Object.fromEntries(order.map((tab) => [tab, tabs.byId[tab]]));
  if (tabs.inFront !== id) return { ...tabs, byId, order };
  return { ...tabs, byId, order, inFront: frontWithout(tabs, id, filedIn, pinned, background) };
}

/**
 * **Where the front goes when tab `id` leaves it** — closed, or put into the background by
 * Smart close (SI-8f). One answer for both, because the operator's ruling is that a smart close
 * sends the front exactly where a close would.
 *
 * The tab before it on its strip as the strip draws it without the background, or the first
 * one after it when it was first, or nothing when it was the only one. A tab in the background
 * is never where the front goes: it is leaving too.
 */
function frontWithout(
  tabs: Tabs,
  id: number,
  filedIn: FiledIn,
  pinned: Pinned,
  background: Backgrounded,
): number | undefined {
  const strip = tabsIn(tabs, workspaceOf(tabs, id, filedIn), filedIn, pinned).filter(
    (tab) => tab === id || !background(tab),
  );
  const at = strip.indexOf(id);
  const beside = strip.filter((tab) => tab !== id);
  return at > 0 ? strip[at - 1] : beside[0];
}

/**
 * **Tab `id` put into the background**: its chat is wrapping up (Smart close, SI-8f), so the
 * front goes where closing it would have sent it ({@link frontWithout}) and the tab stays, drawn
 * as a chip at the strip's left edge ({@link tabsIn}). Nothing else changes — not `tabs.order`,
 * which is where it goes back to if the smart close does not end in a record.
 */
export function sendToBackground(
  tabs: Tabs,
  id: number,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
  background: Backgrounded = nothingBackgrounded,
): Tabs {
  if (!(id in tabs.byId) || tabs.inFront !== id) return tabs;
  return { ...tabs, inFront: frontWithout(tabs, id, filedIn, pinned, background) };
}

/**
 * Which workspace a tab belongs to: **its first pane's.** A chat is filed where it works — the
 * plane's answer — and a view where it was opened, which it carries.
 */
export function workspaceOf(tabs: Tabs, id: number, filedIn: FiledIn): string | undefined {
  const first = contentsOf(tabs, id)[0]?.content;
  if (first === undefined) return undefined;
  return first.kind === "session" ? filedIn(first.session) : first.workspace;
}

/**
 * Whether a tab is pinned, which is a fact about the operator and not about the chat.
 *
 * A function rather than a field, for the same reasons `FiledIn` and `LastMoved` are: it is
 * answered somewhere else — the plane's own app record, through the core (ADR 0040) — and a
 * copy on the tab would be a second answer nothing invalidates.
 */
export type Pinned = (id: number) => boolean;

/** Nothing pinned, which is what every caller that does not care about pins passes. */
export const nothingPinned: Pinned = () => false;

/**
 * Whether a tab is in the background: every pane of it is a chat that is wrapping up (Smart
 * close, SI-8f). The core's answer, as `Pinned` is somebody else's — the window is told which
 * chats are being smart-closed (`smartClose.ts`), and a copy on the tab would be a second one.
 */
export type Backgrounded = (id: number) => boolean;

/** Nothing in the background. */
export const nothingBackgrounded: Backgrounded = () => false;

/**
 * The tabs on one workspace's strip, left to right — **pinned ones first** (ADR 0039).
 *
 * **This is not the strip re-ordering itself.** The rule ADR 0039 fixed is that a tab does not
 * MOVE under the cursor: an operator going back to the chat that was third from the left goes
 * there with their hand, and a strip that re-sorted on activity would turn every click into a
 * read. A pin is the opposite of that — it is the operator putting a tab where they want it,
 * once, deliberately, and it is what pinning means in every browser that has it. Nothing
 * moves that the operator did not move.
 *
 * Within each group the order is `tabs.order`'s, which is the order the chats were opened and
 * is never touched. Pinning two tabs does not sort them against each other.
 *
 * **And this is the whole of what a pin does to the overflow**, which ADR 0039 left open: a
 * pinned tab is first, so it is the last thing the strip scrolls away, and it needs no
 * exemption of its own. An exemption — a tab held out of the scroller — would be a second
 * mechanism deciding what is on screen, and the one thing the strip owes is that nothing in
 * it is unreachable.
 *
 * **A tab in the background goes before the pinned ones** (SI-8f; ADR 0039, amended
 * 2026-09-28). Its chat is wrapping up after the operator's own Smart close click, and that
 * click is what moved it — the rule that nothing moves the operator did not move still holds.
 * It is drawn here and nowhere else: `tabs.order` keeps its place, which is where it is drawn
 * again when the smart close ends without a record.
 */
export function tabsIn(
  tabs: Tabs,
  workspace: string | undefined,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
  background: Backgrounded = nothingBackgrounded,
): number[] {
  const here = tabs.order.filter((id) => workspaceOf(tabs, id, filedIn) === workspace);
  const front = here.filter((id) => !background(id));
  return [
    ...here.filter(background),
    ...front.filter(pinned),
    ...front.filter((id) => !pinned(id)),
  ];
}

/**
 * When a chat last moved, as the core counts moves across every plane. Bigger is more recent.
 *
 * A function rather than a field, for the same reason `FiledIn` is: the count is the core's,
 * it arrives on `chat-moved`, and a copy on the tab would be a second answer that nothing
 * invalidates when the next event lands.
 */
export type LastMoved = (session: number) => number;

/** When a TAB last moved: the most recent of the chats in its panes. */
export function movedAt(tabs: Tabs, id: number, lastMoved: LastMoved): number {
  return panesOf(tabs, id).reduce((most, pane) => Math.max(most, lastMoved(pane.session)), 0);
}

/**
 * `ids` most recently moved first — the order the overflow menu lists them in (ADR 0039).
 *
 * **Ties keep the order they came in**, which is the strip's, because `Array.sort` is stable
 * in every engine this app runs on. That matters more than it looks: a chat nothing has been
 * heard about reads `0`, so at a launch every tab ties and the menu is the strip's order
 * rather than a shuffle. A menu whose rows moved between two openings for no reason the
 * operator can see is the aiming defect ADR 0039 refuses, re-introduced in the one surface
 * that was allowed to sort.
 *
 * It answers a new array and never touches `tabs.order`. The strip's order is fixed.
 */
export function byLastActivity(ids: readonly number[], tabs: Tabs, lastMoved: LastMoved): number[] {
  return [...ids].sort(
    (one, other) => movedAt(tabs, other, lastMoved) - movedAt(tabs, one, lastMoved),
  );
}

/**
 * The arrangement after a workspace is focused: one of ITS tabs in front, and none at all
 * when it holds none.
 *
 * `prefer` is the tab that was in front there last, which is what an operator coming back to
 * a workspace means by it. A workspace with nothing in it puts nothing in front rather than
 * leaving another workspace's chat on screen under this workspace's empty strip.
 */
export function showWorkspace(
  tabs: Tabs,
  workspace: string | undefined,
  filedIn: FiledIn,
  prefer?: number,
  pinned: Pinned = nothingPinned,
): Tabs {
  // The strip's order, so "its first" is the tab the operator can see first — which is a
  // pinned one where there is one.
  const here = tabsIn(tabs, workspace, filedIn, pinned);
  if (here.length === 0) return { ...tabs, inFront: undefined };
  return { ...tabs, inFront: prefer !== undefined && here.includes(prefer) ? prefer : here[0] };
}

export function selectTab(tabs: Tabs, id: number): Tabs {
  return id in tabs.byId ? { ...tabs, inFront: id } : tabs;
}

/** Focuses a pane of the tab in front, so the next split or close acts on it. */
export function focusPane(tabs: Tabs, pane: number): Tabs {
  const tab = frontTab(tabs);
  if (!tab || !panes(tab.layout).some((each) => each.pane === pane)) return tabs;
  return { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, focused: pane } } };
}

/**
 * Divides the focused pane in two, the new one showing `session` and focused.
 *
 * `chat` is the new chat's name. It defaults to the name of the tab's own chat, which is what a
 * split has always meant; a tab showing only a view has none, and the caller names it.
 */
export function splitFocusedPane(
  tabs: Tabs,
  direction: Direction,
  session: number,
  chat?: string,
): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  const pane = tabs.named.panes + 1;
  const content: Content = {
    kind: "session",
    session,
    chat: chat ?? chatNameOf(tabs, tab.id) ?? String(session),
  };
  const layout = replace(tab.layout, tab.focused, (focused) => ({
    kind: "split",
    direction,
    children: [focused, { kind: "pane", pane, content }],
  }));
  return {
    ...tabs,
    byId: { ...tabs.byId, [tab.id]: { ...tab, layout, focused: pane } },
    named: { ...tabs.named, panes: pane },
  };
}

/**
 * Closes the focused pane. What shared its split takes the split's place; when it was the
 * tab's only pane, the tab closes with it.
 */
export function closeFocusedPane(
  tabs: Tabs,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
  background: Backgrounded = nothingBackgrounded,
): Tabs {
  const tab = frontTab(tabs);
  if (!tab) return tabs;
  const left = without(tab.layout, tab.focused);
  if (!left) return closeTab(tabs, tab.id, filedIn, pinned, background);
  const focused = panes(left)[0].pane;
  return { ...tabs, byId: { ...tabs.byId, [tab.id]: { ...tab, layout: left, focused } } };
}

/**
 * Takes chat `session`'s pane away, wherever it is — **the window catching up with a chat the core
 * has already closed**: a smart close whose record landed (ADR 0064). What shared its split takes
 * its place; when it was its tab's only pane, the tab closes by `closeTab`'s rule. A session no
 * tab shows changes nothing.
 */
export function closeChat(
  tabs: Tabs,
  session: number,
  filedIn: FiledIn,
  pinned: Pinned = nothingPinned,
  background: Backgrounded = nothingBackgrounded,
): Tabs {
  const found = tabs.order
    .flatMap((id) => panesOf(tabs, id).map((one) => ({ id, ...one })))
    .find((one) => one.session === session);
  if (found === undefined) return tabs;
  const { id, pane } = found;
  const tab = tabs.byId[id];
  const left = without(tab.layout, pane);
  if (!left) return closeTab(tabs, id, filedIn, pinned, background);
  const focused = tab.focused === pane ? panes(left)[0].pane : tab.focused;
  return { ...tabs, byId: { ...tabs.byId, [id]: { ...tab, layout: left, focused } } };
}

/**
 * The panes of a tab **that show a chat**, left to right and top to bottom.
 *
 * Every caller of this is asking about chats — which to end when the tab closes, which one a
 * queue row brings forward, when the tab last moved — and a pane showing a view has none of
 * those. {@link contentsOf} is every pane.
 */
export function panesOf(tabs: Tabs, id: number): { pane: number; session: number }[] {
  return contentsOf(tabs, id).flatMap(({ pane, content }) =>
    content.kind === "session" ? [{ pane, session: content.session }] : [],
  );
}

/** The sessions with a pane on screen: the only ones a terminal is drawing. */
export function visibleSessions(tabs: Tabs): number[] {
  return tabs.inFront === undefined ? [] : panesOf(tabs, tabs.inFront).map((one) => one.session);
}

function frontTab(tabs: Tabs): Tab | undefined {
  return tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
}

type Pane = Extract<Layout, { kind: "pane" }>;

function panes(layout: Layout): Pane[] {
  return layout.kind === "pane" ? [layout] : layout.children.flatMap(panes);
}

function contents(layout: Layout): { pane: number; content: Content }[] {
  return panes(layout).map(({ pane, content }) => ({ pane, content }));
}

/** `layout` with the pane `pane` put through `change`. */
function replace(layout: Layout, pane: number, change: (found: Pane) => Layout): Layout {
  if (layout.kind === "pane") return layout.pane === pane ? change(layout) : layout;
  return {
    ...layout,
    children: layout.children.map((child) => replace(child, pane, change)) as [Layout, Layout],
  };
}

/** `layout` without the pane `pane`, or nothing when it was the only one. */
function without(layout: Layout, pane: number): Layout | undefined {
  if (layout.kind === "pane") return layout.pane === pane ? undefined : layout;
  const [first, second] = layout.children.map((child) => without(child, pane));
  if (!first) return second;
  if (!second) return first;
  return { ...layout, children: [first, second] };
}
