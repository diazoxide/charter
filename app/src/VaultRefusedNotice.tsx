import { useId, useState } from "react";
import { commands, type PlaneId } from "./bindings";
import { useAskPersona } from "./AskPersona";
import { useDispatchesHeld } from "./dispatchesHeld";
import { Notice, type NoticeAction } from "./Notice";
import { useVaultRefusals } from "./vaultRefusals";

/** What a press answers: the sentence the Notice then says, or nothing for Keep blocked. */
type Answer = { status: "ok"; data: { said: string } | null } | { status: "error"; error: string };

/**
 * **A vault this chat was refused for its persona, on its tab** (#1430), like a blocked host's:
 * which vault, which persona the chat runs as, and who the vault is tagged for. It is never a
 * dead end. It offers
 *
 * - **Allow {persona} to use this vault**: every chat opened as that persona may use the vault
 *   in this project on this machine. The core takes the persona from its own record of the
 *   chat, audits it, and keeps it where no chat writes; it is listed in Settings › Sandbox ›
 *   Granted and revoked there. The chat does not restart: its next run reads it. The core
 *   tells the chat to run the command again only when that is safe (a waiting chat now, a
 *   chat mid-turn when its turn ends), and otherwise the Notice says to ask the chat.
 * - **Keep blocked**: puts the Notice away. The chat's next try raises it again.
 *
 * - **Dispatch to {persona}…**, where the vault is tagged for a persona the project defines
 *   (#1438): opens Ask {persona} for this chat, the dialog its tab's menu opens, and you type
 *   what to ask. The chat's own refusal also told it how to dispatch to that persona.
 *
 * **The dialog opens empty.** What is typed there reaches the new chat as your own words, so
 * this Notice hands it nothing a chat produced: not the command that was refused, and not the
 * vault's name. It names the chat and the persona, which are purlis's own records. The dialog
 * says so when it is opened from here.
 *
 * - **Show the request**, in place of Dispatch to {persona}…, where this chat has already asked
 *   that persona and its dispatch is held for you (#1481): the Notice says so and the button
 *   goes to that question, on this pane. Dispatching is the chat's own work; the manual way is
 *   for when the chat has not asked, and offering both sent the person to an empty form while
 *   the chat's own request waited for an answer.
 *
 * Only a press does any of it. Where an administrator's policy forbids Allow, it is not
 * offered, and the Notice says what policy forbids and who set it.
 *
 * The persona the chat runs as is drawn as the sentence's mark, on what Allow answered too
 * (#1454).
 *
 * The core holds what was refused; this reads it when the pane mounts and each time the core
 * says a chat was refused (`chat-vault-refused`), so a refusal that arrives while the pane is
 * away is on it when it comes back.
 */
export function VaultRefusedNotice({ plane, session }: { plane: PlaneId; session: number }) {
  const id = useId();
  const { refused, heard, read } = useVaultRefusals(plane, session);
  /** What the last press answered, and the persona it was for, said until it is put away. */
  const [answered, setAnswered] = useState<{ persona: string; said: string }>();
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  const askPersona = useAskPersona();
  const { waiting } = useDispatchesHeld(plane, session);

  // The core said this chat was refused again: what the last press answered is put away, so
  // the new refusal is what the pane says. Adjusted while rendering, as React has state
  // follow what it is drawn from.
  const [heardAt, setHeardAt] = useState(heard);
  if (heardAt !== heard) {
    setHeardAt(heard);
    setAnswered(undefined);
  }

  if (answered !== undefined)
    return (
      <Notice
        cause={`vault-refused:${session}:answered`}
        at="pane"
        tone="news"
        label="Vault"
        persona={answered.persona}
        onDismiss={() => setAnswered(undefined)}
      >
        {answered.said}
      </Notice>
    );

  const newest = refused[refused.length - 1];
  if (newest === undefined) return null;
  const { vault, persona } = newest;
  const theirs = newest.tagged_for;
  const more = refused.length - 1;
  const behind =
    more > 0 ? ` ${more} more ${more === 1 ? "vault" : "vaults"} behind this one.` : "";

  const press = (ask: () => Promise<Answer>) => {
    if (busy) return;
    setBusy(true);
    setSaid(undefined);
    void ask()
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else {
          if (done.data !== null) setAnswered({ persona, said: done.data.said });
          read();
        }
      })
      .catch((err: unknown) => setSaid(`purlis could not do that: ${String(err)}`))
      .finally(() => setBusy(false));
  };

  // Allow only where policy leaves it open; Keep blocked always.
  const keep: NoticeAction = {
    label: "Keep blocked",
    onPress: () => press(() => commands.keepVaultBlocked(plane, session, vault)),
  };
  // Ask the vault's own persona, where the project defines it: the dialog, and nothing typed
  // for you (see above). Policy that forbids Allow does not forbid this; what it does forbid
  // of dispatch, the dialog's answer says.
  // Unless this chat has already asked that persona: then the way forward is its own request,
  // waiting on this pane, and the button goes there.
  const asked =
    newest.dispatch_to !== null && waiting.some((one) => one.target === newest.dispatch_to);
  const dispatchTo: NoticeAction[] =
    newest.dispatch_to === null
      ? []
      : asked
        ? [{ label: "Show the request", onPress: () => showTheRequest(session) }]
        : [
            {
              label: `Dispatch to ${newest.dispatch_to}…`,
              onPress: () => {
                if (newest.dispatch_to !== null)
                  askPersona(session, newest.dispatch_to, undefined, "notice");
              },
            },
          ];
  const fixes: readonly [NoticeAction, ...NoticeAction[]] =
    newest.locked === null
      ? [
          {
            label: `Allow ${persona} to use this vault`,
            onPress: () => press(() => commands.allowRefusedVault(plane, session, vault)),
          },
          ...dispatchTo,
          keep,
        ]
      : dispatchTo.length > 0
        ? [dispatchTo[0], keep]
        : [keep];

  // What a press would do, on screen before it, and the way forward that is the chat's own.
  const dispatch = newest.dispatch_to;
  const under =
    newest.locked !== null && dispatch === null ? undefined : (
      <div className="block-allow" id={id}>
        {newest.locked === null && (
          <p>
            Allow lets every chat opened as {persona} use vault{" "}
            <code className="block-allow-target">{vault}</code> in this project on this machine.
            This chat does not restart. You can revoke it in Settings › Sandbox › Granted.
          </p>
        )}
        {dispatch !== null && !asked && (
          <p>
            {newest.locked === null ? "The other way" : "The way forward"} is to have {dispatch} do
            the work. Dispatch to {dispatch}… asks it from this chat, in your words. purlis also
            told this chat how to dispatch to it.
          </p>
        )}
        {dispatch !== null && asked && (
          <p>
            {newest.locked === null ? "The other way" : "The way forward"} is to have {dispatch} do
            the work, and this chat has asked it to. Nothing starts until you answer that request.
          </p>
        )}
      </div>
    );

  return (
    <Notice
      cause={`vault-refused:${session}:${vault}`}
      at="pane"
      tone="trouble"
      label="Vault"
      persona={persona}
      fixes={fixes}
      under={under}
    >
      This chat runs as {persona}, and vault <code className="block-allow-target">{vault}</code> is{" "}
      {theirs === null ? "tagged for no persona" : `tagged for ${theirs}`}, so purlis did not open
      it.{newest.locked !== null && ` ${newest.locked}`}
      {asked && ` This chat has already asked ${newest.dispatch_to}: answer that above.`}
      {said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}

/**
 * **Show the request**: brings this chat's held dispatch, the Notice that asks about it on the
 * same pane, into view and puts the keyboard on it.
 *
 * On its line, never on one of its buttons: the focus moves on a press, and the next key must
 * not be able to answer the question.
 */
function showTheRequest(session: number) {
  const request = document.querySelector<HTMLElement>(`[data-cause^="dispatch-grant:${session}:"]`);
  if (request === null) return;
  request.scrollIntoView?.({ block: "nearest" });
  request.tabIndex = -1;
  request.focus();
}
