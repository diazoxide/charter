import { useId, useState } from "react";
import {
  commands,
  type Allowed,
  type BlockReport,
  type ChatBlocked,
  type GrantLevel,
  type OpenChat,
} from "./bindings";
import { Notice, type NoticeAction } from "./Notice";
import { sandboxCommandReturned } from "./sandboxAsked";

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
 * **Policy has the last word** (#1343): Allow is offered only at the levels an administrator's
 * policy leaves open (`block.levels`), and where policy forbids every Allow and starting the
 * chat without the sandbox too, nothing is offered — the Notice still says what was blocked,
 * what policy forbids, and who set it, so the person knows whom to ask.
 */
export function SandboxBlockNotice({
  block,
  more,
  onDismiss,
  onAllowed,
  onRestarted,
}: {
  block: ChatBlocked;
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
  block: ChatBlocked;
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
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  const what = block.offer === "host" ? "host" : "write";
  const target = block.target ?? typed.trim();
  /** Whether policy leaves Allow at `level` open (#1343). */
  const allowsAt = (level: GrantLevel) => block.levels.includes(level);
  const always = allowsAt("you") || (what === "host" && allowsAt("project"));

  const allow = (level: GrantLevel) => {
    setBusy(true);
    setSaid(undefined);
    void commands
      .allowSandboxBlock(block.plane, block.session, what, target, level)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else {
          setAllowed(done.data);
          setAlways(false);
          onAllowed();
        }
      })
      .catch((err: unknown) => setSaid(`purlis could not allow it: ${String(err)}`))
      .finally(() => {
        setBusy(false);
        // Allowed for every chat, the other chats keep the sandbox they started with: the
        // Notice for chats left behind asks again (#1428).
        sandboxCommandReturned();
      });
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
            {" "}
            in <code className="block-allow-target">{block.target}</code>
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
            {" "}
            in <code className="block-allow-target">{block.target}</code>
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
  if (allowed !== undefined)
    return (
      <Notice
        cause={`${cause}:allowed`}
        at="pane"
        tone="news"
        label="Sandbox block"
        onDismiss={onDismiss}
      >
        {block.target !== null && (
          <>
            <code className="block-allow-target">{block.target}</code>:{" "}
          </>
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
          {allowsAt("you") && (
            <button
              type="button"
              tabIndex={0}
              disabled={busy || target === ""}
              onClick={() => allow("you")}
            >
              Allow for me on this machine
            </button>
          )}
          {what === "host" && allowsAt("project") && (
            <button
              type="button"
              tabIndex={0}
              disabled={busy || target === ""}
              onClick={() => allow("project")}
            >
              Allow for everyone in this project
            </button>
          )}
        </div>
      )}
    </div>
  );
  // Only the levels policy leaves open (#1343); Keep blocked always.
  const keep: NoticeAction = { label: "Keep blocked", onPress: onDismiss };
  const allows: NoticeAction[] = [
    ...(allowsAt("chat")
      ? [
          {
            label: "Allow for this chat",
            onPress: () => {
              if (!busy && target !== "") allow("chat");
            },
          },
        ]
      : []),
    ...(always
      ? [
          {
            label: "Always allow…",
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
      The sandbox blocked {block.said}.{said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}
