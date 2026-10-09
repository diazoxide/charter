import type { ReactNode } from "react";
import type { PlaneId } from "../bindings";
import { askSettingsAction } from "./links";

/**
 * **A link out of Settings, drawn** (#1387, #1388): a button in the setting rows' link style that
 * asks the project's window to run the catalogue's row `action` (`links.ts`) — open a vault's or
 * a persona's tab, the vault picker, the Extensions dialog.
 */
export function OutLink({
  plane,
  action,
  label,
  children,
}: {
  plane: PlaneId;
  action: string;
  /** Its accessible name, where a page names its buttons beyond their words (the dispatch
   *  table names each by what it shows, then whose row it is on). */
  label?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className="ui-setting-reset"
      aria-label={label}
      // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
      tabIndex={0}
      onClick={() => askSettingsAction(plane, action)}
    >
      {children}
    </button>
  );
}
