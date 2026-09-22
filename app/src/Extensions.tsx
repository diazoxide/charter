import { useCallback, useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ApproveExtension } from "./ApproveExtension";
import { commands, type ExtensionAsk, type InstalledExtensions } from "./bindings";
import { drawIn, inForce, load } from "./theme/theme";

/**
 * What has contributed what to this window, and the question charter asks before anything new
 * contributes at all.
 *
 * **This is charter ADR 0041's item 2, and there is no runtime behind it.** An extension is a
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
    await drawWhatIsInForce();
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
    void drawWhatIsInForce();
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

/**
 * Puts the theme an approved extension contributes on the document.
 *
 * **After the first frame, never before it.** `main.tsx` draws a built-in that is compiled into
 * the bundle precisely so that nothing is read from disk on the way to the first paint (ADR
 * 0026's 2 s cold start). An extension theme is a disk read and a fingerprint of every file the
 * extension declares, so it lands afterwards. The visible cost is one repaint for an operator
 * who installed one, and the alternative is a slower launch for everybody who did not.
 *
 * The text is handed to `theme.load`, which is the parse-and-re-emit rule: a token's text is
 * read into a typed value and a fresh string is written out from it, so the theme's own bytes
 * never reach a stylesheet (ADR 0041 property 4). Nothing here throws; a theme that is not JSON
 * at all is dropped and the window keeps the theme it has.
 */
export async function drawWhatIsInForce(): Promise<void> {
  const answered = await commands.extensionThemes();
  if (answered.status === "error" || answered.data.length === 0) return;
  // The first in force. Choosing among several is a preference this machine does not yet keep,
  // and inventing one here would put a setting in a component.
  const chosen = answered.data[0];
  let raw: unknown;
  try {
    raw = JSON.parse(chosen.text);
  } catch {
    return;
  }
  const { theme } = load(raw);
  if (theme !== inForce()) drawIn(theme);
}
