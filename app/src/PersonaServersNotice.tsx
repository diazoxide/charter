import { useCallback, useEffect, useState } from "react";
import { commands, type PersonaServerWaiting, type PlaneId } from "./bindings";
import { Notice } from "./Notice";
import { inTheWindow } from "./personaHostsAllow";

/** Whether this page may approve a persona's server: the window's alone. */
export const mayApprovePersonaServers = (): boolean =>
  inTheWindow(commands, "approvePersonaServer");

/**
 * **A persona's MCP server waits for you** (#1460): one Notice per server of the persona this
 * view shows that takes a value from its vault and that this machine has not approved. A chat
 * as the persona is started without it, because a committed file names what it runs, which a
 * teammate or a chat can change. The Notice shows the line an approval is given to, as
 * `purlis persona approve-mcp` asks it in a terminal, so nobody needs a terminal for it.
 *
 * **Approve** sends back the digest of the line it showed (`approve_persona_server`), and the
 * core records it only if that is still the line: otherwise it is refused, nothing is recorded,
 * and the Notice shows the line as it is now. **Not now** hides it until the view is opened
 * again; nothing is kept. Approve is the window's alone: on a link, whose client has no such
 * call, the Notice says where to approve it.
 */
export function PersonaServersNotice({
  plane,
  persona,
  canApprove = mayApprovePersonaServers(),
}: {
  plane: PlaneId;
  persona: string;
  /** Whether this page may press Approve: the window may, a link may not. */
  canApprove?: boolean;
}) {
  const [waiting, setWaiting] = useState<readonly PersonaServerWaiting[]>([]);
  const [later, setLater] = useState<readonly string[]>([]);
  const [said, setSaid] = useState<Readonly<Record<string, string>>>({});

  const read = useCallback(
    (live: () => boolean) =>
      commands
        .personaServersWaiting(plane, persona)
        .then((answer) => {
          // An answer that is not a list (a core older than this window) is nothing waiting.
          if (live() && answer.status === "ok" && Array.isArray(answer.data))
            setWaiting(answer.data);
        })
        // Servers that cannot be read are said nowhere here: none is started for a chat until
        // it is approved, so saying nothing is the safe way to be wrong.
        .catch(() => {}),
    [plane, persona],
  );

  useEffect(() => {
    let live = true;
    void read(() => live);
    return () => {
      live = false;
    };
  }, [read]);

  const approve = (one: PersonaServerWaiting, shown: string) =>
    void commands
      .approvePersonaServer(plane, persona, one.server, shown)
      .then((answer) => {
        if (answer.status === "ok") {
          setWaiting(answer.data);
          setSaid((now) => ({ ...now, [one.server]: "" }));
          return;
        }
        setSaid((now) => ({ ...now, [one.server]: answer.error }));
        void read(() => true);
      })
      .catch((err: unknown) => setSaid((now) => ({ ...now, [one.server]: String(err) })));

  return (
    <>
      {waiting
        .filter((one) => !later.includes(one.server))
        .map((one) => {
          const notNow = {
            label: "Not now",
            onPress: () => setLater((now) => [...now, one.server]),
          };
          const shown = one.fingerprint;
          return (
            <Notice
              key={one.server}
              cause={`persona-server:${persona}:${one.server}`}
              label={`${persona}'s ${one.server} server waits for you`}
              persona={persona}
              fixes={
                canApprove && shown !== null
                  ? [{ label: "Approve", onPress: () => approve(one, shown) }, notNow]
                  : [notNow]
              }
            >
              <p>
                Chats as {persona} start without the {one.server} server. It takes a value from{" "}
                {persona}'s vault, and the project's files name what it runs, so it waits until you
                approve it on this machine. A change to it asks again.
              </p>
              {shown === null ? (
                <p>
                  purlis cannot show this server's entry in full, so it cannot be approved. Change
                  its entry in personas/{persona}/mcp.json so that it can be shown.
                </p>
              ) : (
                <p>
                  It runs: <code>{one.line}</code>
                </p>
              )}
              {canApprove ? null : <p>Approve it from purlis's own window on this machine.</p>}
              {said[one.server] ? <p>{said[one.server]}</p> : null}
            </Notice>
          );
        })}
    </>
  );
}
