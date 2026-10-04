import { useEffect, useMemo, useState } from "react";
import { commands, type Curations } from "./bindings";

/**
 * The subjects a project offers curation actions on (ADR 0061), in the core's spelling: the
 * plane itself, each workspace, each persona. What the "Curate ▸" menus and the palette's
 * `Curate <subject>: <label>` rows are asked for.
 */
export function curationSubjects(
  workspaces: readonly string[],
  personas: readonly string[],
): string[] {
  return [
    "plane",
    ...workspaces.map((name) => `workspace:${name}`),
    ...personas.map((name) => `persona:${name}`),
  ];
}

/**
 * **What each subject is offered, as the core resolves it** (`curation_offers`): asked again
 * when the subjects change and when the core says a change on disk concerns the curations
 * (`CURATIONS`, FD-10d) — a persona's `curation/` file edited in an editor is one of those, a
 * todo or a memory never is — and never kept past the plane it was asked for.
 *
 * Written as `useVaults` is: the command's own promise, a `gone` flag, and state set only in
 * its callback. Keyed on the subjects' spelling rather than the array, so a sidebar read again
 * with the same workspaces asks nothing.
 */
export function useCurations(
  plane: string,
  subjects: readonly string[],
  curationsChanges: number,
): Curations | undefined {
  const key = subjects.join("\n");
  const [said, setSaid] = useState<{ plane: string; key: string; curations: Curations }>();
  useEffect(() => {
    let gone = false;
    const asked = key === "" ? [] : key.split("\n");
    void commands
      .curationOffers(plane, asked)
      .then((answer) => {
        if (gone || answer == null || answer.status !== "ok") return;
        if (!Array.isArray(answer.data?.subjects)) return;
        setSaid({ plane, key, curations: answer.data });
      })
      .catch(() => {
        // Nothing: a core that did not answer leaves the menus without a Curate group.
      });
    return () => {
      gone = true;
    };
  }, [plane, key, curationsChanges]);
  return useMemo(() => (said?.plane === plane ? said.curations : undefined), [said, plane]);
}
