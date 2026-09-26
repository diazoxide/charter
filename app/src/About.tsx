import { useCallback, useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Info } from "lucide-react";
import { commands, type About } from "./bindings";
import { ExternalLink, ReleaseNotes } from "./ReleaseNotes";

/** Where every version's notes are, the same text this dialog shows for one of them. */
const RELEASES = "https://github.com/diazoxide/charter/releases";

/**
 * **About Charter**: which version of the app this is, and what that version brought.
 *
 * The operator asked for it on the title bar's right-hand side, and said what it should open:
 * *"About Charter — that will open news of charter e.g. current version News"*.
 *
 * # The app's version and the app's changelog
 *
 * `app/src-tauri/src/about.rs` answers with the version this build announces (the one the
 * updater compares and the GitHub release is named for) and that version's section of the
 * repository's `CHANGELOG.md`, which is compiled into the binary. The release workflow puts the
 * same section on the GitHub release, so this dialog and the release page say the same thing.
 * `charter news` prints the same file in a terminal.
 *
 * What is drawn follows what the build is (`Build` in the bindings):
 *
 * - **a release**: the version, its date, and its section;
 * - **a dev build** (`0.2.0-dev.42`): a dev build of the next version, and `[Unreleased]` as
 *   what it has so far;
 * - **a version the changelog does not list**, such as a local build of `main`: said plainly,
 *   with `[Unreleased]` beside it.
 *
 * The notes are Markdown and are drawn as Markdown (`ReleaseNotes.tsx`). The body scrolls
 * inside a bounded height, so the title and Close stay put however long a release is.
 *
 * # Asked when it is opened, and once
 *
 * The changelog is compiled in and the version is the bundle's, so the answer cannot change
 * while the process runs. And a launch does not pay for a dialog nobody opened: this is the
 * one surface in the window most operators open once a release. What IS drawn on failure is
 * the ask failing, because a command can always fail.
 */
export function AboutCharter() {
  const [open, setOpen] = useState(false);
  const [about, setAbout] = useState<About>();
  const [trouble, setTrouble] = useState<string>();

  const ask = useCallback(() => {
    // Once. The changelog ships in the binary, so a second ask reads the same bytes.
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
          aria-label="About Charter — what this version brought"
          title="About Charter — what this version brought"
        >
          <Info aria-hidden="true" /> <span>About</span>
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content className="warning update about" aria-describedby="about-what">
          <Dialog.Title>About Charter</Dialog.Title>
          <div id="about-what">
            {trouble !== undefined ? (
              <p className="honest doctor-trouble" role="alert">
                charter could not read what this version brought: {trouble}
              </p>
            ) : about === undefined ? (
              <p className="pending">reading what this version brought…</p>
            ) : (
              <Said about={about} />
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

/** What the core answered, as sentences and the section it named. */
function Said({ about }: { about: About }) {
  const version = <strong data-testid="about-version">{about.version}</strong>;
  const { build, notes } = about;
  return (
    <div className="about-body">
      {build.kind === "release" ? (
        <p>
          This is Charter {version}
          {notes?.date ? `, released ${notes.date}` : ""}.
        </p>
      ) : build.kind === "dev" ? (
        <p>
          This is Charter {version}, a dev build of {build.of}.
        </p>
      ) : (
        <p>This is Charter {version}. The changelog this build carries has no section for it.</p>
      )}
      {notes === null ? (
        // An unlisted version has already been told so, one line up.
        build.kind !== "unlisted" && (
          <p className="honest">The changelog this build carries has nothing to show for it.</p>
        )
      ) : (
        <section aria-labelledby="about-brought">
          <h3 id="about-brought">
            {build.kind === "release" ? `What ${notes.version} brought` : "Not released yet"}
          </h3>
          {notes.markdown === "" ? (
            <p className="honest">Nothing is recorded for it yet.</p>
          ) : (
            <ReleaseNotes markdown={notes.markdown} />
          )}
        </section>
      )}
      <p className="honest">
        Every version&apos;s notes are on the{" "}
        <ExternalLink href={RELEASES}>releases page</ExternalLink>.
      </p>
    </div>
  );
}
