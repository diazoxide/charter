import { useCallback, useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Info } from "lucide-react";
import { commands, type About } from "./bindings";

/**
 * **About charter** — which charter this is, and what the version it brought brought.
 *
 * The operator asked for it on the title bar's right-hand side, and said what it should open:
 * *"About Charter — that will open news of charter e.g. current version News"*.
 *
 * # It reads charter's own news, and there is no second copy
 *
 * `app/src-tauri/src/about.rs` answers out of [`charter_core::news`], which `build.rs`
 * compiles into the binary. That is the same corpus behind `charter news`, behind a Release
 * body, and behind the plane pin's dialog one row down (`Updates.tsx`'s `PinItem`, which
 * lists `news::between(pin, brought)`). A dialog with release notes of its own would be a
 * fifth copy that nothing keeps honest — which is the drift the news module's own docstring
 * was written to prevent.
 *
 * **The pin dialog and this are not the same statement**, which is why both exist:
 *
 * - The **pin** is about a PROJECT — what this control plane pins against what this charter
 *   brought — so it is drawn only when that plane drifts, and it lives on the status line
 *   beside the project it is about.
 * - **About** is about the BINARY. It needs no plane, it is always available, and it answers
 *   the question a desktop app is expected to answer from that menu on every platform: what
 *   am I running, and what came with it.
 *
 * # Asked when it is opened, and once
 *
 * The corpus is compiled in, so the answer cannot change while the process runs — there is
 * nothing to re-read and nothing to invalidate. And a launch does not pay for a dialog nobody
 * opened: this is the one surface in the window that most operators will open once a release.
 *
 * # The empty state that cannot happen, said rather than drawn
 *
 * `news::shipped_version` is *derived from* the entries — it is the newest released entry's
 * own version — so the version this names always has at least one note. There is no "this
 * version brought nothing" list to draw, and `about.rs` has the test that keeps it that way.
 * What IS drawn is the ask failing, because a command can always fail.
 */
export function AboutCharter() {
  const [open, setOpen] = useState(false);
  const [about, setAbout] = useState<About>();
  const [trouble, setTrouble] = useState<string>();

  const ask = useCallback(() => {
    // Once. The corpus ships in the binary, so a second ask reads the same bytes.
    if (about !== undefined) return;
    void commands
      .aboutCharter()
      .then((said) => {
        setAbout(said);
        setTrouble(undefined);
      })
      .catch((why: unknown) => setTrouble(String(why)));
  }, [about]);

  // Asked on the FIRST open rather than in the click handler, so that a dialog opened from a
  // keyboard, from a restored `open`, or from anything else that may come to sit on this
  // state asks too — there is one way in and it is this effect.
  useEffect(() => {
    if (open) ask();
  }, [ask, open]);

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        {/* `tabIndex={0}`, per `docs/ui-primitives.md` (charter-app#186): WebKit leaves a
            `<button>` out of the tab sequence unless its `tabindex` is written down. It also
            does a second job here — Tauri's drag handler treats any element carrying a
            `tabindex` other than `-1` as clickable and stops the drag at it, which is what
            keeps this button a button on a bar that is otherwise a drag region. */}
        <button
          type="button"
          tabIndex={0}
          className="title-about"
          data-testid="title-about"
          aria-label="About charter — what this version brought"
          title="About charter — what this version brought"
        >
          <Info aria-hidden="true" /> <span>About</span>
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content className="warning update about" aria-describedby="about-what">
          <Dialog.Title>About charter</Dialog.Title>
          <div id="about-what">
            {trouble !== undefined ? (
              <p className="honest doctor-trouble" role="alert">
                charter could not read what this version brought: {trouble}
              </p>
            ) : about === undefined ? (
              <p className="pending">reading what this version brought…</p>
            ) : (
              <>
                <p className="honest">
                  This is charter <strong data-testid="about-version">{about.version}</strong>.
                </p>
                <h3 id="about-brought">What {about.version} brought</h3>
                <ul className="about-news" aria-labelledby="about-brought">
                  {about.notes.map((note) => (
                    <li key={note.headline}>
                      <strong>{note.headline}</strong>
                      {note.body && <p>{note.body}</p>}
                    </li>
                  ))}
                </ul>
                {/* Where the rest of it is. The dialog is one version's, deliberately — the
                    range between two versions is the PIN's question and the status line
                    already answers it for the project in front. */}
                <p className="honest">
                  <code>charter news</code> lists every version&apos;s, and the status line says
                  when this project pins an older charter.
                </p>
              </>
            )}
          </div>
          {/* `tabIndex={0}` on the one control, per `docs/ui-primitives.md`. */}
          <div className="answer">
            <Dialog.Close asChild>
              <button type="button" tabIndex={0}>
                Close
              </button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
