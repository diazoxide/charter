/**
 * Test-only: **every control has a name** (DS-6, #629).
 *
 * A screen reader says a control by its role and its accessible name: "Reset, button". A
 * control without a name is said as "button", and nobody who cannot see the icon on it knows
 * what it does. So every control a surface draws is held to a non-empty accessible name, the
 * name `@testing-library`'s role queries compute (the accessible-name algorithm the platform
 * uses: `aria-labelledby`, `aria-label`, a `<label>`, the text inside, `title`).
 *
 * **Without axe** (D-629-1). axe-core would be a new dependency, which is the operator's call
 * (#629's acceptance line stays open for it). This is the rule axe's `button-name`, `link-name`,
 * `label` and `aria-input-field-name` hold, written with what the window's tests already have.
 *
 * Only what is in the accessibility tree is held: a control inside `aria-hidden` or `hidden`
 * is not said to anyone, so it has nothing to be named for.
 */
import { queryAllByRole, type ByRoleMatcher } from "@testing-library/react";
import { expect } from "vitest";

/** Every role a person operates, as the window draws them. */
export const CONTROL_ROLES = [
  "button",
  "link",
  "textbox",
  "searchbox",
  "checkbox",
  "radio",
  "switch",
  "tab",
  "combobox",
  "listbox",
  "option",
  "slider",
  "spinbutton",
  "menuitem",
  "menuitemcheckbox",
  "menuitemradio",
] as const satisfies readonly ByRoleMatcher[];

/** How a nameless control is written in a failure and in a debt list: its role, its element
 *  and the classes it is drawn with, which are what a person finds it by in the source. */
export function described(role: string, element: Element): string {
  const classes = (element.getAttribute("class") ?? "").trim().split(/\s+/).filter(Boolean);
  const tag = element.tagName.toLowerCase();
  return `${role} <${tag}${classes.length > 0 ? `.${classes.join(".")}` : ""}>`;
}

/** Each control under `root` whose accessible name is empty, described, in document order. */
export function namelessControls(root: HTMLElement): string[] {
  const nameless = new Map<Element, string>();
  for (const role of CONTROL_ROLES)
    for (const element of queryAllByRole(root, role, { name: (name) => name.trim() === "" }))
      if (!nameless.has(element)) nameless.set(element, described(role, element));
  return [...nameless.entries()]
    .sort(([a], [b]) => (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1))
    .map(([, said]) => said);
}

/**
 * Fails, naming each one, if a control under `root` has no accessible name. `debt` lists the
 * nameless controls a surface is known to draw in files this check cannot mend yet, each as
 * {@link described} writes it: they are let through, and a debt that is no longer drawn fails
 * too (see {@link paidDebts}), so the list only shrinks. Answers what it found, debts included.
 */
export function expectEveryControlNamed(
  root: HTMLElement,
  debt: readonly string[] = [],
  where = "",
): string[] {
  const nameless = namelessControls(root);
  const owed = new Set(debt);
  expect(
    nameless.filter((one) => !owed.has(one)),
    `nameless controls${where ? ` in ${where}` : ""}`,
  ).toEqual([]);
  return nameless;
}

/** The debts in `debt` that no surface drew: each is paid, and comes off the list. */
export function paidDebts(debt: readonly string[], drawn: Iterable<string>): string[] {
  const seen = new Set(drawn);
  return debt.filter((one) => !seen.has(one));
}
