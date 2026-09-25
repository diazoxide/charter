import type { ComponentType, ReactNode } from "react";

/**
 * One section of the Attention region: **its heading, and whatever it holds under it.**
 *
 * **One component, so every section is told apart the same way.** charter's own panels, an
 * extension's and the plane's vaults all draw through this, so a divider, a title's look or its
 * spacing cannot be given to charter's panels and not to a stranger's (ADR 0043: a contributed
 * panel is declared data, and drawing it is charter's job).
 *
 * **A title, not a row.** The heading keeps the panel's mark, because the mark is the one place
 * a panel's declared `mark` is drawn; its look (`.sidebar-title`) is what makes it read as the
 * start of a section rather than the first item in one.
 */
export function PanelSection({
  testid,
  from,
  mark: Mark,
  title,
  provenance,
  actions,
  children,
}: {
  testid: string;
  /** Who contributed it, for `data-panel-from`; left off a section that is not a panel. */
  from?: string;
  mark: ComponentType<{ className?: string }>;
  title: string;
  /** Whose panel this is, beside the title, when it is not charter's own. */
  provenance?: string;
  /** Buttons on the heading, such as a view's. */
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section data-testid={testid} data-panel-from={from}>
      <div className="panel-head">
        <h2 className="sidebar-title">
          <Mark className="node-icon" />
          {title}
          {/* **What is in force, after approval and not only at it** — ADR 0041 item
              5. An operator has to be able to tell a panel his own charter draws from one a
              stranger's extension contributed, without opening a dialog to find out. */}
          {provenance !== undefined && <span className="panel-from">{` · ${provenance}`}</span>}
        </h2>
        {actions}
      </div>
      {children}
    </section>
  );
}
