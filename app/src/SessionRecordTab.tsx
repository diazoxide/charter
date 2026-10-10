import { useEffect, useMemo, useState } from "react";
import Markdown, { type Components } from "react-markdown";
import { LoaderCircle } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { Notice } from "./Notice";
import { ExternalLink } from "./ReleaseNotes";
import { Facts } from "./Views";
import { NO_PERSONA_SAID } from "./dispatches";
import { commands, type PlaneId, type SessionRecordView } from "./bindings";
import { jumpTo } from "./fileJump";
import { codeSpans, recordRef, type RecordRef } from "./recordRefs";
import { ToYourEditor } from "./editor/ToYourEditor";

/**
 * **One session record, in a tab of its own** (SI-8d): what a chat wrote of its session when it
 * closed through Smart close — read-only, rendered as the Markdown it is, and selectable, because
 * it is inside the view's `.view-body` (SI-7's opt-in).
 *
 * charter's facts about the record come first, as the persona view's are drawn (`Facts`): when,
 * where, as whom, on what, and whether it holds a conversation Resume can give back. Then the
 * record itself. The heading above it is the view's (`Views.ViewPane`), and carries **Resume** —
 * the catalogue's `session.resume:<path>` row, so the heading, the panel's menu and the palette
 * are one verb.
 *
 * **`react-markdown`, and no HTML**, for `ReleaseNotes`' reason: a record is a file a chat wrote
 * and a hand or a pull can change, so nothing in it reaches `innerHTML` and raw HTML is dropped
 * (`skipHtml`). A link opens in the browser only when it is http or https.
 *
 * **A file the record names opens** (RC-5a #984, RC-20a #1043): an inline code span that names a
 * file of the record's workspace (`recordRefs.ts`, D-984-3) is a button that jumps to it in the
 * light editor at its line (`fileJump.ts`), and the files named are listed after the record,
 * each with *Open in your editor* at that line. A plane-root record names none.
 */
export function SessionRecordTab({ plane, path }: { plane: PlaneId; path: string }) {
  const [said, setSaid] = useState<{ record?: SessionRecordView | null; trouble?: string }>();
  /** Bumped by Read again: the tab reads once when it opens (NO-8's follow-up, #1296). */
  const [again, setAgain] = useState(0);
  useEffect(() => {
    let gone = false;
    void commands
      .sessionRecord(plane, path)
      .then((answer) => {
        if (gone) return;
        setSaid(
          answer.status === "error" ? { trouble: answer.error } : { record: answer.data ?? null },
        );
      })
      .catch((err: unknown) => {
        if (!gone) setSaid({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [plane, path, again]);
  const repos = useRepos(plane, said?.record?.place);

  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the record…
      </p>
    );
  }
  if (said.trouble !== undefined) {
    return (
      <Notice
        cause={`session-record-unread:${path}`}
        tone="trouble"
        fixes={[
          {
            label: "Read again",
            onPress: () => {
              setSaid(undefined);
              setAgain((was) => was + 1);
            },
          },
        ]}
      >
        {said.trouble}
      </Notice>
    );
  }
  const record = said.record;
  if (record === null || record === undefined) {
    // **Not an error**: a tab the last launch left open can name a record since deleted.
    return (
      <EmptyState
        headline="This session record is not here any more"
        body={`Nothing is at ${path} now.`}
        testid="view-gone"
      />
    );
  }
  const { row } = record;
  return (
    <>
      <Facts
        facts={{
          kind: "facts",
          facts: [
            { label: "Written", value: row.when },
            { label: "Where", value: record.place },
            { label: "Persona", value: row.persona ?? "none" },
            {
              label: "Persona's hosts",
              value:
                record.persona_hosts.length === 0
                  ? "none of its own"
                  : record.persona_hosts_locked !== null
                    ? `${record.persona_hosts.join(", ")}; no chat as this persona reaches them. ${record.persona_hosts_locked}`
                    : record.persona_hosts_wait
                      ? // #1362, D-1362-7: no chat as this persona reaches them on this machine yet.
                        `${record.persona_hosts.join(", ")}; no chat as this persona reaches them until you allow them on this machine, on the project's notice or in Settings › Sandbox${record.resume_holds ? ", and a Resume then holds them back until you also allow them on the new chat's tab" : ""}`
                      : record.resume_holds
                        ? `${record.persona_hosts.join(", ")}; Resume holds them back until you allow them on the new chat's tab`
                        : record.persona_hosts.join(", "),
            },
            { label: "Harness", value: row.harness ?? "not recorded" },
            {
              label: "Conversation",
              value: row.resumable
                ? "recorded — Resume gives it back to its harness"
                : "none recorded — Resume starts a new chat with this record",
            },
            { label: "File", value: row.path },
          ],
        }}
      />
      {record.dispatches.length > 0 && (
        /* What this chat handed to other chats before it wrote the record (#1452): from this
           machine's dispatch records, so a record another machine wrote lists none. */
        <section className="session-dispatches" aria-label="Dispatches this chat made">
          <h3>Dispatches</h3>
          <ul>
            {record.dispatches.map((made, at) => (
              // By position: the list arrives whole from one read, in the order they were made.
              <li
                key={at}
              >{`${made.persona ?? NO_PERSONA_SAID} · ${made.task} · ${made.outcome}`}</li>
            ))}
          </ul>
        </section>
      )}
      <article className="session-record release-notes" data-testid="session-record">
        <Markdown skipHtml components={withRefs(plane, record.place, repos)}>
          {record.body}
        </Markdown>
      </article>
      <FilesNamed plane={plane} body={record.body} place={record.place} repos={repos} />
    </>
  );
}

/** What `session_record` says a plane-root record's place is (`active::Place::PLANE_ROOT`). */
const PLANE_ROOT = "plane root";

/**
 * The repos of the record's workspace, by name: what its references resolve in. Empty for a
 * plane-root record, until they are read, and when they could not be — every reference then
 * stays text.
 */
function useRepos(plane: PlaneId, place: string | undefined): readonly string[] {
  const [read, setRead] = useState<{ place: string; repos: string[] }>();
  useEffect(() => {
    if (place === undefined || place === PLANE_ROOT) return;
    let gone = false;
    const told = (repos: string[]) => {
      if (!gone) setRead({ place, repos });
    };
    void commands
      .workspaceRepos(plane, place)
      .then((said) => told(said.status === "ok" ? said.data.repos.map((repo) => repo.name) : []))
      .catch(() => told([]));
    return () => {
      gone = true;
    };
  }, [plane, place]);
  return read !== undefined && read.place === place ? read.repos : NONE;
}

const NONE: readonly string[] = [];

/** What `code` names in the record's `place`, if it names a file there. */
function refOf(code: string, place: string, repos: readonly string[]): RecordRef | undefined {
  return place === PLANE_ROOT ? undefined : recordRef(code, place, repos);
}

/**
 * {@link COMPONENTS}, with an inline code span that names a file drawn as the jump to it. A
 * block's code ends in a newline and stays a block.
 */
function withRefs(plane: PlaneId, place: string, repos: readonly string[]): Components {
  return {
    ...COMPONENTS,
    code: ({ children, className }) => {
      const ref =
        typeof children === "string" && !children.includes("\n")
          ? refOf(children, place, repos)
          : undefined;
      if (ref === undefined) return <code className={className}>{children}</code>;
      return (
        <button
          type="button"
          tabIndex={0}
          className="record-ref"
          aria-label={`Open ${children}`}
          title={`Open ${ref.path} at line ${ref.line}`}
          onClick={() => jumpTo({ plane, ...ref })}
        >
          <code>{children}</code>
        </button>
      );
    },
  };
}

/**
 * **The files the record names**, each once, after the record: where *Open in your editor*
 * sits, since its button and the sentence it may answer are blocks and the record's references
 * sit in its sentences.
 */
function FilesNamed({
  plane,
  body,
  place,
  repos,
}: {
  plane: PlaneId;
  body: string;
  place: string;
  repos: readonly string[];
}) {
  const named = useMemo(
    () =>
      codeSpans(body).flatMap((code) => {
        const ref = refOf(code, place, repos);
        return ref === undefined ? [] : [{ code, ref }];
      }),
    [body, place, repos],
  );
  if (named.length === 0) return null;
  return (
    <section className="session-dispatches" aria-label="Files this record names">
      <h3>Files named</h3>
      <ul>
        {named.map(({ code, ref }) => (
          <li key={code}>
            <code>{code}</code>{" "}
            <ToYourEditor plane={plane} cut={ref.place} path={ref.path} line={ref.line} />
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * A record's `# title` and `## sections`, one level down: the view's heading is above them. A
 * memory's tab renders its body with the same (`MemoryTab.tsx`), for the same reason.
 */
export const COMPONENTS: Components = {
  h1: "h3",
  h2: "h4",
  h3: "h5",
  a: ({ href, children }) => <ExternalLink href={href}>{children}</ExternalLink>,
};
