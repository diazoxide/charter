import type { ReactNode } from "react";
import type { ProfileRow } from "./bindings";

/**
 * **What a profile's row says, and the sentence that approves it** — the picker's own (ADR 0022),
 * shared by every surface that records a profile approval, so none of them says it differently.
 *
 * The operator's ruling (V69, PR #949): a surface other than the picker may record a profile
 * approval only with the picker's exact sentence — *charter has not run this profile before*, or
 * *as it now stands* when the command changed since it was approved — with the command in
 * `<code>`, which `.choice .meta .where` keeps undimmed, and the row's `.needs-approval` mark.
 */
export function ApprovalSentence({ row }: { row: ProfileRow | undefined }) {
  if (!row?.approval) return null;
  return (
    <p className="honest approve" role="alert">
      charter has not run this profile {row.approval === "new" ? "before" : "as it now stands"}. It
      would run: <code>{row.shown}</code>
    </p>
  );
}

/**
 * A profile row's detail: what kind it is, the command, where it was declared, whether it is the
 * default, and whether it needs approving. It describes the row's name, which is the radio's
 * whole accessible name.
 */
export function ProfileMeta({
  row,
  id,
  children,
}: {
  row: ProfileRow;
  /** The id the radio's `aria-describedby` names, when the row does not name its own. */
  id?: string;
  /** What the surface adds, after the picker's own detail. */
  children?: ReactNode;
}) {
  return (
    <span className="meta" id={id}>
      <span className="what">{row.kind}</span>
      {/* Already contained by the core: a profile is a file a chat can write, and a control
          byte in a command must never redraw this row. */}
      <code className="where">{row.shown}</code>
      <span className="from">{row.source}</span>
      {row.is_default && <span className="what">default</span>}
      {row.approval && <span className="what needs-approval">{row.approval}</span>}
      {children}
    </span>
  );
}
