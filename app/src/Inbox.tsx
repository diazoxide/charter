import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactElement } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { useTabStop } from "./roving";
import { commands, type Offered, type PlaneId, type Shown } from "./bindings";
import { answerThrough, asksMoved } from "./asks";
import { DispatchGrantNotice } from "./DispatchGrantNotice";
import {
  askKey,
  byChat,
  noteAnswered,
  noteSeen,
  replyBytes,
  seenOrder,
  useAnswered,
  type Answered,
} from "./inboxRules";

/** What an empty Inbox says (I-12). */
export const NOTHING_WAITS = "Nothing is waiting on you";

/** What an Inbox says before its project's asks were read. */
export const NOT_READ_YET = "Reading what waits on you…";

/** What a chain is drawn as: the session first, the chat that asked last (I-9). */
export const chainSaid = (chain: readonly string[]) => chain.join(" › ");

/** The time an answer was given, as the list says it. */
const timeSaid = (at: number) =>
  new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/**
 * **The Inbox** (#1692, spec #1688): everything in this project that waits on the person, at the
 * top of the right side's activity bar, the "for you" side (ADR 0038 as amended 2026-10-10).
 *
 * **One list, from the asks registry** (#1690): grouped by the chat that asks, the chat that has
 * waited longest first, each group under its chain (`steward 12 › #3046 drill › log watch`, I-9).
 * An ask is gone from here the moment its source stops waiting, wherever it was answered (I-8).
 *
 * **Answered in place, through the ask's own path** (I-3): the registry names the command its
 * source's Notice answers with, and a button here sends exactly that (`asks.answerThrough`).
 * Nothing new is opened, and an answer here clears its Notice on the chat's pane, because both
 * are drawn from the same source:
 *
 * - **a permission** prompt and a choice of several: each option the harness offered is a button;
 * - **a sandbox host**: Allow at each level policy leaves open, and Keep blocked; an Allow owes
 *   the chat its restart, as the block Notice's does (`onAnswered`);
 * - **a dispatch grant** is its own Notice, drawn here whole (`DispatchGrantNotice`): the brief,
 *   what the target works with and the boxes its digest is bound to are read before any Allow,
 *   and it reads the very list the pane's copy reads, so answering one clears the other;
 * - **a question** a chat's turn ended on gets a reply box, sent as the person's message;
 * - **a prompt in a harness's own terminal** says so, and is answered there.
 *
 * Every ask has **Go to chat**, which for a task opens it in its session's tab (`onGo`).
 *
 * **The keyboard** (I-11): ↑ and ↓ move between the asks and their buttons, Enter presses the
 * focused button, as it does anywhere, and Escape leaves for the chat in front. **No key of a
 * single letter does anything**, so a stray keystroke never answers an ask. The keyboard comes
 * in on the first ask itself, never on a button.
 *
 * **What an ask says is data** (I-1, #1690): a chain a chat named, a command line, a host are
 * drawn as text, and the only controls are the fixed words of the answers it offers.
 *
 * **Empty, it says so** (I-12) — "Nothing is waiting on you" — and lists what was answered here
 * lately, read-only: when, which chat, what it asked and the answer.
 */
export function Inbox({
  plane,
  asks,
  onGo,
  onLeave,
  onAnswered,
  onIgnore,
  onShowList,
  whyOf,
}: {
  plane: PlaneId;
  /** The project's asks, as the registry derived them last; nothing before the first read. */
  asks: readonly Shown[] | undefined;
  /** The chat that asked, in front: a task in its session's tab. */
  onGo: (session: number) => void;
  /** Escape: the keyboard goes back to the chat in front. */
  onLeave: () => void;
  /** An answer from here applied: what the source's Notice does after it is done here too. */
  onAnswered?: (ask: Shown, option: Offered) => void;
  /**
   * The queue's own Ignore for a chat waiting on a reply or in its terminal
   * (`needs.ignore:<session>`): put away until it asks again. Never offered on a decision.
   */
  onIgnore?: (session: number) => void;
  /**
   * Opens the title bar's list, where it still holds what is no ask yet: a dispatch refused
   * while nobody was there, a chat whose Smart close stopped (#1693 and #1695 move them here).
   * Absent while it holds nothing more.
   */
  onShowList?: () => void;
  /**
   * Why a chat in the queue waits, where it is not that it asked anything (#1448, SI-8f): a
   * task that failed, a report with nowhere to go, a Smart close that stopped. Said in place of
   * "Waiting on your reply", and such a chat gets no reply box, since nothing it asked is
   * answered by typing (#1700, until #1693 makes them updates).
   */
  whyOf?: (session: number) => string | undefined;
}) {
  if (asks !== undefined) noteSeen(plane, asks);
  const groups = asks === undefined ? [] : byChat(asks, (ask) => seenOrder(plane, ask));
  const recent = useAnswered(plane);
  /** The asks a way out of a Notice drawn here was pressed on, with the way out's words:
   *  answered here once they go. */
  const touched = useRef(new Map<string, { ask: Shown; answer: string }>());
  const listed = new Set((asks ?? []).map(askKey));
  useEffect(() => {
    for (const [key, { ask, answer }] of touched.current) {
      if (listed.has(key)) continue;
      touched.current.delete(key);
      noteAnswered(plane, { at: Date.now(), chain: ask.chain, says: ask.says, answer });
    }
  });
  /** Each chat's rows: a chat's held dispatches are one Notice, which asks about the first
   *  and says how many wait behind it. */
  const rows = groups.map((group) => ({
    ...group,
    asks: group.asks.filter(
      (ask, at) =>
        ask.source !== "dispatch" ||
        group.asks.findIndex((one) => one.source === "dispatch") === at,
    ),
  }));
  const shapes = new Map(
    rows.flatMap((group) => group.asks).map((ask) => [askKey(ask), shapeOf(ask, whyOf, onIgnore)]),
  );
  // **One Tab stop for the list** (`docs/ui-primitives.md`, ADR 0037): the window's roving
  // focus, so ↑ and ↓, Home and End move through the asks and their buttons in order.
  const stops = [
    ...rows
      .flatMap((group) => group.asks)
      .flatMap((ask) => {
        const shape = shapes.get(askKey(ask)) ?? shapeOf(ask, whyOf, onIgnore);
        return stopIds(askKey(ask), shape, shape.answers ? ask.options : []);
      }),
    ...(onShowList === undefined ? [] : [LIST_STOP]),
  ];
  const stop = useTabStop(undefined, stops);

  const leave = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    onLeave();
  };

  return (
    <section className="inbox" data-view="inbox" aria-label="Inbox">
      <RovingFocusGroup.Root asChild orientation="vertical" loop={false} {...stop}>
        <div className="inbox-list" onKeyDown={leave}>
          {asks === undefined ? (
            <p className="inbox-none">{NOT_READ_YET}</p>
          ) : rows.length === 0 ? (
            <Empty recent={recent} />
          ) : (
            rows.map((group) => (
              <section
                key={group.session}
                className="inbox-chat"
                aria-label={chainSaid(group.chain)}
                data-session={group.session}
              >
                <h3 className="inbox-chain">{chainSaid(group.chain)}</h3>
                <ul>
                  {group.asks.map((ask) => (
                    <Ask
                      key={askKey(ask)}
                      plane={plane}
                      ask={ask}
                      shape={shapes.get(askKey(ask)) ?? shapeOf(ask, whyOf, onIgnore)}
                      onGo={() => onGo(ask.session)}
                      onAnswered={onAnswered}
                      onIgnore={onIgnore}
                      onTouched={(answer) => touched.current.set(askKey(ask), { ask, answer })}
                    />
                  ))}
                </ul>
              </section>
            ))
          )}
          {onShowList !== undefined && (
            <p className="inbox-none">
              More waits in the title bar's list.{" "}
              <Stop id={LIST_STOP}>
                <button type="button" className="inbox-list-link" onClick={onShowList}>
                  Show the list
                </button>
              </Stop>
            </p>
          )}
        </div>
      </RovingFocusGroup.Root>
    </section>
  );
}

/** The list's link to the title bar's list, as a stop. */
const LIST_STOP = "inbox:list";

/** What one ask's row offers, decided once for its stops and its drawing. */
type Shape = {
  /** Why the chat waits, said in place of what it asked. */
  why?: string;
  /** Its answers are buttons here. */
  answers: boolean;
  /** It gets a reply box. */
  reply: boolean;
  /** It has the queue's Ignore. */
  ignore: boolean;
};

function shapeOf(
  ask: Shown,
  whyOf: ((session: number) => string | undefined) | undefined,
  onIgnore: ((session: number) => void) | undefined,
): Shape {
  const waits = ask.source === "question" || ask.source === "terminal";
  const why = ask.source === "question" ? whyOf?.(ask.session) : undefined;
  return {
    why,
    answers:
      ask.source !== "dispatch" && ask.options.length > 0 && ask.answer.via !== "in-its-pane",
    reply: ask.source === "question" && why === undefined,
    // A chat that only waits for the person's next word asks again at its next stop, so it
    // may be put away, as the queue always let it be. A decision never is.
    ignore: waits && onIgnore !== undefined,
  };
}

/** The stops of one ask's row, in the order it draws them. */
function stopIds(key: string, shape: Shape, options: readonly Offered[] = []): string[] {
  return [
    key,
    ...options.map((option) => `${key}:${option.id}`),
    ...(shape.reply ? [`${key}:reply`, `${key}:send`] : []),
    `${key}:go`,
    ...(shape.ignore ? [`${key}:ignore`] : []),
  ];
}

/** One stop of the list's roving focus. */
function Stop({
  id,
  focusable = true,
  children,
}: {
  id: string;
  focusable?: boolean;
  children: ReactElement;
}) {
  return (
    <RovingFocusGroup.Item asChild tabStopId={id} focusable={focusable}>
      {children}
    </RovingFocusGroup.Item>
  );
}

/** What a row says of where it waits. */
const SOURCE_SAID: Record<Shown["source"], string> = {
  permission: "Asks your permission",
  dispatch: "Asks to dispatch",
  "sandbox-host": "Sandbox",
  terminal: "In its terminal",
  question: "Waiting on your reply",
};

function Ask({
  plane,
  ask,
  shape,
  onGo,
  onAnswered,
  onIgnore,
  onTouched,
}: {
  plane: PlaneId;
  ask: Shown;
  shape: Shape;
  onGo: () => void;
  onAnswered?: (ask: Shown, option: Offered) => void;
  onIgnore?: (session: number) => void;
  /** A way out of the Notice drawn in the row was pressed, with its words. */
  onTouched: (answer: string) => void;
}) {
  const id = useId();
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  const who = chainSaid(ask.chain);
  const key = askKey(ask);

  const answer = (option: Offered) => {
    if (busy) return;
    setBusy(true);
    setSaid(undefined);
    void answerThrough(plane, ask, option.id)
      .then((refused) => {
        if (refused !== undefined) {
          setSaid(refused);
          return;
        }
        noteAnswered(plane, {
          at: Date.now(),
          chain: ask.chain,
          says: ask.says,
          answer: option.label,
        });
        onAnswered?.(ask, option);
      })
      .finally(() => {
        setBusy(false);
        // Whatever it answered, the list is read again: a refusal may mean it went elsewhere.
        asksMoved(plane);
      });
  };

  const body =
    ask.source === "dispatch" ? (
      // The dispatch's own Notice, whole: what an Allow is bound to is read before it. Only a
      // press of one of its ways out counts as answering it here, never a tick or a scroll.
      <div
        className="inbox-notice"
        onClickCapture={(event) => {
          const way = (event.target as Element).closest("button.notice-fix");
          if (way?.textContent) onTouched(way.textContent);
        }}
      >
        <DispatchGrantNotice plane={plane} session={ask.session} />
      </div>
    ) : shape.reply ? (
      <Reply plane={plane} ask={ask} stop={key} />
    ) : shape.answers ? (
      <div className="inbox-answers" role="group" aria-label={`Answers to ${who}`}>
        {ask.options.map((option) => (
          <Stop key={option.id} id={`${key}:${option.id}`} focusable={!busy}>
            <button
              type="button"
              className={option.allows ? "inbox-answer allows" : "inbox-answer"}
              disabled={busy}
              aria-busy={busy || undefined}
              onClick={() => answer(option)}
            >
              {option.label}
            </button>
          </Stop>
        ))}
      </div>
    ) : null;

  return (
    <Stop id={key}>
      <li
        className="inbox-ask"
        data-source={ask.source}
        aria-labelledby={`${id}-says`}
        aria-describedby={`${id}-whose`}
      >
        <p className="inbox-says">
          <span id={`${id}-whose`} className="inbox-source">
            {SOURCE_SAID[ask.source]}
          </span>
          {/* The ask's words are the source's, drawn as text and never as a control. */}
          <span id={`${id}-says`} className="ask-says">
            {shape.why ?? ask.says}
          </span>
        </p>
        {body}
        {said !== undefined && (
          <p className="inbox-refused" role="status">
            {said}
          </p>
        )}
        <Stop id={`${key}:go`}>
          <button
            type="button"
            className="inbox-go"
            aria-label={`Go to chat ${who}`}
            onClick={onGo}
          >
            Go to chat
          </button>
        </Stop>
        {shape.ignore && onIgnore !== undefined && (
          <Stop id={`${key}:ignore`}>
            <button
              type="button"
              className="inbox-go"
              aria-label={`Ignore ${who} until it asks again`}
              title={`Ignore ${who} until it asks again`}
              onClick={() => onIgnore(ask.session)}
            >
              Ignore
            </button>
          </Stop>
        )}
      </li>
    </Stop>
  );
}

/**
 * **A reply box for a chat waiting on the person's reply** (I-3): what is typed is sent to the
 * chat as the person's message, one paste and Enter (`replyBytes`), through the pane's own
 * input path. Short replies only; anything longer is better written in the chat itself.
 */
function Reply({ plane, ask, stop }: { plane: PlaneId; ask: Shown; stop: string }) {
  const [text, setText] = useState("");
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  const who = chainSaid(ask.chain);
  const send = () => {
    const bytes = replyBytes(text);
    if (bytes === undefined || busy) return;
    setBusy(true);
    setSaid(undefined);
    void commands
      .sendInput(plane, ask.session, bytes)
      .then((done) => {
        if (done.status === "error") {
          setSaid(done.error);
          return;
        }
        noteAnswered(plane, {
          at: Date.now(),
          chain: ask.chain,
          says: ask.says,
          answer: text.trim(),
        });
        setText("");
        asksMoved(plane);
      })
      .catch((err: unknown) => setSaid(`purlis could not send it: ${String(err)}`))
      .finally(() => setBusy(false));
  };
  return (
    <form
      className="inbox-reply"
      onSubmit={(event) => {
        event.preventDefault();
        send();
      }}
    >
      {/* A stop of the list too, so ↑ and ↓ reach it; Home and End there are the list's. */}
      <Stop id={`${stop}:reply`}>
        <input
          type="text"
          aria-label={`Reply to ${who}`}
          value={text}
          placeholder="Your reply"
          autoComplete="off"
          spellCheck
          onChange={(event) => setText(event.target.value)}
        />
      </Stop>
      <Stop id={`${stop}:send`} focusable={!busy && replyBytes(text) !== undefined}>
        <button type="submit" disabled={busy || replyBytes(text) === undefined}>
          Send reply
        </button>
      </Stop>
      {said !== undefined && (
        <p className="inbox-refused" role="status">
          {said}
        </p>
      )}
    </form>
  );
}

/** An empty Inbox: the sentence, and what was answered here lately, read-only (I-12). */
function Empty({ recent }: { recent: readonly Answered[] }) {
  return (
    <>
      <p className="inbox-none">{NOTHING_WAITS}</p>
      {recent.length > 0 && (
        <section className="inbox-recent" aria-label="Recently answered">
          <h3 className="inbox-chain">Recently answered</h3>
          <ul>
            {recent.map((one) => (
              <li key={`${one.at}:${chainSaid(one.chain)}:${one.says}`}>
                <time dateTime={new Date(one.at).toISOString()}>{timeSaid(one.at)}</time>{" "}
                <span className="inbox-recent-chat">{chainSaid(one.chain)}</span>
                {": "}
                <span className="ask-says">{one.says}</span>
                {" · "}
                <span className="inbox-recent-answer">{one.answer}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}

/**
 * **Where a click on an ask's notification lands** (#1694, I-7): chat `session`'s group in the
 * Inbox drawn now, brought on screen, its first ask given the keyboard, from which the keys go
 * on as from any ask. Answers whether the chat has a group to land on: its asks may have been
 * answered elsewhere since.
 */
export function landOnGroup(session: number): boolean {
  const group = document.querySelector<HTMLElement>(
    `section.inbox .inbox-chat[data-session="${session}"]`,
  );
  const first = group?.querySelector<HTMLElement>("li");
  if (!group || !first) return false;
  group.scrollIntoView?.({ block: "nearest" });
  first.focus();
  return true;
}
