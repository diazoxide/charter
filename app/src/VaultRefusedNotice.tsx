import { useCallback, useEffect, useId, useState } from "react";
import { commands, type PlaneId, type VaultRefused } from "./bindings";
import { listen } from "./here";
import { Notice, type NoticeAction } from "./Notice";

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
 * The other way forward, a chat opened as the vault's persona, is not a button here yet: the
 * chat's own refusal tells it how to dispatch to that persona, and the Notice says so.
 *
 * Only a press does any of it. Where an administrator's policy forbids Allow, it is not
 * offered, and the Notice says what policy forbids and who set it.
 *
 * The core holds what was refused; this reads it when the pane mounts and each time the core
 * says a chat was refused (`chat-vault-refused`), so a refusal that arrives while the pane is
 * away is on it when it comes back.
 */
export function VaultRefusedNotice({ plane, session }: { plane: PlaneId; session: number }) {
  const id = useId();
  const [refused, setRefused] = useState<readonly VaultRefused[]>([]);
  /** What the last press answered, said until it is put away. */
  const [answered, setAnswered] = useState<string>();
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);

  const read = useCallback(() => {
    void commands
      .vaultRefusals(plane, session)
      .then((held) => {
        // A core that answers nothing (an older one, a test's stand-in) holds none.
        if (held.status === "ok") setRefused(Array.isArray(held.data) ? held.data : []);
      })
      // A list that cannot be read shows nothing: the chat's own refusal still names the ways.
      .catch(() => {});
  }, [plane, session]);

  useEffect(() => {
    read();
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<VaultRefused>("chat-vault-refused", (event) => {
          if (gone || event.payload.plane !== plane || event.payload.session !== session) return;
          setAnswered(undefined);
          read();
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [plane, session, read]);

  if (answered !== undefined)
    return (
      <Notice
        cause={`vault-refused:${session}:answered`}
        at="pane"
        tone="news"
        label="Vault"
        onDismiss={() => setAnswered(undefined)}
      >
        {answered}
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
          if (done.data !== null) setAnswered(done.data.said);
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
  const fixes: readonly [NoticeAction, ...NoticeAction[]] =
    newest.locked === null
      ? [
          {
            label: `Allow ${persona} to use this vault`,
            onPress: () => press(() => commands.allowRefusedVault(plane, session, vault)),
          },
          keep,
        ]
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
        {dispatch !== null && (
          <p>
            {newest.locked === null ? "The other way" : "The way forward"} is to have {dispatch} do
            the work. purlis told this chat how to dispatch to it.
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
      fixes={fixes}
      under={under}
    >
      This chat runs as {persona}, and vault <code className="block-allow-target">{vault}</code> is{" "}
      {theirs === null ? "tagged for no persona" : `tagged for ${theirs}`}, so purlis did not open
      it.{newest.locked !== null && ` ${newest.locked}`}
      {said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}
