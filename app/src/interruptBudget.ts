/**
 * **W10's interrupt budget** (DS-9, #631): at most three prompts stand between a new machine and
 * the first answered agent turn. It is part of ST9's definition of done, and the first run's
 * scenario tests (`FirstRun.test.tsx`) count every prompt each path shows and fail above it.
 *
 * **A prompt is what stops the operator until they answer it**, told apart by what the page says
 * it is, never by how it is drawn:
 *
 * - every `dialog` and `alertdialog`, modal or not: the picker, the trust question, any confirm,
 *   and an open native `<dialog>`, whose role is its own;
 * - every inline question: a `group` or `radiogroup` whose name is a question, ending in "?" —
 *   the forge question is one. A `<fieldset>` is a group, named by its `<legend>`. An ask moved
 *   out of a dialog onto the page is still an ask.
 *
 * The first-run screen is not one of them: asking for the repo is what the first run is for,
 * not an interruption on the way to it. A `status` line, an `alert` saying why something
 * failed, a tab opened beside the chat and a group of controls that asks nothing are not
 * prompts either.
 *
 * **What it cannot see**, so a review has to:
 *
 * - a question whose group is named without a "?" ("Pick a forge");
 * - a toast or a `status` line that carries an action the operator has to take;
 * - a native OS dialog (tauri-plugin-dialog, such as the folder picker), which is not on the
 *   page — only the real app sees one, which is why FR-1's clean-machine run should count too;
 * - a second prompt shown at the same moment as one with the same role and name, which counts
 *   once.
 *
 * Test-only: nothing in the window imports this.
 */
export const INTERRUPT_BUDGET = 3;

const PROMPTS =
  '[role="dialog"], [role="alertdialog"], [role="group"], [role="radiogroup"], dialog[open], fieldset';

/** The role an element has: the one it is given, else its tag's own. */
function roleOf(one: Element): string | null {
  const given = one.getAttribute("role");
  if (given) return given;
  if (one.localName === "dialog") return "dialog";
  if (one.localName === "fieldset") return "group";
  return null;
}

/**
 * What an element is called: its `aria-label`, else the text its `aria-labelledby` names, else
 * for a `<fieldset>` its `<legend>`.
 */
function nameOf(one: Element): string {
  const label = one.getAttribute("aria-label");
  if (label) return label.trim();
  const ids = one.getAttribute("aria-labelledby")?.split(/\s+/).filter(Boolean) ?? [];
  const named = ids
    .map((id) => one.ownerDocument.getElementById(id)?.textContent ?? "")
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
  if (named || one.localName !== "fieldset") return named;
  const legend = [...one.children].find((child) => child.localName === "legend");
  return (legend?.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** The prompts on the page now, each as `role|name` once. */
function onThePage(root: ParentNode): Set<string> {
  const now = new Set<string>();
  for (const one of root.querySelectorAll(PROMPTS)) {
    const role = roleOf(one);
    const name = nameOf(one);
    if ((role === "group" || role === "radiogroup") && !name.endsWith("?")) continue;
    now.add(`${role}|${name}`);
  }
  return now;
}

export type Interrupts = {
  /** Every prompt shown since counting began, by name, in the order shown. */
  asked: () => string[];
  /** Stops counting; what was counted stays. */
  stop: () => void;
};

/**
 * Counts the prompts shown in `root` from now on, and the ones already up. A prompt counts
 * once for each time it appears: drawn again while it is up is the same ask, and taken down
 * and shown again is a second one.
 */
export function countInterrupts(root: ParentNode = document.body): Interrupts {
  const asked: string[] = [];
  let up = new Set<string>();
  const look = () => {
    const now = onThePage(root);
    for (const key of now) if (!up.has(key)) asked.push(key.slice(key.indexOf("|") + 1));
    up = now;
  };
  look();
  let stopped = false;
  const observer = new MutationObserver(look);
  observer.observe(root as Node, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["role", "aria-label", "aria-labelledby", "open"],
  });
  const catchUp = () => {
    if (stopped) return;
    if (observer.takeRecords().length > 0) look();
  };
  return {
    asked: () => {
      catchUp();
      return [...asked];
    },
    stop: () => {
      catchUp();
      observer.disconnect();
      stopped = true;
    },
  };
}

/** Throws, naming every prompt, when `asked` holds more than the budget allows. */
export function withinTheBudget(asked: readonly string[], budget = INTERRUPT_BUDGET): void {
  if (asked.length <= budget) return;
  throw new Error(
    `${asked.length} prompts before the first answered turn, over W10's budget of ${budget}: ` +
      asked.join(" · "),
  );
}
