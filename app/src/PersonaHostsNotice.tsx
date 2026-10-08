import { useEffect, useState } from "react";
import { commands, type PersonaHosts, type PlaneId, type SandboxState } from "./bindings";
import { Notice } from "./Notice";

/** "a", "a and b", "a, b and c". */
function listed(hosts: readonly string[]): string {
  if (hosts.length < 2) return hosts.join("");
  return `${hosts.slice(0, -1).join(", ")} and ${hosts[hosts.length - 1]}`;
}

/** The personas whose hosts this machine has not allowed, and a chat would reach. */
function waiting(state: SandboxState | null | undefined): PersonaHosts[] {
  if (!state || state.policy?.persona_hosts === true) return [];
  return (state.persona_hosts ?? []).filter((one) => !one.allowed && one.reached.length > 0);
}

/**
 * **A persona's hosts wait for you** (#1362, D-1362-7): one Notice per persona whose hosts the
 * project's committed settings list and this machine has not allowed as they stand. A teammate
 * can change that file, and these hosts reach past the presets (a cluster over a VPN, say), so
 * a chat as that persona reaches none of them here until you press **Allow**.
 *
 * Allow sends back the digest of the list the Notice showed (`allow_persona_hosts`), and the
 * core keeps it only if the list still is that one: a list that changed in between is refused,
 * and the Notice shows the list as it is now. A change after an Allow asks again. **Not now**
 * hides it until the project is opened again; nothing is kept. Allow is the window's alone: no
 * chat and no link can press it.
 */
export function PersonaHostsNotice({ plane }: { plane: PlaneId }) {
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
    void commands
      .allowPersonaHosts(plane, one.persona, one.digest)
      .then((answer) => {
        if (answer.status === "ok") {
          setState(answer.data);
          setSaid((now) => ({ ...now, [one.persona]: "" }));
        } else {
          setSaid((now) => ({ ...now, [one.persona]: answer.error }));
          void commands.sandboxState(plane).then((again) => {
            if (again.status === "ok") setState(again.data);
          });
        }
      })
      .catch((err: unknown) => setSaid((now) => ({ ...now, [one.persona]: String(err) })));

  return (
    <>
      {waiting(state)
        .filter((one) => !later.includes(one.persona))
        .map((one) => (
          <Notice
            key={one.persona}
            cause={`sandbox-hosts:persona:${one.persona}`}
            label={`${one.persona}'s hosts wait for you`}
            persona={one.persona}
            fixes={[
              { label: `Allow for ${one.persona} chats`, onPress: () => allow(one) },
              {
                label: "Not now",
                onPress: () => setLater((now) => [...now, one.persona]),
              },
            ]}
          >
            <p>
              The project's settings let chats as {one.persona} reach {listed(one.reached)}. Anyone
              who can change those settings can change this list, so no chat here reaches these
              hosts until you allow them on this machine. A change to the list asks again.
              {said[one.persona] ? <> {said[one.persona]}</> : null}
            </p>
          </Notice>
        ))}
    </>
  );
}
