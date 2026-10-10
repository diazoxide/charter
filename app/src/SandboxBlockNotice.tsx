import { useId, useState } from "react";
import {
  commands,
  type Allowed,
  type BlockReport,
  type GrantLevel,
  type OpenChat,
} from "./bindings";
import { Notice, type NoticeAction } from "./Notice";
import { sandboxCommandReturned } from "./sandboxAsked";
import { asksMoved } from "./asks";
import { hostsOf, type HeldBlock } from "./sandboxBlocks";
import { SETTLE_MS } from "./TaskBlocksNotice";

/**
 * **What a chat's sandbox blocked, on its tab** (#1338): the operation and the kind of path or
 * host, in the core's words, and nothing of what the chat ran.
 *
 * A block of **purlis's own** operation is a purlis bug, and says so. It offers **Report…**,
 * which shows the draft the core makes from the block alone — the operation, the kind and the
 * versions — and files it only on **File report**: by the app, under the operator's own `gh`
 * login, never from inside the chat. Nothing is sent before that press.
 *
 * A block of the chat's own work is **never a dead end** (#1342): a host or a folder to write
 * offers **Allow for this chat**, **Always allow…** (every chat of this project on this machine,
 * or for a host everyone in the project) and **Keep blocked**, with what it would allow shown
 * whole first. The core judges it again and audits it; the window then restarts the chat on its
 * conversation once its turn has ended (`onAllowed`, driven by `PlaneView`), and it is told what
 * was allowed. What is never granted says the way that works; what purlis grants nothing for
 * offers **Start without the sandbox for this chat**, the person's own choice.
 *
 * **Several hosts refused at once are one Notice** (#1637): each host is listed whole, and one
 * Allow allows each of them, every one judged by the core on its own as a single host is, with
 * one restart to take them all. A host that joins the Notice while an answer is on its way is
 * not in that answer: it stays up, asking.
 *
 * **Policy has the last word** (#1343): Allow is offered only at the levels an administrator's
 * policy leaves open (`block.levels`), and where policy forbids every Allow and starting the
 * chat without the sandbox too, nothing is offered — the Notice still says what was blocked,
 * what policy forbids, and who set it, so the person knows whom to ask. Where it removed one
 * choice, or turned asking off, the Notice says so (`block.ruled`, #1666).
 *
 * **A host is asked live** (#1666): where the chat's proxy holds the connection while the
 * person answers (`block.held`), the Notice says it waits, and an Allow lets the same command
 * carry on with nothing restarting (`Allowed.live`). For a host the main button allows it for
 * this project on this machine; **Other scopes…** offers only this chat, or everyone in the
 * project. **Keep blocked** refuses what the proxy holds and tells the chat
 * (`keep_sandbox_block`).
 *
 * **A host allowed already offers no Allow** (#1666's fold-in): it says so, and that this chat
 * takes it once it restarts, with **Restart this chat**.
 */
export function SandboxBlockNotice({
  block,
  more,
  onDismiss,
  onAllowed,
  onRestarted,
}: {
  block: HeldBlock;
  /** How many other blocks this chat holds behind this one. */
  more: number;
  onDismiss: () => void;
  /** Something was allowed: the chat is owed a restart once its turn has ended. */
  onAllowed: () => void;
  /** The chat started again in its place, without the sandbox. */
  onRestarted: (chat: OpenChat) => void;
}) {
  const id = useId();
  /** The Report's draft once it is open, what filing it answered, and any refusal. */
  const [draft, setDraft] = useState<BlockReport>();
  const [filed, setFiled] = useState<string>();
  const [said, setSaid] = useState<string>();
  const [filing, setFiling] = useState(false);
  const cause = `sandbox-blocked:${block.session}:${block.operation}:${block.kind}:${block.ours ? "ours" : "chat"}`;
  const behind =
    more > 0 ? ` ${more} more ${more === 1 ? "block" : "blocks"} behind this one.` : "";

  if (!block.ours)
    return (
      <AllowNotice
        block={block}
        cause={cause}
        behind={behind}
        onDismiss={onDismiss}
        onAllowed={onAllowed}
        onRestarted={onRestarted}
      />
    );

  const report = () => {
    if (draft !== undefined) {
      setDraft(undefined);
      return;
    }
    setSaid(undefined);
    void commands
      .sandboxBlockReport(block.operation, block.kind, block.harness)
      .then((made) => {
        if (made.status === "error") setSaid(made.error);
        else setDraft(made.data);
      })
      .catch((err: unknown) => setSaid(String(err)));
  };
  const file = (shown: BlockReport) => {
    setFiling(true);
    setSaid(undefined);
    void commands
      .fileSandboxBlockReport(block.operation, block.kind, block.harness, shown.digest)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else {
          setFiled(done.data);
          setDraft(undefined);
        }
      })
      .catch((err: unknown) => setSaid(String(err)))
      .finally(() => setFiling(false));
  };

  const under =
    draft === undefined ? undefined : (
      <div className="block-report" id={id}>
        <p>
          Nothing is sent until you press File report. It would be filed on {draft.repository} under
          your own gh login.
        </p>
        <pre className="block-report-draft" aria-label="Report draft">
          {`${draft.title}\n\n${draft.body}`}
        </pre>
        <div className="block-report-actions">
          <button type="button" tabIndex={0} disabled={filing} onClick={() => file(draft)}>
            File report
          </button>
          <button type="button" tabIndex={0} onClick={() => setDraft(undefined)}>
            Cancel
          </button>
        </div>
      </div>
    );

  return (
    <Notice
      cause={cause}
      at="pane"
      tone="trouble"
      label="Sandbox block"
      fixes={
        filed === undefined
          ? [{ label: "Report…", onPress: report, opens: { id, open: draft !== undefined } }]
          : undefined
      }
      onDismiss={onDismiss}
      under={under}
    >
      The sandbox blocked {block.said} that purlis itself ran. That is a purlis bug.
      {filed !== undefined && ` Reported: ${filed}.`}
      {said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}

/** What each scope's button says (#1666, N-3). */
const SCOPE: Readonly<Record<GrantLevel, string>> = {
  you: "Allow for me on this machine",
  chat: "Allow only for this chat",
  project: "Allow for everyone in this project",
};

/** The time now: when a host joined, read once in the render that draws it, or when pressed. */
const joinedAt = () => Date.now();

/**
 * **Allow, Always allow or Keep blocked** for a block of the chat's own work (#1342). Nothing is
 * allowed until a press, and what a press allows is on screen, whole, before it.
 */
function AllowNotice({
  block,
  cause,
  behind,
  onDismiss,
  onAllowed,
  onRestarted,
}: {
  block: HeldBlock;
  cause: string;
  behind: string;
  onDismiss: () => void;
  onAllowed: () => void;
  onRestarted: (chat: OpenChat) => void;
}) {
  const id = useId();
  const [typed, setTyped] = useState(block.target ?? "");
  const [alwaysOpen, setAlways] = useState(false);
  const [allowed, setAllowed] = useState<Allowed>();
  /** The hosts an Allow on this Notice allowed (#1637). */
  const [allowedHosts, setAllowedHosts] = useState<readonly string[]>([]);
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  const what = block.offer === "host" ? "host" : "write";
  const target = block.target ?? typed.trim();
  /** The hosts this Notice lists that no Allow on it has allowed yet, oldest first. */
  const hosts = hostsOf(block);
  const asking = hosts.filter((host) => !allowedHosts.includes(host));
  /**
   * **When the hosts it asks about last changed** (#1637): a host that joins the Notice changes
   * what one Allow grants, so for {@link SETTLE_MS} after, a press does nothing and says why, as
   * the question for several tasks does. Never at its first drawing: nothing joined it yet.
   */
  const listing = asking.join(" ");
  const [joined, setJoined] = useState({ listing, at: 0 });
  if (joined.listing !== listing) {
    const grew = asking.some((host) => !joined.listing.split(" ").includes(host));
    setJoined({ listing, at: grew ? joinedAt() : joined.at });
  }
  /**
   * Whether a press now is too soon after a host joined what it would allow: as this Notice saw
   * it join, or as the window's store held it, so a Notice drawn again keeps the guard.
   */
  const tooSoon = () => {
    if (joinedAt() - Math.max(joined.at, block.joined ?? 0) < SETTLE_MS) {
      setSaid(
        "A host joined this Notice just now, so nothing was allowed. Read the hosts it lists and " +
          "answer again.",
      );
      return true;
    }
    return false;
  };
  /** Whether policy leaves Allow at `level` open (#1343). */
  const allowsAt = (level: GrantLevel) => block.levels.includes(level);
  /**
   * **The main button, and the rest under a menu.** A host: this project on this machine first
   * (#1666, N-3), then only this chat, then everyone in the project. A folder: this chat, then
   * this machine; never the project.
   */
  const order: readonly GrantLevel[] =
    what === "host" ? ["you", "chat", "project"] : ["chat", "you"];
  const open = order.filter(allowsAt);
  const main = open[0];
  const others = open.slice(1);

  const allow = (level: GrantLevel) => {
    if (busy) return;
    if (target === "") return;
    if (tooSoon()) return;
    setBusy(true);
    setSaid(undefined);
    // Each host the Notice lists and has not allowed, or the one thing it names (#1637): what
    // is on screen when pressed, never a host that joins it while the answer is on its way.
    const targets = hosts.length > 0 ? asking : [target];
    void (async () => {
      const done: string[] = [];
      const refused: string[] = [];
      let last: Allowed | undefined;
      /** Whether every host allowed was taken live: then nothing restarts for them (#1666). */
      let live = true;
      for (const one of targets) {
        // The block it showed, so the core answers only that one, and only while the chat is
        // still held on it (#1538).
        const named = targets.length > 1 ? `${one}: ` : "";
        try {
          const answer = await commands.allowSandboxBlock(
            block.plane,
            block.session,
            { operation: block.operation, kind: block.kind, what, target: one },
            level,
          );
          if (answer.status === "error") refused.push(`${named}${answer.error}`);
          else {
            done.push(one);
            last = answer.data;
            live = live && answer.data.live === true;
          }
        } catch (err: unknown) {
          refused.push(`${named}purlis could not allow it: ${String(err)}`);
        }
      }
      if (last !== undefined) {
        setAllowedHosts((was) => [...was, ...done]);
        setAllowed(last);
        setAlways(false);
        // One restart takes every host allowed, unless the chat's proxy took them live.
        if (!live) onAllowed();
      }
      if (refused.length > 0) setSaid(refused.join(" "));
    })().finally(() => {
      setBusy(false);
      // Allowed for every chat, the other chats keep the sandbox they started with: the
      // Notice for chats left behind asks again (#1428).
      sandboxCommandReturned();
      // The Inbox lists the same hosts (#1692): answered here, they go there too.
      asksMoved(block.plane);
    });
  };
  /** Keep blocked (#1666): the core refuses what the proxy holds, and tells the chat. */
  const keepBlocked = () => {
    if (busy) return;
    const targets = hosts.length > 0 ? asking : block.target !== null ? [block.target] : [];
    if (what !== "host" || targets.length === 0) {
      onDismiss();
      return;
    }
    setBusy(true);
    setSaid(undefined);
    void Promise.all(
      targets.map((one) =>
        commands
          .keepSandboxBlock(block.plane, block.session, {
            operation: block.operation,
            kind: block.kind,
            what,
            target: one,
          })
          .then((done) => (done.status === "error" ? done.error : undefined))
          .catch((err: unknown) => `purlis could not keep it blocked: ${String(err)}`),
      ),
    )
      .then((answers) => {
        const refused = answers.filter((one): one is string => one !== undefined);
        // Said, never swallowed: the Notice stays up with why (docs/ui-copy.md).
        if (refused.length > 0) setSaid(refused.join(" "));
        else onDismiss();
      })
      .finally(() => setBusy(false));
  };
  /** Restart this chat, for a host allowed already (#1666's fold-in). */
  const restartThis = () => {
    if (busy) return;
    setBusy(true);
    setSaid(undefined);
    void commands
      .askChatRestart(block.plane, block.session)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else {
          onAllowed();
          onDismiss();
        }
      })
      .catch((err: unknown) => setSaid(`purlis could not restart the chat: ${String(err)}`))
      .finally(() => setBusy(false));
  };
  const withoutSandbox = () => {
    setBusy(true);
    setSaid(undefined);
    void commands
      .restartChatWithoutSandbox(block.plane, block.session, 80, 24)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else onRestarted(done.data);
      })
      .catch((err: unknown) => setSaid(`purlis could not start the chat again: ${String(err)}`))
      .finally(() => setBusy(false));
  };

  if (block.offer === "allowed")
    return (
      <Notice
        cause={cause}
        at="pane"
        tone="trouble"
        label="Sandbox block"
        fixes={[{ label: "Restart this chat", onPress: restartThis, busy }]}
        onDismiss={onDismiss}
      >
        The sandbox blocked {block.said}
        {block.target !== null && (
          <>
            : <code className="block-allow-target">{block.target}</code>
          </>
        )}
        . {block.route}
        {said !== undefined && ` ${said}`}
        {behind}
      </Notice>
    );
  if (block.offer === "brokered")
    return (
      <Notice cause={cause} at="pane" tone="trouble" label="Sandbox block" onDismiss={onDismiss}>
        The sandbox blocked {block.said}. purlis never allows that to a chat. {block.route}
        {behind}
      </Notice>
    );
  if (block.offer === "policy")
    return (
      <Notice cause={cause} at="pane" tone="trouble" label="Sandbox block" onDismiss={onDismiss}>
        The sandbox blocked {block.said}
        {block.target !== null && (
          <>
            {block.kind === "host" ? ": " : " in "}
            <code className="block-allow-target">{block.target}</code>
          </>
        )}
        . {block.route}
        {behind}
      </Notice>
    );
  if (block.offer === "unsandboxed")
    return (
      <Notice
        cause={cause}
        at="pane"
        tone="trouble"
        label="Sandbox block"
        fixes={[
          {
            label: "Start without the sandbox for this chat",
            onPress: () => {
              if (!busy) withoutSandbox();
            },
          },
        ]}
        onDismiss={onDismiss}
      >
        The sandbox blocked {block.said}
        {block.target !== null && (
          <>
            {block.kind === "host" ? ": " : " in "}
            <code className="block-allow-target">{block.target}</code>
          </>
        )}
        . {block.route} Only you can choose to start this chat again without the sandbox: it
        restarts now, even mid-turn, on the same conversation, and nothing it runs is confined until
        it next starts. Anything allowed for it that it has not yet taken is dropped.
        {said !== undefined && ` ${said}`}
        {behind}
      </Notice>
    );
  if (block.offer !== "host" && block.offer !== "write")
    return (
      <Notice cause={cause} at="pane" tone="trouble" label="Sandbox block" onDismiss={onDismiss}>
        The sandbox blocked {block.said}.{behind}
      </Notice>
    );
  /** The hosts said whole, each apart from purlis's sentence. */
  const drawn = (of: readonly string[]) =>
    of.map((host, at) => (
      <span key={host}>
        {at > 0 && (at === of.length - 1 ? " and " : ", ")}
        <code className="block-allow-target">{host}</code>
      </span>
    ));
  if (allowed !== undefined && asking.length === 0)
    return (
      <Notice
        cause={`${cause}:allowed`}
        at="pane"
        tone="news"
        label="Sandbox block"
        onDismiss={onDismiss}
      >
        {hosts.length > 0 ? (
          <>{drawn(allowedHosts)}: </>
        ) : (
          block.target !== null && (
            <>
              <code className="block-allow-target">{block.target}</code>:{" "}
            </>
          )
        )}
        {allowed.said}
      </Notice>
    );

  const named =
    block.target === null ? (
      <label className="block-allow-host">
        Host{" "}
        <input
          value={typed}
          onChange={(event) => setTyped(event.target.value)}
          placeholder="api.example.com"
          aria-label="Host to allow"
        />
      </label>
    ) : hosts.length > 0 ? (
      drawn(asking)
    ) : (
      <code className="block-allow-target">{block.target}</code>
    );
  const under = (
    <div className="block-allow" id={id}>
      <p>
        {what === "host" ? (
          <>Allow reaching {named}</>
        ) : (
          <>Allow writing {named} and everything in it</>
        )}
      </p>
      {alwaysOpen && (
        <div className="block-allow-actions">
          {others.map((level) => (
            <button
              key={level}
              type="button"
              tabIndex={0}
              disabled={busy || target === ""}
              onClick={() => allow(level)}
            >
              {SCOPE[level]}
            </button>
          ))}
        </div>
      )}
    </div>
  );
  // Only the levels policy leaves open (#1343); Keep blocked always.
  const keep: NoticeAction = { label: "Keep blocked", onPress: keepBlocked };
  const allows: NoticeAction[] = [
    ...(main !== undefined ? [{ label: SCOPE[main], onPress: () => allow(main) }] : []),
    ...(others.length > 0
      ? [
          {
            label: what === "host" ? "Other scopes…" : "Always allow…",
            onPress: () => setAlways((was) => !was),
            opens: { id, open: alwaysOpen },
          },
        ]
      : []),
  ];
  const [first, ...rest] = [...allows, keep];
  const fixes: readonly [NoticeAction, ...NoticeAction[]] = [first ?? keep, ...rest];
  return (
    <Notice
      cause={cause}
      at="pane"
      tone="trouble"
      label="Sandbox block"
      fixes={fixes}
      under={under}
    >
      {block.held ? (
        <>
          The chat's command is waiting on {block.said}: purlis holds the connection while you
          answer, and an Allow lets the same command carry on. If nobody answers within a minute it
          is refused, and an Allow after that tells the chat to run it again.
        </>
      ) : (
        <>The sandbox blocked {block.said}.</>
      )}
      {asking.length > 1 &&
        (block.held
          ? ` ${asking.length} hosts are asked about. One Allow allows each of them.`
          : ` ${asking.length} hosts were refused. One Allow allows each of them, and the chat restarts once.`)}
      {block.ruled !== null && block.ruled !== undefined && ` ${block.ruled}`}
      {allowedHosts.length > 0 && <> Allowed already: {drawn(allowedHosts)}.</>}
      {said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}
