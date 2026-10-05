import {
  createContext,
  useContext,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

/**
 * **A Notice: a standing line in a project's window about something true now** (CONTEXT.md,
 * rulings V91a–d, NO-1 #1223).
 *
 * The window used to draw each of these by hand — a `<p>` with a sentence, sometimes a Dismiss,
 * sometimes nothing — and about thirty of them left the operator reading a problem with nothing
 * to press. This component is the one way to draw one, and **its props will not take a Notice
 * with no way out**: it needs at least one of
 *
 * - `fixes` — charter does it, here, on the press (Retry, Undo, Turn the sandbox on);
 * - `link` — it goes to where the problem is fixed (a Settings group, a record, a view);
 * - `copy` — the command to run elsewhere, **the last resort** (V91q): allowed only where the
 *   window has no fix, and every Notice whose only remedy it is, is listed as debt by
 *   `Notice.guard.test.ts`;
 * - `onDismiss` — hides it.
 *
 * **`cause` names what the line is about**, stably (`pin-gone:ide`, `chat-did-not-start:3`).
 * It is what a dismissal is keyed by once Dismiss lasts until the cause changes (NO-2), and it
 * is on the element as `data-cause` so a test can name the Notice it means.
 *
 * `tone` is `news` (nothing is wrong, the operator is being told) or `trouble` (something went
 * wrong), and it is only the look: a Notice is always a polite `status`, because it stands
 * until it is dealt with and an `alert` would interrupt a screen reader on every relaunch. `at` is where it stands: under the strip (`band`, the
 * default), or in a pane's corner over its terminal, where it takes no row (`pane`).
 *
 * `guard.test` in this folder fails on a standing line built any other way, so a new dead end
 * cannot come back in.
 */
export type NoticeAction = {
  /** The button's words: a verb for what it does ("Undo", "Open record"). */
  label: string;
  onPress: () => void;
};

type Ways = {
  /** What charter can do about it on a press, in the order they read. */
  fixes?: readonly [NoticeAction, ...NoticeAction[]];
  /** Where it is fixed. */
  link?: NoticeAction;
  /** The command that fixes it, offered as Copy command. The last resort, and debt (V91q). */
  copy?: string;
  /** Hides the Notice. */
  onDismiss?: () => void;
};

/** At least one way out, said in the type: a Notice with none does not compile. */
type WayOut =
  | (Ways & { fixes: readonly [NoticeAction, ...NoticeAction[]] })
  | (Ways & { link: NoticeAction })
  | (Ways & { copy: string })
  | (Ways & { onDismiss: () => void });

export type NoticeProps = WayOut & {
  /** What the line is about, stably: what a dismissal will be keyed by. */
  cause: string;
  tone?: "news" | "trouble";
  at?: "band" | "pane";
  /** The accessible name, where the sentence alone would not make a good one. */
  label?: string;
  /** The sentence (and anything else the line says before its ways out). */
  children: ReactNode;
};

export function Notice(props: NoticeProps) {
  const { cause, tone = "news", at = "band", label, children } = props;
  const { fixes, link, copy, onDismiss } = props as Ways;
  const band = useContext(Band);
  const stacked = band !== null && at === "band";
  const id = useId();
  // Where this Notice is drawn when a band stacks it: an element of its own, which the band
  // puts among the two shown, in the "+N more" list, or nowhere, in importance order.
  const [host] = useState(() => document.createElement("div"));
  useLayoutEffect(
    () => (stacked ? band.register(id, { cause, tone, host }) : undefined),
    [band, cause, host, id, stacked, tone],
  );
  const classes = ["notice", `notice-${at}`, tone === "trouble" ? "notice-trouble" : ""]
    .filter(Boolean)
    .join(" ");
  const drawn = (
    <div className={classes} role="status" aria-label={label} data-cause={cause}>
      <div className="notice-says">{children}</div>
      {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#186). */}
      {fixes?.map((fix) => (
        <button
          key={fix.label}
          type="button"
          className="notice-fix"
          tabIndex={0}
          onClick={fix.onPress}
        >
          {fix.label}
        </button>
      ))}
      {link && (
        <button type="button" className="notice-link" tabIndex={0} onClick={link.onPress}>
          {link.label}
        </button>
      )}
      {copy !== undefined && (
        <button
          type="button"
          className="notice-copy"
          tabIndex={0}
          title={copy}
          onClick={() => void navigator.clipboard?.writeText(copy).catch(() => undefined)}
        >
          Copy command
        </button>
      )}
      {onDismiss && (
        <button type="button" className="notice-dismiss" tabIndex={0} onClick={onDismiss}>
          Dismiss
        </button>
      )}
    </div>
  );
  return stacked ? createPortal(drawn, host) : drawn;
}

/**
 * **How important a Notice is, by the family of its cause** (V91i): trouble before news, then
 * this order, then the one drawn first. A family is the cause up to its first `:`; one not in
 * the list comes after every one that is.
 *
 * - an Undo that lasts a few seconds, before anything that waits;
 * - what the operator just did, before what charter found;
 * - an offer, before news about how things came back;
 * - a chat that lost its conversation, before one charter had to guess about, before one that
 *   came back as it was.
 */
export const IMPORTANCE: readonly string[] = [
  "window-trouble",
  "chat-did-not-start",
  "memory-deleted",
  "pin-forgotten",
  "session-saved",
  "sandbox-offer",
  "pin-dormant",
  "chat-fresh",
  "chat-guessed",
  "chat-resumed",
];

/** How many Notices stand under the strip; the rest are behind "+N more" (V91i). */
export const SHOWN = 2;

/** A cause's family: what it is up to the first `:`. */
const familyOf = (cause: string) => cause.split(":", 1)[0];

const rank = (cause: string) => {
  const at = IMPORTANCE.indexOf(familyOf(cause));
  return at < 0 ? IMPORTANCE.length : at;
};

/**
 * **The families of Notice that also come from the doctor** (V91i): only these count in the
 * status bar, where the doctor's button already counts them, so a Notice is never counted
 * twice and never counted as something the doctor did not find. None yet: the doctor's rows
 * become Notices with the fix registry (#1221), and that ticket adds their families here.
 */
export const FROM_THE_DOCTOR: ReadonlySet<string> = new Set();

/** Whether a Notice with this cause counts in the status bar. */
export const countsInStatusBar = (cause: string): boolean => FROM_THE_DOCTOR.has(familyOf(cause));

type Stacked = { cause: string; tone: "news" | "trouble"; host: HTMLElement };
type Entry = Stacked & { seq: number };

/** What a band is to the Notices drawn inside it. */
const Band = createContext<{
  register: (id: string, notice: Stacked) => () => void;
} | null>(null);

/**
 * **The Notices under the strip, stacked** (V91i, NO-2 #1229): at most {@link SHOWN}, the most
 * important first ({@link IMPORTANCE}); the rest behind **+N more**, which opens them as a list
 * of the same Notices, with their ways out, in the same order.
 *
 * Every `at="band"` Notice drawn anywhere inside it is stacked — the window's own, and those a
 * component draws (the sandbox offer, a memory's Undo) — so nothing has to hand the band its
 * lines. Each Notice is drawn into an element of its own, and the band only moves those
 * elements: the order is the band's, what each says is still its Notice's.
 */
export function NoticeBand({ children }: { children: ReactNode }) {
  const [entries, setEntries] = useState<ReadonlyMap<string, Entry>>(() => new Map());
  const [open, setOpen] = useState(false);
  const shownAt = useRef<HTMLDivElement>(null);
  const listAt = useRef<HTMLDivElement>(null);
  const next = useRef(0);

  const band = useMemo(
    () => ({
      register: (id: string, notice: Stacked) => {
        setEntries((was) =>
          new Map(was).set(id, { ...notice, seq: was.get(id)?.seq ?? next.current++ }),
        );
        return () =>
          setEntries((was) => {
            const without = new Map(was);
            without.delete(id);
            return without;
          });
      },
    }),
    [],
  );

  const ordered = useMemo(
    () =>
      [...entries.values()].sort(
        (a, b) =>
          Number(b.tone === "trouble") - Number(a.tone === "trouble") ||
          rank(a.cause) - rank(b.cause) ||
          a.seq - b.seq,
      ),
    [entries],
  );
  const behind = Math.max(ordered.length - SHOWN, 0);
  // Nothing behind it: the list closes, so the next one to fall behind does not open it again.
  // Adjusted while rendering, as React has state follow what it is drawn from.
  if (behind === 0 && open) setOpen(false);

  // Before the frame is painted, so a Notice is never seen out of its place.
  useLayoutEffect(() => {
    const place = (at: HTMLElement | null, hosts: HTMLElement[]) => {
      if (at === null) return;
      for (const child of [...at.children])
        if (!hosts.includes(child as HTMLElement)) child.remove();
      for (const host of hosts) at.appendChild(host);
    };
    const hosts = ordered.map((one) => one.host);
    place(shownAt.current, hosts.slice(0, SHOWN));
    place(listAt.current, open ? hosts.slice(SHOWN) : []);
  }, [open, ordered]);

  return (
    <Band.Provider value={band}>
      {children}
      <div className="notice-band-stack">
        <div className="notice-band-shown" ref={shownAt} />
        {behind > 0 && (
          <button
            type="button"
            className="notice-more"
            tabIndex={0}
            aria-expanded={open}
            onClick={() => setOpen((was) => !was)}
          >
            +{behind} more
          </button>
        )}
        <div
          className="notice-more-list"
          ref={listAt}
          role="group"
          aria-label="More notices"
          hidden={!open || behind === 0}
        />
      </div>
    </Band.Provider>
  );
}
