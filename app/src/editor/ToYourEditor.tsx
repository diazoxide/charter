import { useState, type ReactNode } from "react";
import { commands, type PlaneId } from "../bindings";
import type { Place } from "../pieceViews";
import { askSettingsLink } from "../settings/links";
import { CHOOSE_EDITOR, NO_EDITOR, useYourEditor } from "../yourEditor";

// *Open in your editor* in a module of its own, apart from the light editor: a session record
// offers it too (#1043), and importing it from `PieceFiles.tsx` would pull CodeMirror into the
// window's main bundle (`tools/main-chunk-has-no-codemirror.mjs`).

/**
 * What a header's button answered — the core's refusal, or what was done — said under the
 * buttons as a status: *Open in your editor*'s, *Copy path*'s and Reveal's.
 */
export function Said({ children }: { children: ReactNode }) {
  return (
    <p className="piece-files-trouble" role="status">
      {children}
    </p>
  );
}

/**
 * *Open in your editor*: the button, and the sentence when nothing opened. With no editor
 * chosen it asks for one rather than guess, and links to where one is chosen (#1201).
 */
export function ToYourEditor({
  plane,
  cut,
  path,
  line,
}: {
  plane: PlaneId;
  cut: Place;
  path: string;
  line: number;
}) {
  const editor = useYourEditor();
  // What went wrong, for the file and editor it went wrong for: another file, or another
  // editor chosen, says nothing until it is tried.
  const [said, setSaid] = useState<{ about: string; trouble: string }>();
  const about = `${path}\n${editor ?? ""}`;
  const trouble = said?.about === about ? said.trouble : undefined;
  const say = (trouble: string | undefined) =>
    setSaid(trouble === undefined ? undefined : { about, trouble });
  const open = () => {
    if (editor === undefined) {
      say(NO_EDITOR);
      return;
    }
    say(undefined);
    void commands
      .openInYourEditor(plane, cut.workspace, cut.repo, cut.piece, path, line, editor)
      .then((answer) => {
        if (answer.status === "error") say(answer.error);
      })
      .catch((err: unknown) => say(String(err)));
  };
  return (
    <>
      <button type="button" tabIndex={0} onClick={open}>
        {`Open in your editor at line ${line}`}
      </button>
      {trouble !== undefined && (
        <Said>
          {trouble}
          {trouble === NO_EDITOR && (
            <>
              {" "}
              <button
                type="button"
                tabIndex={0}
                onClick={() => askSettingsLink(plane, CHOOSE_EDITOR)}
              >
                Choose your editor
              </button>
            </>
          )}
        </Said>
      )}
    </>
  );
}
