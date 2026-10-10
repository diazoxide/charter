import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {
  aim,
  CHAT_KEYBOARD,
  inPalette,
  RENAMES_ON_F2,
  narrow,
  PASS_THROUGH_ID,
  PASS_THROUGH_KEY,
  revealSaid,
  type Offer,
  type Ran,
  type Said,
} from "./actions";
import { commands, type FileScope, type FoundFile, type PlaneId } from "./bindings";
import { hitSaid, scopeSaid, useFileFind, type Part } from "./fileFind";
import { opensTheSwitcher } from "./switcherKey";
import { onAMac } from "./tabKeys";
import { landSettingsFocus } from "./settings/entering";
import type { SettingsLink } from "./settings/links";
import { SaidLink } from "./SaidLink";
import { FileIcon } from "./FileIcon";
import { useFileIcons } from "./projectTheme";
import { iconFor } from "./theme/icons";

/**
 * The command palette: every action the window can do, reachable by typing.
 *
 * **It is the primary input** (spec decision 1), and the tmux frame's `F2` before it. The
 * frame replaced a menu with it rather than putting one beside the other, because two
 * answers to "how do I do a thing" is how its single menu became weird in the first place —
 * so the bar's buttons here are not a second list, they are four rows of THIS one, drawn
 * permanently. `actions.catalogue` is the whole of the seam.
 *
 * **Keyboard first, and nothing in it needs a mouse.** `⌘K` opens it from anywhere, `F2` from
 * anywhere but a focused chat tab — where it is the platform's rename key (charter-app#254,
 * `theTabRenamesOnIt`) — and `Ctrl-K` from anywhere but a chat's own terminal (the rule is
 * below); typing narrows it, the arrows move over every row, Enter runs the one it is aimed at,
 * Escape leaves — and the focus goes back where it was, which for an operator mid-chat is the
 * terminal they were typing in.
 * A click selects and runs too; that is a convenience, not the path.
 *
 * **An unavailable row is listed WITH ITS REASON.** It is dimmed and `aria-disabled`, and the
 * reason is a visible sentence beside it rather than the dimming alone — the same rule M1.3b
 * set for chat state marks. Enter aimed at one runs nothing and says the reason.
 *
 * **The key it claims, it can hand back.** `F2` is taken on the window, capture-phase, so a
 * harness that wants `F2` never sees it — and used to have no way to (charter-app#47). A
 * second `F2` closes the palette and sends `F2` to the chat in front, which is `send-prefix`,
 * the idiom of the frame this app replaces. It is the catalogue's own `pane.sendkey` row that
 * runs, not a second path to the same thing, and the palette says so on screen the moment it
 * opens — a way out nobody can find is the same bug with more code in it.
 *
 * **And the rule that decides which keys it may claim at all** (charter-app#106):
 *
 * > A chord a terminal encodes belongs to the chat whenever a chat has the keyboard, unless
 * > the window claims it there deliberately — and a key claimed from under a focused chat
 * > must have a way to hand it back.
 *
 * Three keys, three answers out of one rule. `F2` is claimed from under the chat and has its
 * way back, which is what #47 bought. `⌘K` is claimed everywhere and takes nothing, because a
 * terminal sends nothing for it: xterm.js 6.0.0 answers a `⌘`-chord with `SELECT_ALL` for
 * `⌘A` and with no bytes at all for anything else. `Ctrl-K` is neither — a terminal encodes
 * it as `\x0b`, which is readline's kill-to-end-of-line in the shell every chat starts in, so
 * the chat keeps it and this listener stands back (`theChatKeepsIt`).
 *
 * **A refusal keeps the palette open, in the core's own words.** `purlis_core::worktree`
 * refuses a removal with a sentence naming the repair; the window does not reword it, does
 * not replace it with a generic failure, and shows it beside the rows rather than behind
 * them — the profile picker's rule, for the same reason: a refusal an operator cannot see
 * next to what they were doing is one they cannot act on.
 *
 * **It is the project switcher too** (FR-27), and that is one surface rather than two for the
 * reason the bar's buttons are rows of it: the box, the arrows, Enter, Escape and the keyboard
 * going back where it was are this component's, and a second dialog would be a second copy of
 * each. The switcher is the palette listing only `projects.rows` — the window's own
 * `Switch to project …` rows, the last project the operator was in first — under its own
 * name. It opens on its key (`switcherKey.ts`), on the title bar's button, and on the
 * palette's own `Switch project…` row, which turns the palette it is run from into the
 * switcher rather than closing it. **A further press of its key moves down one**, the way a
 * system's window switcher does, so the key and Enter go back to the last project and the key
 * twice and Enter to the one before that.
 *
 * **And it finds files** (FM-7, #1110; V86 F10): once something is typed, a "Files" group
 * follows the projects — a group of its own, never ranked into the projects' list (ADR 0079's
 * rule for a box that asks two kinds of thing). Its scope follows the window's focus, the
 * branch picked in the explorer, else the project in front, and Tab widens it a rung at a time
 * up to every open project (Shift+Tab narrows); the scope is said above the group. With files
 * to find, the key is the window's whenever a project is open, not only when there are two to
 * switch between, and a lone project's own row, which could only say it is already in front,
 * is not listed.
 */
export function Palette({
  offers,
  projects,
  files,
  said,
  onSettings,
  onRun,
  onOpened,
}: {
  offers: readonly Offer[];
  /**
   * The project switcher's rows, in the order it lists them, and how many times it has been
   * asked for from outside this component — the title bar's button. Absent, there is no
   * switcher and its key is nobody's.
   */
  projects?: { rows: readonly Offer[]; asked: number };
  /**
   * ⌘P's files (FM-7): the scopes it steps through, narrowest first, what a project is called,
   * and how a found file is opened. Absent, the switcher finds no files.
   */
  files?: {
    ladder: readonly FileScope[];
    nameOf: (plane: PlaneId) => string;
    onOpen: (file: FoundFile) => void;
  };
  /** What the last row answered, when it refused or had something to say. */
  said?: Said;
  /** Follows a refusal's way to its setting (#1201): the palette closes, and the window opens
   *  Settings at the group. Absent, the link is not drawn. */
  onSettings?: (link: SettingsLink) => void;
  /** The window carrying out a row. It answers what happened; this decides what to draw. */
  onRun: (offer: Offer) => Ran | Promise<Ran>;
  /** Told whenever the palette opens or closes, so the window can say it is open. */
  onOpened?: (open: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  /** What it is listing: every action, or only the projects (FR-27). */
  const [scope, setScope] = useState<"actions" | "projects">("actions");
  /** The same, for the one keydown listener. */
  const scopeNow = useRef(scope);
  const [query, setQuery] = useState("");
  /** The row the arrows moved to, or nothing while Enter is aimed by `aim`. */
  const [at, setAt] = useState<number>();
  /** The reason the last Enter did nothing, for a row that cannot run. */
  const [held, setHeld] = useState<string>();
  /** Where the keyboard was when the palette opened, so Escape can give it back. */
  const came = useRef<Element | null>(null);
  const box = useRef<HTMLInputElement>(null);
  /** Whether it is up, for the one listener that is registered once and must not be torn
   *  down and rebuilt on every open. */
  const up = useRef(false);
  /** Which rung of `files.ladder` the files are found in: Tab widens it (FM-7). */
  const [rung, setRung] = useState(0);
  /** What the aimed file's Copy path or Reveal answered (#1143), said on the palette's line. */
  const [fileSaid, setFileSaid] = useState<Omit<Said, "from">>();

  /** The rows and the dispatcher as they are right now, for the ONE keydown listener: it is
   *  registered once and must not be rebuilt on every render, so it cannot close over
   *  either. */
  const latest = useRef({ offers, onRun, shown: [] as readonly Offer[], aimed: -1 });

  const close = useCallback(() => {
    setOpen(false);
    setScope("actions");
    setRung(0);
    setQuery("");
    setAt(undefined);
    setHeld(undefined);
    setFileSaid(undefined);
    onOpened?.(false);
    // The keyboard goes back in `giveTheKeyboardBack` and not here: this runs while the
    // surface is still up and still trapping focus, so a `focus()` from here is pulled
    // straight back inside and then dropped on the floor when the surface unmounts.
  }, [onOpened]);

  /**
   * Back to the terminal, the tab or the panel the operator was in.
   *
   * A pane's keyboard lives in xterm's own textarea, which is an ordinary focusable element —
   * so this is the one function that makes the palette something an operator can open
   * mid-sentence. It runs as the dialog's closing move (`onCloseAutoFocus`), which is after
   * the focus trap has let go; Radix would otherwise restore the focus itself, to the same
   * element, without the check that the element is still on the page.
   */
  const giveTheKeyboardBack = useCallback(() => {
    const back = came.current;
    if (back instanceof HTMLElement && back.isConnected) back.focus();
  }, []);

  /** Lists only the projects, from a fresh box: what was typed was typed at the other list. */
  const toTheProjects = useCallback(() => {
    setScope("projects");
    setRung(0);
    setQuery("");
    setAt(undefined);
    setHeld(undefined);
  }, []);

  /** Opens it as the switcher, from wherever the keyboard is; or turns it into one. */
  const openTheProjects = useCallback(() => {
    if (!up.current) {
      came.current = document.activeElement;
      onOpened?.(true);
      setOpen(true);
    }
    toTheProjects();
  }, [onOpened, toTheProjects]);

  /**
   * Runs a row and leaves if it ran. What Enter does, what a click does, and what the second
   * `F2` does — one function, so the chord can never become a second implementation of a row
   * the catalogue already describes.
   */
  const runOffer = useCallback(
    (offer: Offer) => {
      if (!offer.available) {
        // Nothing runs, and the reason is said rather than left to the dimming.
        setHeld(offer.reason);
        return;
      }
      // **The one row that is about this palette itself**: it turns the palette into the
      // switcher, which closing and opening again would do with a flash and the keyboard sent
      // somewhere in between.
      if (offer.does.verb === "switchProject") {
        toTheProjects();
        return;
      }
      setHeld(undefined);
      setFileSaid(undefined);
      // **A project switch closes the palette in the same commit as the switch** (FR-27): it
      // cannot be refused, and closing after the switch's own answer was a second redraw of the
      // whole window, after the project in front had been drawn once already.
      const closesNow = offer.does.verb === "selectProject";
      if (closesNow) close();
      void (async () => {
        const ran = await latest.current.onRun(offer);
        // A refusal is not an ending: the operator is still here, still choosing, and the
        // catalogue may now offer them the answer to it.
        if (ran.ok && !closesNow) close();
      })();
    },
    [close, toTheProjects],
  );

  // The title bar's button, as a new count: each one opens the switcher. The first value is
  // the count the window started with, which asked for nothing.
  const askedFor = useRef(projects?.asked ?? 0);
  useEffect(() => {
    const asked = projects?.asked ?? 0;
    if (asked === askedFor.current) return;
    askedFor.current = asked;
    openTheProjects();
  }, [projects?.asked, openTheProjects]);

  /**
   * Whether this window has a switcher for its key to open, for the one keydown listener: two
   * projects or more, as the title bar's button and the palette's row — or any project at all
   * once there are files to find in it (FM-7). With neither the key is not taken at all — it
   * goes on to whatever has the keyboard, as any key the window has no use for does.
   */
  const switches = (projects?.rows.length ?? 0) > 1 || (files?.ladder.length ?? 0) > 0;
  const switchesNow = useRef(switches);
  useEffect(() => {
    switchesNow.current = switches;
  }, [switches]);

  /** The second press of the key that opened it, as the row that sends it. */
  const handBack = useCallback(() => {
    const offer = latest.current.offers.find((row) => row.id === PASS_THROUGH_ID);
    if (offer) runOffer(offer);
  }, [runOffer]);

  // The key that opens it and the key that always leaves, listened for on the window and
  // CAPTURED. A pane's terminal would otherwise take the keystroke and send it to the
  // harness: xterm reads from its own textarea, and a capturing listener on the window runs
  // before that textarea's does.
  //
  // **Escape is here rather than on the box**, which is where it started. On the box it only
  // works while the box has the keyboard, and "the one key that always leaves" has to be true
  // wherever the focus has got to — a row reached by clicking, a browser that moved focus on
  // its own, or a surface that took it. A palette that cannot be left is the worst thing a
  // modal surface can be, and it is not a state to be one stray focus away from.
  //
  // **A layout effect, so it is listening from the first frame** (#1469). A plain effect runs
  // after the browser paints, and an `F2` pressed as the window appeared was lost in between —
  // the first scenario spec's, on a slow Linux runner. Added in the commit that draws the
  // window, the listener is there before anything can be seen to press it at.
  useLayoutEffect(() => {
    const key = (e: KeyboardEvent) => {
      // The switcher's key (FR-27). A terminal sends nothing for it (`switcherKey.ts`), so it
      // is claimed wherever the keyboard is, and there is nothing to hand back.
      if (switchesNow.current && opensTheSwitcher(e, onAMac())) {
        e.preventDefault();
        e.stopPropagation();
        if (e.repeat) return;
        if (up.current && scopeNow.current === "projects") {
          // Down one, round to the top: the window switcher's own idiom — past a row that
          // cannot run, which is the project already in front.
          const { shown, aimed } = latest.current;
          for (let step = 1; step <= shown.length; step++) {
            const next = (aimed + step) % shown.length;
            if (shown[next]?.available) {
              setAt(next);
              break;
            }
          }
          return;
        }
        openTheProjects();
        return;
      }
      // The chord is the palette's, but not everywhere: one a terminal encodes belongs to the
      // chat while the chat has the keyboard. Falling through here is the whole of the fix —
      // nothing is prevented and nothing is stopped, so the keystroke carries on down to
      // xterm's textarea exactly as it would if the palette were not here at all.
      if (opensIt(e) && !theChatKeepsIt(e) && !theTabRenamesOnIt(e)) {
        e.preventDefault();
        e.stopPropagation();
        // Pressed again while it is already up. tmux answers this with `send-prefix` and so
        // does this: the second `F2` is the operator asking for the key ITSELF, so the
        // palette gets out of the way and the chat in front receives it. `⌘K` hands nothing
        // back, and needs to hand nothing back: a terminal sends no bytes for it, so a second
        // press would be delivering a keystroke the pane never had (charter-app#106). Nor
        // does `Ctrl-K`, for the opposite reason — the chat kept it, and a key that was never
        // taken has nothing to give back.
        //
        // **A held key is not a second press.** A key held down repeats, and without this
        // the palette would open, close, open, close under a resting finger, spraying
        // `ESC O Q` at the chat as it went. `repeat` is the browser saying the operator has
        // not let go, and it is checked on BOTH paths so neither half can flicker.
        if (e.repeat) return;
        if (up.current) {
          // Only from the palette: the switcher lists no chat to hand it to.
          if (e.key === PASS_THROUGH_KEY && scopeNow.current === "actions") handBack();
          return;
        }
        setOpen((was) => {
          if (was) return was;
          came.current = document.activeElement;
          onOpened?.(true);
          return true;
        });
        return;
      }
      // Only while it is up: the picker and the quit warning listen for Escape too, and
      // swallowing theirs would leave them with no way out for the same reason.
      if (e.key === "Escape" && up.current) {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [close, handBack, onOpened, openTheProjects]);

  // The box takes the keyboard as soon as it exists, so the first thing typed narrows.
  useEffect(() => {
    up.current = open;
    if (open) box.current?.focus();
  }, [open]);

  // A lone project is not listed beside the files: its one row could only say it is in front.
  const projectRows =
    files === undefined || (projects?.rows.length ?? 0) > 1 ? (projects?.rows ?? []) : [];
  // A menu's own rows are left out (`menuOnly`, #1468): the palette asks from the chat in front.
  const listing = scope === "projects" ? projectRows : inPalette(offers);
  const rows = open ? narrow(query, listing) : [];
  const ladder = files?.ladder ?? [];
  const finding = open && scope === "projects" && ladder.length > 0;
  /** The rung in use: the ladder can be shorter than when Tab last moved, at the next opening. */
  const rungNow = Math.min(rung, ladder.length - 1);
  const fileScope = finding ? ladder[rungNow] : undefined;
  const found = useFileFind(finding, fileScope, query);
  const fileRows = finding ? found.files : [];
  /** The rows the arrows move over: the projects', then the files'. */
  const total = rows.length + fileRows.length;
  const firstRunnable = aim(rows);
  const aimed =
    at !== undefined && at < total
      ? at
      : firstRunnable >= 0
        ? firstRunnable
        : fileRows.length > 0
          ? rows.length
          : -1;
  const rowId = (index: number) =>
    index < rows.length ? `palette-row-${rows[index]?.id}` : `palette-file-${index - rows.length}`;
  const aimedId = aimed >= 0 && aimed < total ? rowId(aimed) : undefined;
  useEffect(() => {
    latest.current = { offers, onRun, shown: rows, aimed };
    scopeNow.current = scope;
  });

  // The aimed row, brought on screen. The list scrolls at 55vh and the catalogue is 117 rows
  // with fifty chats open (charter-app#48), so past the first screenful the arrows were
  // moving `aria-selected` onto a row nobody could see — an aim an operator cannot read is
  // not an aim. `block: "nearest"` scrolls only when it has to, so the list does not jump
  // under a row that was already visible. Optional call because jsdom has no layout and
  // therefore no `scrollIntoView`: a unit test must not fail for want of a scrollbar.
  useEffect(() => {
    if (aimedId === undefined) return;
    document.getElementById(aimedId)?.scrollIntoView?.({ block: "nearest" });
  }, [aimedId]);

  if (!open) return null;

  const move = (by: number) => {
    if (total === 0) return;
    const from = aimed < 0 ? (by > 0 ? -1 : 0) : aimed;
    setAt((from + by + total) % total);
  };

  /** Opens a found file in its file tab, and leaves. */
  const openFile = (file: FoundFile) => {
    close();
    files?.onOpen(file);
  };

  /**
   * The aimed file's path copied, or the file revealed in the file manager (#1143): the two
   * commands the explorer's row menu runs (FM-10), so the core places the path inside the branch
   * and through no link here as there. The palette stays, so the person goes on finding; Copy
   * path says what it put on the clipboard, and Reveal says nothing when the file manager has
   * come forward with it, as the row menu does. A refusal is said on the palette's line.
   */
  const onFile = (file: FoundFile, does: FileKey) => {
    setFileSaid(undefined);
    const asked =
      does === "copy"
        ? commands.copyBranchPath(
            file.plane,
            file.workspace,
            file.repo,
            file.piece,
            file.path,
            false,
          )
        : commands.revealBranchPath(file.plane, file.workspace, file.repo, file.piece, file.path);
    void asked
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }))
      .then((answer) => {
        // An answer that comes after the palette went is about nothing on screen.
        if (!up.current) return;
        if (answer.status === "error") setFileSaid({ refused: true, words: answer.error });
        else if (does === "copy")
          setFileSaid({ refused: false, words: `Copied the path of ${file.path}.` });
      });
  };

  /** Enter, or a click, on the row at `index` of either group. */
  const runRow = (index: number) => {
    if (index < 0) return;
    if (index < rows.length) runOffer(rows[index]);
    else {
      const file = fileRows[index - rows.length];
      if (file) openFile(file);
    }
  };

  // The aimed file's own answer is the newer one: it is cleared whenever a row runs.
  const showing = fileSaid ?? (said && said.words !== "" ? said : undefined);
  /** The file the arrows or the query aimed at, which Copy path and Reveal act on. */
  const aimedFile = aimed >= rows.length ? fileRows[aimed - rows.length] : undefined;
  const mac = onAMac();
  // The way out of the key this palette claimed, said where the person who needs it is
  // standing: they pressed `F2` meaning to send `F2`, and this is on screen the instant it
  // opened. Only while there is somewhere to send it — otherwise the row below says why, and
  // a hint promising something that would refuse is worse than no hint.
  const handsBack =
    scope === "actions" && offers.some((row) => row.id === PASS_THROUGH_ID && row.available);
  // The switcher's words, where the palette's would be: what it is, what to type, and what
  // its rows are called.
  const switcher = scope === "projects";
  // What the switcher is, said: with files to find, it is where a project or a file is gone to.
  const switcherAsk = finding ? "Go to a project or a file" : "Switch to a project";

  return (
    // A Radix dialog (`docs/ui-primitives.md`). What it adds over the markup that was here is
    // the trap: the box took the keyboard on opening and nothing held it there, so focus could
    // leave for a pane behind the overlay while the palette was still up and modal.
    //
    // **Its Escape stays on the window, deliberately.** Radix listens on the document in the
    // capture phase; this listener is on the window in the capture phase, so it runs first and
    // stops the event — which is the point, because the same listener is what claims `F2` and
    // hands `F2` back, and those two answers have to be decided in one place. Radix's own
    // Escape is wired to the same `close` for the case where the window listener is not the
    // one that sees it.
    <Dialog.Root
      open
      onOpenChange={(up) => {
        if (!up) close();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          className="warning palette"
          aria-label={switcher ? (finding ? switcherAsk : "Project switcher") : "Command palette"}
          // A click outside answers nothing, which is how every surface in this app has always
          // behaved: the way out is Escape or a row. Turned off explicitly rather than left to
          // the default, so a reviewer sees it was decided.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            // The box, so the first thing typed narrows. Radix would otherwise aim at the
            // first tabbable thing in the content, which is the box today and need not stay
            // so.
            e.preventDefault();
            box.current?.focus();
          }}
          onCloseAutoFocus={(e) => {
            e.preventDefault();
            // A row that went into Settings sent the keyboard there (#1206).
            if (!landSettingsFocus()) giveTheKeyboardBack();
          }}
        >
          <label className="palette-ask" htmlFor="palette-query">
            {switcher ? switcherAsk : "Run an action"}
          </label>
          <input
            id="palette-query"
            ref={box}
            className="palette-query"
            type="text"
            role="combobox"
            autoComplete="off"
            aria-expanded="true"
            aria-controls={
              [rows.length > 0 && "palette-rows", fileRows.length > 0 && "palette-files"]
                .filter(Boolean)
                .join(" ") || undefined
            }
            aria-activedescendant={aimedId}
            placeholder={
              switcher
                ? finding
                  ? "Type a project's or a file's name"
                  : "Type a project's name"
                : "Type to narrow"
            }
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              // The aim goes back to `aim` on every keystroke: a row the arrows reached under
              // the last query is not the row that survived this one.
              setAt(undefined);
              setHeld(undefined);
              setFileSaid(undefined);
            }}
            onKeyDown={(e) => {
              // Escape is not here: it is on the window, so it leaves from anywhere.
              const fileKey = aimedFile && fileKeyOf(e.nativeEvent, mac);
              if (aimedFile && fileKey) {
                e.preventDefault();
                if (!e.repeat) onFile(aimedFile, fileKey);
              } else if (e.key === "ArrowDown") {
                e.preventDefault();
                move(1);
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                move(-1);
              } else if (e.key === "Enter") {
                e.preventDefault();
                runRow(aimed);
              } else if (e.key === "Tab" && finding) {
                // Tab widens where files are found, Shift+Tab narrows it, round the ladder.
                e.preventDefault();
                const by = e.shiftKey ? -1 : 1;
                setRung((rungNow + by + ladder.length) % ladder.length);
                setAt(undefined);
              }
            }}
          />

          {handsBack && (
            <p className="palette-through">
              Press {PASS_THROUGH_KEY} again to send {PASS_THROUGH_KEY} to the chat in front.
            </p>
          )}

          {/* The core's sentence, unchanged, beside the rows. A refusal is an alert because it
            is the answer to something the operator just did; a report is a status. */}
          {showing && (
            <p
              className={showing.refused ? "refusal said" : "said"}
              role={showing.refused ? "alert" : "status"}
            >
              <span>{showing.words}</span>
              {showing.settings !== undefined && onSettings !== undefined && (
                <SaidLink
                  way={showing.settings}
                  onFollow={(link) => {
                    close();
                    onSettings(link);
                  }}
                />
              )}
            </p>
          )}
          {held && (
            <p className="held" role="status">
              {held}
            </p>
          )}

          {rows.length === 0 && finding ? null : rows.length === 0 ? (
            <p className="none" role="status">
              {switcher
                ? "No open project matches what you typed."
                : "No action matches what you typed."}
            </p>
          ) : (
            <ul
              id="palette-rows"
              className="palette-rows"
              role="listbox"
              aria-label={switcher ? "Projects" : "Actions"}
            >
              {rows.map((row, index) => (
                <li
                  key={row.id}
                  id={`palette-row-${row.id}`}
                  role="option"
                  aria-selected={index === aimed}
                  aria-disabled={row.available ? undefined : true}
                  className={
                    (index === aimed ? "palette-row aimed" : "palette-row") +
                    (row.available ? "" : " refused")
                  }
                  onClick={() => runOffer(row)}
                >
                  {/* A project's name alone in the switcher: every row there is a switch, and
                    `Switch to project` ten times over is ten times the same three words. */}
                  <span className="palette-title">
                    {switcher ? (row.name ?? row.title) : row.title}
                  </span>
                  {/* The reason, as words. Dimming is decoration; this is the meaning, and it
                    is what a screen reader and a monochrome display both get. */}
                  {!row.available && <span className="palette-why">{row.reason}</span>}
                  {/* What a row that CAN run costs, where its title cannot fit it: ending a
                    chat ends the program it runs, and nothing said so (charter-app#130).
                    In the same slot as the reason, because a row has one or the other. */}
                  {row.available && row.note && <span className="palette-why">{row.note}</span>}
                </li>
              ))}
            </ul>
          )}

          {/* ⌘P's files (FM-7): a group of their own after the projects, never ranked into
            them, under the scope they were found in. */}
          {finding && fileScope && files && (
            <section className="palette-files" aria-label="Files">
              <p className="palette-scope" role="status" aria-label="Where files are found">
                Files in {scopeSaid(fileScope, files.nameOf)}
                {ladder.length > 1 && (
                  <span className="palette-why">
                    {rungNow === ladder.length - 1 ? "Tab to narrow again" : "Tab to widen"}
                  </span>
                )}
              </p>
              {found.partial.map((said) => (
                <p key={said} className="said" role="status">
                  {said}
                </p>
              ))}
              {found.refused.map((why) => (
                <p key={why} className="refusal said" role="alert">
                  {why}
                </p>
              ))}
              {query.trim() !== "" && fileRows.length === 0 && !found.looking && (
                <p className="none" role="status">
                  No file matches what you typed.
                </p>
              )}
              {fileRows.length > 0 && (
                <ul id="palette-files" className="palette-rows" role="listbox" aria-label="Files">
                  {fileRows.map((file, at) => {
                    const index = rows.length + at;
                    const { marks } = hitSaid(file, files.nameOf);
                    return (
                      <li
                        key={`${file.plane}\n${file.workspace}\n${file.repo}\n${file.piece ?? ""}\n${file.path}`}
                        id={`palette-file-${at}`}
                        role="option"
                        aria-selected={index === aimed}
                        className={index === aimed ? "palette-row aimed" : "palette-row"}
                        onClick={() => openFile(file)}
                      >
                        <FoundIcon file={file} />
                        <span className="palette-title">
                          <Marked parts={marks.name} />
                        </span>
                        <span className="palette-why">
                          <Marked parts={marks.folder} />
                          {marks.after}
                        </span>
                      </li>
                    );
                  })}
                </ul>
              )}
              {/* The aimed file's two keys, said while there is a file to act on (#1143). */}
              {aimedFile && (
                <p className="palette-through">
                  Copy path ({fileKeySaid("copy", mac)}) · {revealSaid(navigator.platform)} (
                  {fileKeySaid("reveal", mac)})
                </p>
              )}
            </section>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * A found file's icon (#1145), in the icon theme of the project and workspace it was found in —
 * the theme its own tree draws it with, so a hit looks the same here as in the explorer. A row of
 * its own because the hits of "all open projects" can come from several projects, and each
 * asks its own once.
 */
function FoundIcon({ file }: { file: FoundFile }) {
  const icons = useFileIcons(file.plane, file.workspace);
  const name = file.path.slice(file.path.lastIndexOf("/") + 1);
  return <FileIcon symbol={iconFor(icons, { name, folder: false })} />;
}

/** A file row's text with the letters the query matched marked (#1131): `<mark>`, which a
 *  screen reader may announce as highlighted and which reads the same as plain text. */
function Marked({ parts }: { parts: readonly Part[] }) {
  return parts.map((part, at) =>
    part.matched ? (
      <mark key={at} className="palette-matched">
        {part.text}
      </mark>
    ) : (
      part.text
    ),
  );
}

/** What the aimed file of ⌘P can have done to it from the keyboard (#1143). */
export type FileKey = "copy" | "reveal";

/**
 * Which of the aimed file's keys this keystroke is, if any (#1143): **⌥⌘C and ⌥⌘R on a Mac,
 * Shift+Alt+C and Shift+Alt+R everywhere else** — Copy Path and Reveal in Finder's keys in VS
 * Code's explorer, so the fingers that know an editor know these.
 *
 * They are read only in the palette's box, so no chat or tab ever loses them. ⌘C is the box's
 * own copy, which is why the Mac's has Option in it. Off a Mac, Ctrl is never part of it: Ctrl
 * with Alt is AltGr on Windows, which types a character. A letter is matched by what the key
 * types; on a Mac, Option makes C and R type `ç` and `®`, so a key that types no letter at all
 * is read by its place instead, and a key that types another letter is never this one.
 */
export function fileKeyOf(e: KeyboardEvent, mac: boolean): FileKey | undefined {
  if (!e.altKey || e.ctrlKey) return undefined;
  if (mac ? !e.metaKey || e.shiftKey : !e.shiftKey || e.metaKey) return undefined;
  const typed = e.key.length === 1 && /^[a-z]$/i.test(e.key) ? e.key.toLowerCase() : undefined;
  const letter = typed ?? (e.code === "KeyC" ? "c" : e.code === "KeyR" ? "r" : undefined);
  if (letter === "c") return "copy";
  if (letter === "r") return "reveal";
  return undefined;
}

/** How the palette spells that key, on this platform. */
export function fileKeySaid(does: FileKey, mac: boolean): string {
  const letter = does === "copy" ? "C" : "R";
  return mac ? `⌥⌘${letter}` : `Shift+Alt+${letter}`;
}

/**
 * Whether this keystroke opens the palette.
 *
 * `F2` is the tmux frame's own key, carried over so an operator's fingers do not have to be
 * retrained, and it needs no modifier — which matters because a modifier's name differs by
 * platform and the scenario tests drive a real window. `⌘K` and `Ctrl-K` are what a desktop
 * app is expected to answer; both are accepted on both platforms rather than sniffing one,
 * because the wrong guess is an app with no palette at all.
 */
export function opensIt(e: KeyboardEvent): boolean {
  if (e.key === "F2") return !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
  return (e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey) && !e.altKey;
}

/**
 * Whether the chat keeps this keystroke, although the palette would otherwise open on it.
 *
 * **The question `opensIt` deliberately does not answer.** That one is about the KEY — is
 * this the palette's chord — and this one is about where it landed, because the same chord
 * belongs to two different programs depending on who has the keyboard. Two predicates and not
 * one condition, for the reason `actions` gives about `available` and `reason`: a surface that
 * says the wrong thing should be a defect in one of them rather than an ambiguity in both.
 *
 * **Only a `Ctrl` chord, and that is the whole of the rule** (charter-app#106). A terminal
 * encodes `Ctrl` with a letter as a C0 control byte — xterm.js 6.0.0 answers `Ctrl` plus a
 * key code in 65..90 with `String.fromCharCode(code - 64)`, so `Ctrl-K` is `\x0b`, which is
 * kill-to-end-of-line in readline and in every emacs-keys line editor. Swallowing it on the
 * window is taking an editing key out of the shell every chat starts in. `F2` is not here
 * because #47 already settled it the other way, with a way back; `⌘` is not here because
 * xterm.js sends nothing at all for a `⌘`-chord but `⌘A`, so there is nothing for the window
 * to be taking.
 *
 * **Where it landed, not where the focus is.** A capture listener on the window runs before
 * the focus has any say, and `e.target` is the element the keystroke was delivered to — which
 * inside a pane is xterm's own textarea, a descendant of the marked holder. An undispatched
 * event has no target and is therefore nobody's: that is the palette's, which is what makes
 * `opensIt` testable on a bare `KeyboardEvent`.
 */
export function theChatKeepsIt(e: KeyboardEvent): boolean {
  if (!e.ctrlKey || e.metaKey) return false;
  const on = e.target;
  return on instanceof Element && on.closest(`[${CHAT_KEYBOARD}]`) !== null;
}

/**
 * Whether an `F2` landed on a chat's tab, where it renames the tab rather than opening this
 * (charter-app#254, `actions.RENAMES_ON_F2`).
 *
 * The same question `theChatKeepsIt` asks — where did the keystroke land — about the one other
 * place `F2` means something of its own: the platform's rename key on a focused item. Nothing
 * is taken from the operator: `⌘K` opens the palette from the tab, and `F2` still does from
 * everywhere else, including the chat itself.
 */
export function theTabRenamesOnIt(e: KeyboardEvent): boolean {
  if (e.key !== "F2") return false;
  const on = e.target;
  return on instanceof Element && on.hasAttribute(RENAMES_ON_F2);
}
