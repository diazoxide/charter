import { useEffect, useState } from "react";
import Markdown, { type Components } from "react-markdown";
import { LoaderCircle } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { ExternalLink } from "./ReleaseNotes";
import { Facts } from "./Views";
import { commands, type PlaneId, type SessionRecordView } from "./bindings";

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
 */
export function SessionRecordTab({ plane, path }: { plane: PlaneId; path: string }) {
  const [said, setSaid] = useState<{ record?: SessionRecordView | null; trouble?: string }>();
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
  }, [plane, path]);

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
      <p className="trouble" role="alert">
        {said.trouble}
      </p>
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
      <article className="session-record release-notes" data-testid="session-record">
        <Markdown skipHtml components={COMPONENTS}>
          {record.body}
        </Markdown>
      </article>
    </>
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
