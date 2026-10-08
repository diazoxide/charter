import { useEffect, useState } from "react";
import { commands, type PersonaHosts, type PlaneId, type SandboxState } from "./bindings";
import { Notice } from "./Notice";
import {
  allowLabel,
  allowPersonaHosts,
  FROM_NEXT_START,
  listedHosts,
  mayAllowPersonaHosts,
  whoReaches,
} from "./personaHostsAllow";

/** The personas whose hosts this machine has not allowed as they stand, and a chat would reach. */
export function waitingPersonas(state: SandboxState | null | undefined): PersonaHosts[] {
  if (!state || state.policy?.persona_hosts === true) return [];
  return (state.persona_hosts ?? []).filter((one) => !one.allowed && one.reached.length > 0);
}

/**
 * **A persona's hosts wait for you** (#1362, D-1362-7): one Notice per persona whose hosts the
 * project's committed settings list and this machine has not allowed as they stand. A teammate
 * can change that file, and these hosts reach past the presets (a cluster over a VPN, say), so
 * a chat as that persona reaches none of them here until you press **Allow**. For the project's
 * default persona the Allow is also for every chat that names no persona, and says so
 * (D-1362-12).
 *
 * Allow sends back the digest of what the Notice showed (`allow_persona_hosts`), and the core
 * keeps it only if that is still what stands: otherwise it is refused, and the Notice shows it
 * as it is now. An Allow whose list changed since is said as such, and asks anew (D-1362-13).
 * **Not now** hides it until the project is opened again; nothing is kept. Allow is the
 * window's alone: on a link, whose client has no such call, the Notice says where to allow it.
 */
export function PersonaHostsNotice({
  plane,
  canAllow = mayAllowPersonaHosts(),
}: {
  plane: PlaneId;
  /** Whether this page may press Allow: the window may, a link may not. */
  canAllow?: boolean;
}) {
  const [state, setState] = useState<SandboxState>();
  const [later, setLater] = useState<readonly string[]>([]);
  const [said, setSaid] = useState<Readonly<Record<string, string>>>({});

  useEffect(() => {
    let live = true;
    void commands
      .sandboxState(plane)
      .then((answer) => {
        if (live && answer.status === "ok") setState(answer.data);
      })
      // A project whose state cannot be read asks nothing here: no persona's host reaches a
      // chat until it is allowed, so saying nothing is the safe way to be wrong.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [plane]);

  const allow = (one: PersonaHosts) =>
    void allowPersonaHosts(plane, one).then((answer) => {
      if ("state" in answer) {
        setState(answer.state);
        setSaid((now) => ({ ...now, [one.persona]: "" }));
        return;
      }
      setSaid((now) => ({ ...now, [one.persona]: answer.refused }));
      void commands
        .sandboxState(plane)
        .then((again) => {
          if (again.status === "ok") setState(again.data);
        })
        .catch(() => {});
    });

  return (
    <>
      {waitingPersonas(state)
        .filter((one) => !later.includes(one.persona))
        .map((one) => {
          const notNow = {
            label: "Not now",
            onPress: () => setLater((now) => [...now, one.persona]),
          };
          return (
            <Notice
              key={one.persona}
              cause={`sandbox-hosts:persona:${one.persona}`}
              label={`${one.persona}'s hosts wait for you`}
              persona={one.persona}
              fixes={
                canAllow
                  ? [{ label: allowLabel(one), onPress: () => allow(one) }, notNow]
                  : [notNow]
              }
            >
              <p>
                {one.waiting
                  ? `You allowed ${one.persona}'s hosts on this machine, and they have changed since, so no chat reaches any of them until you allow them again. `
                  : ""}
                The project's settings let {whoReaches(one)} reach {listedHosts(one.reached)}.
                Anyone who can change those settings can change this, so no chat here reaches these
                hosts until you allow them on this machine. A change asks again. {FROM_NEXT_START}
                {canAllow ? "" : " Allow them from purlis's own window on this machine."}
                {said[one.persona] ? <> {said[one.persona]}</> : null}
              </p>
            </Notice>
          );
        })}
    </>
  );
}
