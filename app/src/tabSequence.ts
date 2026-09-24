/**
 * **The engine's tab sequence, written down once** (charter-app#186, charter-app#189).
 *
 * charter embeds the system WebView — a WKWebView on macOS and WebKitGTK on Linux — so where
 * Tab goes is WebKit's decision, and WebKit's rule for a form control is this, in full, since
 * r263447 (`[popover] Improve focus handling`, shipped in Safari 17 and WebKitGTK 2.42):
 *
 * ```cpp
 * bool HTMLFormControlElement::isKeyboardFocusable(const FocusEventData& focusEventData) const
 * {
 *     if (!!tabIndexSetExplicitly())
 *         return Element::isKeyboardFocusable(focusEventData);
 *     return isFocusable() && document().frame()
 *         && document().frame()->eventHandler().tabsToAllFormControls(focusEventData);
 * }
 * ```
 *
 * It has two readers. The tests, which cannot run WebKit and hold the window to this instead
 * (`Modals.keyboard.test.tsx`, `Window.keyboard.test.tsx`); and a chat's terminal, whose Tab is
 * the shell's, so the one way the keyboard leaves it (`SessionPane.leavesTheChat`) has to walk
 * the same sequence Tab would — not a library's idea of it. `tabbable`, the usual answer, counts
 * every `<button>`, which is exactly the gap WebKit does not.
 */

/** The `<input>` types WebKit tabs to with full keyboard access off (`TextFieldInputType`). */
const TEXT_ENTRY = new Set(["text", "search", "url", "tel", "email", "password", "number"]);

/**
 * Whether WebKit puts this element in the tab sequence, with full keyboard access off.
 *
 * - **an explicit `tabindex` ends the question** — `HTMLFormControlElement::isKeyboardFocusable`
 *   hands straight over to `Element::isKeyboardFocusable`, which asks only whether the element
 *   is focusable and the index is not negative;
 * - **a form control without one is skipped** — that is the `tabsToAllFormControls` gate, and
 *   it is off unless macOS's full keyboard access is on;
 * - **except a text field**, which `TextFieldInputType::isKeyboardFocusable` answers for
 *   itself and never consults the gate — which is why Tab moves between text boxes in Safari
 *   and between nothing else;
 * - **anything else focusable is in**: a link, a `<summary>`, a `div` that was given an index.
 *
 * It answers about the attributes only. Whether the element is drawn — `visibility: hidden`, a
 * closed `<details>` — is the engine's to know, and {@link moveAlong} asks it by trying.
 */
export function inWebKitsTabSequence(el: HTMLElement): boolean {
  if (el.hasAttribute("disabled")) return false;
  if (el.tabIndex < 0) return false;
  if (el.hasAttribute("tabindex")) return true;
  const tag = el.tagName.toLowerCase();
  if (tag === "a") return el.hasAttribute("href");
  if (tag === "input") return TEXT_ENTRY.has((el as HTMLInputElement).type);
  if (tag === "textarea" || tag === "summary") return true;
  if (tag === "button" || tag === "select") return false;
  return el.isContentEditable;
}

/** Everything under `scope` that WebKit would stop at, in the order it would stop. */
export function sequenceIn(scope: ParentNode): HTMLElement[] {
  return [...scope.querySelectorAll<HTMLElement>("*")].filter(inWebKitsTabSequence);
}

/**
 * Moves the keyboard to the first stop outside `from` in one direction, as Tab would have.
 *
 * **Outside `from`, and not merely after the focused element**: leaving a terminal means
 * leaving the pane, not landing on the next thing inside xterm's own DOM. A stop the engine
 * will not take — hidden, or inside something closed — refuses `focus()` and the next one is
 * tried, which is how a sequence read from attributes stays honest about what is drawn.
 *
 * @returns Whether the keyboard moved. At either end of the window it stays where it was.
 */
export function moveAlong(from: Element, backwards: boolean): boolean {
  const side = backwards ? Node.DOCUMENT_POSITION_PRECEDING : Node.DOCUMENT_POSITION_FOLLOWING;
  const beyond = sequenceIn(from.ownerDocument).filter(
    (el) => !from.contains(el) && from.compareDocumentPosition(el) & side,
  );
  if (backwards) beyond.reverse();
  const was = from.ownerDocument.activeElement;
  for (const candidate of beyond) {
    candidate.focus();
    if (from.ownerDocument.activeElement !== was) return true;
  }
  return false;
}
