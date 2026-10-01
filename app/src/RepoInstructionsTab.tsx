import { useCallback, useEffect, useId, useState } from "react";
import * as Checkbox from "@radix-ui/react-checkbox";
import { LoaderCircle } from "lucide-react";
import { commands, type InstructionFile, type PlaneId } from "./bindings";

/**
 * **The agent instructions a workspace's repo carries, offered to its memory** (FR-18a, #612):
 * `CLAUDE.md`, `AGENTS.md` and `.cursor/rules`, each shown whole, and written into the
 * workspace's memory only when **Add to memory** is pressed — the preview's yes (W10).
 *
 * **A tab, not a dialog.** The first run opens it beside the first chat and not in front of it,
 * so it asks nothing until the operator goes to it: W10's budget of three questions before the
 * first answered turn — the repo, the trust question and, only when there is a choice, the
 * picker — is not spent on it. Leaving it unanswered costs nothing; closing it writes nothing.
 *
 * Every file the core offers starts ticked, since the operator is looking at exactly what would
 * be written. A file already in memory, or left out — a link, a file too large, one that looks
 * like it holds a secret — is listed with why, and has no box. The core is handed back the text
 * shown here and writes nothing if a file no longer holds it
 * (`charter_core::repoinstructions::import`).
 *
 * The words are ADR 0072's: a code repo is a "repo", and what is written is the workspace's
 * memory.
 */
export function RepoInstructionsTab({
  plane,
  workspace,
  onClose,
}: {
  plane: PlaneId;
  workspace: string;
  /** Close this tab: Not now. */
  onClose: () => void;
}) {
  const [files, setFiles] = useState<InstructionFile[]>();
  const [trouble, setTrouble] = useState<string>();
  const [unticked, setUnticked] = useState<ReadonlySet<string>>(new Set());
  const [adding, setAdding] = useState(false);
  const [added, setAdded] = useState<number>();
  const [asked, setAsked] = useState(0);

  useEffect(() => {
    let gone = false;
    void commands
      .repoInstructions(plane, workspace)
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setTrouble(answer.error);
        else setFiles(answer.data);
      })
      .catch((err: unknown) => {
        if (!gone) setTrouble(String(err));
      });
    return () => {
      gone = true;
    };
  }, [plane, workspace, asked]);

  const offered = (files ?? []).filter((one) => one.standing === "offered");
  const ticked = offered.filter((one) => !unticked.has(named(one)));

  const add = useCallback(() => {
    setAdding(true);
    setTrouble(undefined);
    void commands
      .importInstructions(
        plane,
        workspace,
        ticked.map(({ repo, file, text }) => ({ repo, file, text })),
      )
      .then((answer) => {
        if (answer.status === "error") setTrouble(answer.error);
        else {
          setAdded(answer.data);
          setUnticked(new Set());
          setAsked((was) => was + 1);
        }
      })
      .catch((err: unknown) => setTrouble(String(err)))
      .finally(() => setAdding(false));
  }, [plane, workspace, ticked]);

  if (files === undefined && trouble === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the repo…
      </p>
    );
  }

  return (
    <div className="repo-instructions">
      <p className="came-back">
        This repo has instructions for AI agents. Add them to this workspace&apos;s memory and every
        chat here starts with them, whichever agent it runs. Nothing is written until you press Add
        to memory. Nothing is written into your repo.
      </p>

      {/* Verbatim: the core's sentence names the file and what to do. */}
      {trouble && (
        <p className="trouble said-in-full" role="alert">
          {trouble}
        </p>
      )}
      {added !== undefined && (
        <p className="came-back" role="status">
          {added === 1 ? "Added 1 file to memory." : `Added ${added} files to memory.`}
        </p>
      )}

      <ul className="repo-instruction-files" aria-label="Instruction files">
        {(files ?? []).map((one) => (
          <FileRow
            key={named(one)}
            file={one}
            ticked={!unticked.has(named(one))}
            onTicked={(on) =>
              setUnticked((was) => {
                const next = new Set(was);
                if (on) next.delete(named(one));
                else next.add(named(one));
                return next;
              })
            }
          />
        ))}
      </ul>

      {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#189). */}
      <div className="doing">
        <button type="button" tabIndex={0} disabled={ticked.length === 0 || adding} onClick={add}>
          Add to memory
        </button>
        <button type="button" tabIndex={0} onClick={onClose}>
          {added === undefined ? "Not now" : "Done"}
        </button>
      </div>
    </div>
  );
}

/** How a file is named to the operator, and keyed: `<repo>/<path>`. */
function named(file: InstructionFile): string {
  return `${file.repo}/${file.file}`;
}

function FileRow({
  file,
  ticked,
  onTicked,
}: {
  file: InstructionFile;
  ticked: boolean;
  onTicked: (on: boolean) => void;
}) {
  const id = useId();
  const name = named(file);
  return (
    <li className="choice">
      {file.standing === "offered" ? (
        <>
          <Checkbox.Root
            id={id}
            className="box"
            checked={ticked}
            onCheckedChange={(next) => onTicked(next === true)}
            tabIndex={0}
            aria-label={name}
          >
            <Checkbox.Indicator className="box-mark">✓</Checkbox.Indicator>
          </Checkbox.Root>
          <label className="who" htmlFor={id}>
            {name}
          </label>
        </>
      ) : (
        <span className="who">
          {name}
          {": "}
          {file.standing === "in-memory" ? "already in memory" : `left out; ${file.why ?? ""}`}
        </span>
      )}
      {file.text !== "" && <pre className="repo-instruction-text">{file.text}</pre>}
    </li>
  );
}
