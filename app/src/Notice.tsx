import type { ReactNode } from "react";

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
  const classes = ["notice", `notice-${at}`, tone === "trouble" ? "notice-trouble" : ""]
    .filter(Boolean)
    .join(" ");
  return (
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
}
