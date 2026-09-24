import { useCallback, useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ApproveExtension } from "./ApproveExtension";
import {
  commands,
  type ExtensionAsk,
  type ExtensionTheme,
  type InstalledExtensions,
} from "./bindings";
import { extensionsChanged } from "./extensionsOn";
import { projectThemeChanged } from "./projectTheme";
import {
  BUILT_IN,
  DEFAULT_THEME,
  drawIn,
  inForce,
  load,
  PREFERS_LIGHT,
  SYSTEM,
  systemTheme,
  type Theme,
} from "./theme/theme";
import { forgetTheirTheme, theirThemeOnce } from "./windowprefs";

/**
 * What has contributed what to this window, and the question charter asks before anything new
 * contributes at all.
 *
 * **This is ADR 0041's item 2, and there is no runtime behind it.** An extension is a
 * directory the operator points at; charter reads its manifest, hashes it and everything it
 * declares, lists what it says it brings, and puts none of it in force until it is approved.
 * Today the only thing it can bring that charter acts on is a theme, which is declarative data
 * against a vocabulary charter owns and has nothing to isolate. A declared program is listed
 * and is not started, because there is nothing here that starts one.
 *
 * **It is called an extension and not a plugin, everywhere, deliberately.** `enabledPlugins` in
 * the first-open dialog is Claude Code's plugin list, travelling in a project's committed
 * settings. Two things called "plugin" in two lists in one window is a consent surface that has
 * stopped being read (ADR 0041).
 *
 * **The honest sentence is not written here.** {@link ExtensionAsk.runs_as_you} comes out of
 * `charter_core::extension::RUNS_AS_YOU` and is rendered as given. A dialog that composed its
 * own words about what an extension can reach would drift kinder than the truth one edit at a
 * time — and the truth is that a subprocess runs as the operator does, so what is listed is a
 * declaration and never a limit. A prompt that implied a cage would be worse than no prompt: it
 * would manufacture confidence charter cannot back.
 */
export function Extensions({ onClose }: { onClose: () => void }) {
  const [listed, setListed] = useState<InstalledExtensions | null>(null);
  const [asking, setAsking] = useState<ExtensionAsk | null>(null);
  const [went, setWent] = useState<string | null>(null);

  const reread = useCallback(async () => {
    const answered = await commands.installedExtensions();
    if (answered.status === "error") {
      setWent(answered.error);
      return;
    }
    setWent(null);
    setListed(answered.data);
    // An approval or a removal changes what every project has on, and which themes there are.
    extensionsChanged();
    projectThemeChanged();
    await drawWhatIsInForce({ reread: true });
  }, []);

  // The first read, written as `Opener`'s is: the command's own promise, a `gone` flag, and
  // the state set inside the callback. Not `void reread()` — `react-hooks/set-state-in-effect`
  // reads a call in an effect body as synchronous however far the setState is from it, and the
  // shape that satisfies it is also the one that stops a dialog closed mid-read from setting
  // state on nothing.
  useEffect(() => {
    let gone = false;
    void commands
      .installedExtensions()
      .then((answered) => {
        if (gone) return;
        if (answered.status === "error") setWent(answered.error);
        else setListed(answered.data);
      })
      .catch(() => undefined);
    void drawWhatIsInForce({ reread: true });
    return () => {
      gone = true;
    };
  }, []);

  const add = async () => {
    const picked = await commands.pickExtension();
    if (picked.status === "error") return setWent(picked.error);
    if (picked.data === null) return;
    const read = await commands.installExtension(picked.data);
    if (read.status === "error") return setWent(read.error);
    setWent(null);
    setAsking(read.data);
  };

  const approve = async (ask: ExtensionAsk) => {
    // The fingerprint that was on screen, handed straight back. What is recorded is what was
    // read out loud, so a write between the drawing and the click is not consented to — it
    // makes the next launch ask again (charter-app#123's shape).
    const said = await commands.approveExtension(ask.id, ask.path, ask.fingerprint);
    setAsking(null);
    if (said.status === "error") return setWent(said.error);
    await reread();
  };

  const forget = async (id: string) => {
    const said = await commands.forgetExtension(id);
    if (said.status === "error") return setWent(said.error);
    await reread();
  };

  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content className="warning extensions" aria-labelledby="extensions">
          <Dialog.Title id="extensions">Extensions</Dialog.Title>
          {/* `tabIndex={0}` on every button in this dialog, per `docs/ui-primitives.md`
              (charter-app#186), and this is the surface where it mattered most. Its controls
              are `Add an extension…`, then a `Review` and a `Remove` per installed row, then
              `Done` — so Radix's focus scope covered the first and the last, and every row's
              two buttons sat in the middle, on an engine that will not tab to a `<button>`
              whose `tabindex` is not written down. Reviewing or removing an extension was a
              mouse-only act, on the one surface in the window that is about trust. */}
          <button type="button" tabIndex={0} onClick={() => void add()}>
            Add an extension…
          </button>

          {went && <p className="came-back">{went}</p>}

          {listed?.unreadable && (
            <p className="came-back">
              charter could not read this machine&rsquo;s extension record, so nothing an extension
              declares is in force: {listed.unreadable}
            </p>
          )}

          <h3>charter&rsquo;s own themes</h3>
          <ul className="contributes">
            {(listed?.built_in_themes ?? []).map((name) => (
              <li key={name}>
                <code>{name}</code>
              </li>
            ))}
          </ul>

          <h3>Installed</h3>
          {listed && listed.extensions.length === 0 && (
            <p className="came-back">None. An extension is a directory you point charter at.</p>
          )}
          <ul className="extension-rows">
            {(listed?.extensions ?? []).map((row) => (
              <li key={row.id} data-standing={row.standing}>
                <span className="name">{row.name}</span>
                <code className="where">{row.path}</code>
                <span className="standing">{standingReads(row.standing)}</span>
                {row.refused && <span className="came-back">{row.refused}</span>}
                {row.themes_in_force.length > 0 && (
                  <span className="in-force">Drawing: {row.themes_in_force.join(", ")}</span>
                )}
                {row.ask && (
                  <button type="button" tabIndex={0} onClick={() => setAsking(row.ask)}>
                    Review
                  </button>
                )}
                <button type="button" tabIndex={0} onClick={() => void forget(row.id)}>
                  Remove
                </button>
              </li>
            ))}
          </ul>

          {(listed?.dropped ?? []).length > 0 && (
            <>
              <h3>Dropped from the record</h3>
              <ul className="contributes">
                {listed?.dropped.map((why) => (
                  <li key={why}>{why}</li>
                ))}
              </ul>
            </>
          )}

          {asking && (
            <ApproveExtension
              ask={asking}
              onApprove={(ask) => void approve(ask)}
              onCancel={() => setAsking(null)}
            />
          )}

          <div className="doing">
            <button type="button" tabIndex={0} onClick={onClose}>
              Done
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** What a standing says, in the words to put in front of the operator. */
function standingReads(standing: string): string {
  if (standing === "approved") return "approved";
  if (standing === "changed") return "changed since you approved it — contributing nothing";
  return "not approved — contributing nothing";
}

/** The themes approved extensions contribute, asked once and again when the registry changes. */
let offered: Promise<ExtensionTheme[]> | undefined;
/** Whose themes may be drawn: the project in front's extensions, or every approved one when no
 *  project is in front (charter-app#253, ADR 0048). */
let allowed: ReadonlySet<string> | "every" = "every";
/** What the project in front picked (charter-app#273), as its file holds it — `charter-dark`,
 *  `charter-light`, `system` or `<extension>/<theme>` — or `null` when it picked nothing. */
let picked: string | null = null;
/** Each theme's text, loaded once: so a project switch that keeps the theme repaints nothing. */
const loaded = new Map<string, Theme | null>();
/** Bumped by every draw: a draw that waited on the survey and was overtaken draws nothing. */
let drawing = 0;

/**
 * Draws the theme the project in front has — `on` is what that project has on
 * (`extensionsOn.ts`, the core's `extension::project::resolve`), or `"every"` with no project in
 * front; `pick` is what it picked (`projectTheme.ts`, the core's `extension::project::theme`).
 * A project that turned an extension off gets charter's own theme back, and so does a pick the
 * window holds no such theme for.
 */
export function drawThemeFor(on: ReadonlySet<string> | "every", pick: string | null = null) {
  allowed = on;
  picked = pick;
  if (pick === SYSTEM) followSystem();
  return drawWhatIsInForce();
}

/** For tests: forget what was asked and for whom. */
export function forgetExtensionThemes() {
  offered = undefined;
  allowed = "every";
  picked = null;
  loaded.clear();
  unfollowSystem?.();
  unfollowSystem = undefined;
  forgetTheirTheme();
}

/** How to stop following the system's appearance, while it is followed. */
let unfollowSystem: (() => void) | undefined;

/**
 * Redraws when the operating system turns light or dark, while the project in front follows it.
 * One listener for the life of the window, started by the first project that picks it.
 */
function followSystem() {
  if (unfollowSystem !== undefined || typeof matchMedia !== "function") return;
  const query = matchMedia(PREFERS_LIGHT);
  const changed = () => {
    if (picked === SYSTEM) draw(systemTheme());
  };
  query.addEventListener("change", changed);
  unfollowSystem = () => query.removeEventListener("change", changed);
}

/** Puts `theme` in force, unless it already is — so the terminal is told only of a change. */
function draw(theme: Theme | null) {
  if (theme !== null && theme !== inForce()) drawIn(theme);
}

/**
 * Puts the theme the project in front has on the document.
 *
 * **After the first frame, never before it.** `main.tsx` draws a built-in that is compiled into
 * the bundle precisely so that nothing is read from disk on the way to the first paint (ADR
 * 0026's 2 s cold start). An extension theme is a disk read and a fingerprint of every file the
 * extension declares, so it lands afterwards — when the window knows which project is in front
 * (`App.tsx`). The visible cost is one repaint for an operator who installed one, and the
 * alternative is a slower launch for everybody who did not.
 *
 * **In this order** (charter-app#273, ADR 0048):
 *
 * 1. **What the project picked**: a built-in, the built-in matching the system, or the
 *    extension theme it names — which the window holds only while the project has that extension
 *    on and this machine approved it. A pick the window holds nothing for draws the built-in; the
 *    settings tab says why.
 * 2. With no pick, **the operator's own `theme.json`**, the one theme they wrote for this machine
 *    themselves, drawn before the first frame (`windowprefs.ts`). A project's pick is more
 *    specific than a machine's file, which is why it comes first.
 * 3. With neither, the first theme from an extension the project has on, or charter's own.
 *
 * The text is handed to `theme.load`, which is the parse-and-re-emit rule: a token's text is
 * read into a typed value and a fresh string is written out from it, so the theme's own bytes
 * never reach a stylesheet (ADR 0041 property 4). Nothing here throws; a theme that is not JSON
 * at all is dropped and the window keeps the theme it has.
 */
export async function drawWhatIsInForce({ reread = false } = {}): Promise<void> {
  const mine = ++drawing;
  if (reread) offered = undefined;
  const pick = picked;
  if (pick === SYSTEM) return draw(systemTheme());
  if (pick !== null && Object.prototype.hasOwnProperty.call(BUILT_IN, pick))
    return draw(BUILT_IN[pick]);
  if (pick === null) {
    const theirs = theirThemeOnce();
    if (theirs !== undefined) return draw(theirs);
  }
  offered ??= commands
    .extensionThemes()
    .then((answered) => (answered.status === "ok" ? (answered.data ?? []) : []))
    .catch((): ExtensionTheme[] => []);
  const themes = await offered;
  if (mine !== drawing) return;
  const on = allowed;
  const may = (theme: ExtensionTheme) => on === "every" || on.has(theme.extension);
  const chosen =
    pick === null
      ? // Choosing among several is the project's pick; with none, the first in force.
        themes.find(may)
      : themes.find((theme) => may(theme) && `${theme.extension}/${theme.name}` === pick);
  const theme = chosen === undefined ? DEFAULT_THEME : parsed(chosen.text);
  // A theme that is not JSON at all keeps the window as it is — unless the project picked it,
  // where keeping it would leave the last project's theme on this one: that draws the built-in.
  draw(theme === null && pick !== null ? DEFAULT_THEME : theme);
}

/** A theme's text as a theme, loaded once, or `null` when it is not JSON at all. */
function parsed(text: string): Theme | null {
  const known = loaded.get(text);
  if (known !== undefined) return known;
  let theme: Theme | null;
  try {
    theme = load(JSON.parse(text) as unknown).theme;
  } catch {
    theme = null;
  }
  loaded.set(text, theme);
  return theme;
}
