import { useEffect, useRef, useState } from "react";
import { commands, type GrantsHeld, type PlaneId } from "./bindings";
import { Notice } from "./Notice";

/**
 * **A chat that holds another persona's grants instead of its own** (#1362, D-1362-5 and
 * D-1362-6), on its tab. A chat handed off to a persona whose hosts reach past the asking chat's,
 * or resumed from a session record as a persona wider than the default, runs with the narrower
 * grants until the person allows its own here: no chat widens what it reaches on its own say.
 *
 * **Allow** records it (`allow_persona_grants`). A chat's sandbox is compiled as it starts, so
 * the persona's hosts reach it from its next start; **Restart now** starts it again resuming its
 * conversation (`restart_chat`), once its turn has ended, so a turn is never cut off. **Keep**
 * puts the Notice away and changes nothing: it is shown again when the tab opens again.
 */
export function PersonaGrantsNotice({
  plane,
  session,
  running,
  onRestart,
}: {
  plane: PlaneId;
  session: number;
  /** Whether the chat is mid-turn: a restart waits for the turn to end. */
  running: boolean;
  /** Starts the chat again, resuming its conversation, in its pane. */
  onRestart: () => void;
}) {
  const [held, setHeld] = useState<GrantsHeld>();
  const [allowed, setAllowed] = useState(false);
  const [kept, setKept] = useState(false);
  const [restarting, setRestarting] = useState(false);

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

  // The restart the person asked for, once the chat's turn has ended: asked once. The pane
  // then shows the new run, whose hold is gone.
  const fired = useRef(false);
  useEffect(() => {
    if (restarting && !running && !fired.current) {
      fired.current = true;
      onRestart();
    }
  }, [restarting, running, onRestart]);

  if (held === undefined || kept) return null;
  const persona = held.persona ?? "its persona";

  if (allowed)
    return (
      <Notice
        cause={`persona-grants:${session}`}
        at="pane"
        label="Persona's hosts allowed"
        fixes={[{ label: "Restart now", onPress: () => setRestarting(true) }]}
      >
        Allowed. This chat reaches {persona}'s hosts from its next start.
        {restarting && " It restarts, resuming its conversation, when this turn ends."}
      </Notice>
    );

  const allow = () =>
    void commands
      .allowPersonaGrants(plane, session)
      .then((said) => {
        if (said.status === "ok") setAllowed(true);
      })
      .catch(() => {});

  const how =
    held.from === null
      ? `This chat was resumed from a session record as ${persona}.`
      : `This chat was opened by ${held.from} as ${persona}.`;
  return (
    <Notice
      cause={`persona-grants:${session}`}
      at="pane"
      label="Persona's hosts held"
      fixes={[
        { label: "Allow", onPress: allow },
        { label: "Keep", onPress: () => setKept(true) },
      ]}
    >
      {how} It can't reach {persona}'s hosts until you allow it.
    </Notice>
  );
}
