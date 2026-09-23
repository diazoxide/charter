import type { ReactNode } from "react";
import Markdown, { type Components } from "react-markdown";
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * One version's release notes, drawn from the Markdown `CHANGELOG.md` holds them in.
 *
 * The notes arrive as Markdown because that is what they are: a section of the changelog, with
 * `### Added` / `### Changed` / `### Fixed` and a bullet per change. The GitHub release page
 * renders the same text, and a dialog that printed it raw would show `- **Switching tabs**`
 * where the page shows a list.
 *
 * **`react-markdown`, and no HTML.** It builds React elements from the Markdown's syntax tree,
 * so nothing is handed to `innerHTML`, and `skipHtml` drops any raw HTML in the source rather
 * than rendering it or printing it as text. There is no `rehype-raw` and there must never be.
 *
 * **A link opens in the operator's browser**, through the opener plugin (`opener:default`
 * allows http and https). Followed inside the webview it would replace charter's own window
 * with a GitHub page and no way back. A link to anything other than http or https is drawn as
 * its text. `tabIndex={0}` is WebKit's rule (`docs/ui-primitives.md`): a link is out of the tab
 * sequence unless its `tabindex` is written down.
 *
 * A `###` heading is drawn one level down, as an `h4`, because every surface that shows notes
 * already has its own heading for the version above them.
 */
export function ReleaseNotes({ markdown }: { markdown: string }) {
  return (
    <div className="release-notes">
      <Markdown skipHtml components={COMPONENTS}>
        {markdown}
      </Markdown>
    </div>
  );
}

/**
 * A link that opens in the operator's browser rather than in charter's window. Anything but an
 * http or https address is drawn as its text.
 */
export function ExternalLink({ href, children }: { href?: string; children: ReactNode }) {
  if (href === undefined || !/^https?:\/\//.test(href)) return <>{children}</>;
  return (
    <a
      href={href}
      tabIndex={0}
      onClick={(event) => {
        event.preventDefault();
        void openUrl(href);
      }}
    >
      {children}
    </a>
  );
}

const COMPONENTS: Components = {
  h1: "h4",
  h2: "h4",
  h3: "h4",
  a: ({ href, children }) => <ExternalLink href={href}>{children}</ExternalLink>,
};
