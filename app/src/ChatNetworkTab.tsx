import { useEffect, useState } from "react";
import { commands, type ChatNetwork, type PlaneId, type ReachedBy } from "./bindings";
import { EmptyState } from "./EmptyState";
import { RefusedRows } from "./settings/NetworkLists";
import { useSandboxCommands } from "./sandboxAsked";

/** What each reason a chat reaches a host is called, by the four words (#1662). */
const BY: Readonly<Record<ReachedBy, string>> = {
  open: "Open hosts",
  persona: "Persona hosts",
  allowed: "Allowed hosts",
};

/**
 * **A chat's Network view** (#1662, spec #1661): what chat `session` can reach now, by why it
 * reaches each host (Open hosts, Persona hosts, Allowed hosts), and what its sandbox refused,
 * each with Allow where one may be kept. A chat started without the sandbox says it can reach
 * anything, and nothing more: purlis records nothing of it. Read once as the tab opens, and
 * again when a sandbox command of the window's returns.
 */
export function ChatNetworkTab({ plane, session }: { plane: PlaneId; session?: number }) {
  const [network, setNetwork] = useState<ChatNetwork>();
  const [said, setSaid] = useState<string>();
  const moved = useSandboxCommands();
  useEffect(() => {
    if (session === undefined) return;
    let gone = false;
    void commands
      .chatNetwork(plane, session)
      .then((done) => {
        if (gone) return;
        if (done.status === "error") setSaid(done.error);
        else {
          setSaid(undefined);
          setNetwork(done.data);
        }
      })
      .catch((err: unknown) => !gone && setSaid(String(err)));
    return () => {
      gone = true;
    };
  }, [plane, session, moved]);

  if (said !== undefined)
    return <EmptyState headline="purlis could not read this chat's network" body={said} />;
  if (session === undefined || (network !== undefined && !network.open))
    return (
      <EmptyState headline="This chat is not open" body="Open it again to see what it can reach." />
    );
  if (network === undefined)
    return <p className="chat-network">Reading this chat&apos;s network…</p>;
  if (!network.sandboxed)
    return (
      <div className="chat-network">
        <p>Not sandboxed: can reach anything.</p>
      </div>
    );
  const groups = (Object.keys(BY) as ReachedBy[])
    .map((by) => ({ by, hosts: network.reach.filter((one) => one.by === by) }))
    .filter((group) => group.hosts.length > 0);
  return (
    <div className="chat-network">
      <section aria-label="Can reach now">
        <h3>Can reach now</h3>
        {groups.length === 0 ? (
          <p>No host on the internet.</p>
        ) : (
          groups.map((group) => (
            <div key={group.by} className="sandbox-hosts">
              <span className="sandbox-what">{BY[group.by]}</span>
              <ul aria-label={BY[group.by]}>
                {group.hosts.map((one) => (
                  <li key={one.host}>
                    <code>{one.host}</code>
                  </li>
                ))}
              </ul>
            </div>
          ))
        )}
      </section>
      <section aria-label="Refused">
        <h3>Refused</h3>
        {network.refused.length === 0 ? (
          <p>Nothing was refused in the last 30 days.</p>
        ) : (
          <RefusedRows plane={plane} rows={network.refused} label="Refused" />
        )}
      </section>
    </div>
  );
}
