import { commands, type ExtensionTheme, type ProjectExtension } from "../bindings";
import { BUILT_IN_ICONS } from "../theme/icons";
import { key, textAt, valueAt, shown, type Control, type Shown } from "./fileControls";

/**
 * **The Icons setting, in either Appearance group** (#1145; FM-3, #1106): `[theme] icons` as a
 * pick, the way Theme is one — charter's own icon theme and every icon theme an extension this
 * machine approved contributes, as `<extension>/<icon theme>`, whether or not the project has
 * that extension on (the Theme list's terms, `project_theme`'s `options`).
 *
 * The list is `extension_icon_themes`' answer, the same one the file trees draw from
 * (`projectTheme.ts`), so no new command is asked. A pick the file holds that is not in the list
 * stays shown as held (`SettingsTab`'s select keeps it), and {@link iconsNotes} says why the
 * trees draw charter's own instead.
 */

/** The path the pick is kept at, in a project's file and in a workspace's `settings`. */
const ICONS = key("theme", "icons");

/** charter's own, by the name a file picks it by (`BUILT_IN_ICON_THEMES`). */
const BUILT_IN = Object.keys(BUILT_IN_ICONS);

/** The icon theme drawn when no pick can be: `purlis_core`'s `ICONS_FALLBACK`. */
const FALLBACK = "charter-icons";

/** An offered icon theme as a file picks it. */
const pickOf = (one: ExtensionTheme) => `${one.extension}/${one.name}`;

/**
 * The Icons control: a pick of `offered`, with each extension named as the Extensions group
 * names it. `offered` is `undefined` until the core has answered, and the built-in is offered
 * on its own until then.
 */
export function iconsControl(
  here: "project" | "workspace",
  offered: readonly ExtensionTheme[] | undefined,
  extensions: readonly ProjectExtension[],
): Control {
  const whose = (id: string) => extensions.find((it) => it.id === id)?.name ?? id;
  const labels: Record<string, string> = Object.fromEntries(
    BUILT_IN.map((name) => [name, `${name} (built in)`]),
  );
  for (const one of offered ?? []) labels[pickOf(one)] = `${one.name} (${whose(one.extension)})`;
  return {
    ...textAt(ICONS, "Icons", {
      kind: "choice",
      choices: [...new Set([...BUILT_IN, ...(offered ?? []).map(pickOf)])],
      hint:
        here === "project"
          ? "The file trees' and search results' icons while this project is in front: charter's own, or an icon theme an extension approved on this machine contributes."
          : "The file trees' and search results' icons while this workspace is in front: charter's own, or an icon theme an extension approved on this machine contributes.",
    }),
    unset: here === "project" ? `not set — ${FALLBACK}` : "not set — the project's pick",
    labels,
  };
}

/** Why the trees cannot draw extension `id`'s icon theme `name` here, or `null` when they can. */
function unavailable(
  id: string,
  name: string,
  extensions: readonly ProjectExtension[],
  offered: readonly ExtensionTheme[] | undefined,
): string | null {
  const it = extensions.find((one) => one.id === id);
  switch (it?.state) {
    case "on":
      // Only once the core has said what there is: until then, nothing is said.
      if (offered === undefined || offered.some((one) => pickOf(one) === `${id}/${name}`))
        return null;
      return `${id} contributes no icon theme called “${name}”`;
    case "off":
      return it.source === "workspace"
        ? `${id} is off in this workspace`
        : `${id} is off in this project`;
    case "needs-approval":
      return `this machine has not approved ${id}`;
    default:
      return `${id} is not installed on this machine`;
  }
}

/**
 * **What the trees draw instead of the pick `file` holds**, in the core's words for it
 * (`extension::project::theme::unavailable`, which `resolve_icons` writes as `Resolved::why`):
 * nothing for no pick, charter's own, or a pick the trees can draw. A value that is no pick at
 * all is not said here: the file's reader refuses it, and the tab says that above the groups.
 *
 * `drawn` is the core's answer for the same place (`project_icons_drawn`), when it is known: a
 * pick that is neither `held`'s nor the built-in is another file's that wins over `held`'s — a
 * workspace's is under `charter.local.toml`'s — and then `held`'s is not what is said.
 */
export function iconsNotes(
  held: { file: string; pick: string } | undefined,
  extensions: readonly ProjectExtension[],
  offered: readonly ExtensionTheme[] | undefined,
  drawn?: string | null,
): string[] {
  if (held === undefined || BUILT_IN.includes(held.pick)) return [];
  if (drawn != null && drawn !== held.pick && !BUILT_IN.includes(drawn)) return [];
  const cut = held.pick.indexOf("/");
  if (cut <= 0 || cut === held.pick.length - 1) return [];
  const id = held.pick.slice(0, cut);
  const name = held.pick.slice(cut + 1);
  const because = unavailable(id, name, extensions, offered);
  return because === null
    ? []
    : [
        `${held.file} picks “${name}” from ${id}, but ${because} — so the built-in ${FALLBACK} is drawn`,
      ];
}

/** The pick `file` holds, and the file's name; `undefined` when it holds none. */
export function iconsHeld(file: Shown): { file: string; pick: string } | undefined {
  const pick = shown(valueAt(file, ICONS)).trim();
  return pick === "" ? undefined : { file: file.file, pick };
}

/** Every icon theme an approved extension contributes, as the file trees ask it; `undefined`
 *  when the core could not say. */
export function askIconThemes(): Promise<ExtensionTheme[] | undefined> {
  return commands
    .extensionIconThemes()
    .then((said) => (said.status === "ok" ? (said.data ?? []) : undefined))
    .catch(() => undefined);
}
