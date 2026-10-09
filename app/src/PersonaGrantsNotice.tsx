import { useEffect, useState } from "react";
import { commands, type GrantsHeld, type PlaneId } from "./bindings";
import { Notice } from "./Notice";

/**
 * **A chat that holds another persona's grants instead of its own** (#1362, D-1362-5 and
 * D-1362-6), on its tab. A chat resumed from a session record as a persona wider than the
 * default, or one an older purlis handed off to a persona whose hosts reach past the asking
 * chat's, runs with the narrower grants until the person allows its own here: no chat widens
 * what it reaches on its own say. A handoff no longer starts a chat held this way (#1444): it is
 * a dispatch, and the dispatch grant is what consents to it.
 *
 * **Allow** records it (`allow_persona_grants`). A chat's sandbox is compiled as it starts, so
 * the persona's hosts reach it from its next start; **Restart now** asks for the chat's restart
 * on its conversation (`onRestart`), which the window makes once its turn has ended, so a turn
 * is never cut off. Whether one is owed is the window's to say (`owed`), not this Notice's to
 * remember: after a restart that was refused nothing is owed, and Restart now asks again.
 * **Keep** puts the Notice away and changes nothing: it is shown again when the tab opens again.
 *
 * **Where an administrator's policy forbids a persona's own hosts** (#1343), allowing them would
 * reach nothing: the Notice says so, naming the policy and who set it, and offers no Allow.
 *
 * **Where the persona's hosts also wait for this machine's Allow** (#1362, D-1362-7), lifting the
 * hold alone reaches none of them: the Notice says so, before and after Allow, and names where
 * the other Allow is (the project's notice, or Settings › Sandbox).
 */
export function PersonaGrantsNotice({
  plane,
  session,
  running,
  owed,
  onRestart,
}: {
  plane: PlaneId;
  session: number;
  /** Whether the chat is mid-turn: the Notice then says its restart waits for the turn to end. */
  running: boolean;
  /** Whether the window owes this chat a restart now: one asked for and not yet made. */
  owed: boolean;
  /** Asks for the chat's restart on its conversation. The window waits for its turn to end. */
  onRestart: () => void;
}) {
  const [held, setHeld] = useState<GrantsHeld>();
  const [allowed, setAllowed] = useState(false);
  const [kept, setKept] = useState(false);

  useEffect(() => {
    let live = true;
    void commands
      .personaGrantsHeld(plane, session)
      .then((said) => {
        if (live && said.status === "ok") setHeld(said.data ?? undefined);
      })
      // A chat whose hold cannot be read says nothing here: it keeps holding, which is safe.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [plane, session]);

  if (held === undefined || kept) return null;
  const persona = held.persona ?? "its persona";
  const alsoHere = held.waits_here
    ? ` ${persona}'s hosts also wait for your Allow on this machine, on the project's notice or in Settings › Sandbox; until then this chat reaches none of them, even after it restarts.`
    : "";

  if (allowed)
    return (
      <Notice
        cause={`persona-grants:${session}`}
        persona={held.persona}
        at="pane"
        label="Persona's hosts allowed"
        // Asking twice asks for one restart: the window holds it from the first press, and
        // the pane then shows the new run, whose hold is gone.
        fixes={[{ label: "Restart now", onPress: onRestart }]}
      >
        {held.waits_here
          ? `Allowed for this chat.${alsoHere}`
          : `Allowed. This chat reaches ${persona}'s hosts from its next start.`}
        {owed && running && " It restarts, resuming its conversation, when this turn ends."}
      </Notice>
    );

  const allow = () =>
    void commands
      .allowPersonaGrants(plane, session)
      .then((said) => {
        if (said.status === "ok") setAllowed(true);
      })
      .catch(() => {});

  if (held.locked !== null)
    return (
      <Notice
        cause={`persona-grants:${session}`}
        persona={held.persona}
        at="pane"
        label="Persona's hosts held"
        fixes={[{ label: "Keep", onPress: () => setKept(true) }]}
      >
        This chat can't reach {persona}'s hosts. {held.locked}
      </Notice>
    );

  const how =
    held.from === null
      ? `This chat was resumed from a session record as ${persona}.`
      : `This chat was opened by ${held.from} as ${persona}.`;
  return (
    <Notice
      cause={`persona-grants:${session}`}
      persona={held.persona}
      at="pane"
      label="Persona's hosts held"
      fixes={[
        { label: "Allow", onPress: allow },
        { label: "Keep", onPress: () => setKept(true) },
      ]}
    >
      {how} It can't reach {persona}'s hosts until you allow it.{alsoHere}
    </Notice>
  );
}
