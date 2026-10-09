import type { Level } from "./groups";

/**
 * **Every Settings group by its address and its label, without building a level** (#1201).
 *
 * A group's id and label live inside its level's builder (`you.tsx`, `project.ts`,
 * `workspace.tsx`, and the pages `dispatch.tsx`, `GrantedList.tsx` and `thisMachine.tsx` add),
 * which needs the level's files read before it can say anything. The palette lists one row per
 * group before anything is read, so it reads this list instead. `catalogue.test.ts` reads the
 * builders' sources and fails on a group this list lacks, misnames or keeps after it went.
 *
 * In each level's order, as its group nav lists them. A profile's own page
 * (`project.profile.<name>`) is not here: there is one per profile, and the palette's row for
 * profiles is Harness & profiles. The Persona level has no groups of its own yet.
 */
export type CataloguedGroup = { id: string; label: string };

export const SETTINGS_GROUPS: Readonly<
  Record<Exclude<Level, "persona">, readonly CataloguedGroup[]>
> = {
  you: [
    { id: "you.text", label: "Text" },
    { id: "you.editor", label: "Editor" },
    { id: "you.chats", label: "Chats list" },
    { id: "you.machine", label: "This machine" },
  ],
  project: [
    { id: "project.general", label: "General" },
    { id: "project.saving", label: "Saving" },
    { id: "project.harness", label: "Harness & profiles" },
    { id: "project.sandbox", label: "Sandbox" },
    { id: "project.sandbox.mine", label: "Your hosts" },
    { id: "project.sandbox.granted", label: "Granted" },
    { id: "project.dispatch", label: "Dispatch" },
    { id: "project.forges", label: "Forges" },
    { id: "project.extensions", label: "Extensions" },
    { id: "project.appearance", label: "Appearance" },
    { id: "project.plugins", label: "Plugins" },
  ],
  workspace: [
    { id: "workspace.live", label: "Live" },
    { id: "workspace.repos", label: "Repos" },
    { id: "workspace.dispatch", label: "Dispatch" },
    { id: "workspace.extensions", label: "Extensions" },
    { id: "workspace.appearance", label: "Appearance" },
    { id: "workspace.plugins", label: "Plugins" },
  ],
};
