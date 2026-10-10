import { useEffect, useState } from "react";
import { FileText, LoaderCircle } from "lucide-react";
import { commands, type InstructionFile, type PlaneId } from "./bindings";
import { EmptyState } from "./EmptyState";
import { Choice, SettingActions, SettingRow } from "./settings/components";

/**
 * **The agent instructions a workspace's repo carries, offered to its memory** (FR-18a, #612):
 * `CLAUDE.md`, `AGENTS.md` and `.cursor/rules`, each shown whole, and written into the
 * workspace's memory only when **Add to memory** is pressed — the preview's yes (W10).
 *
 * **A tab, not a dialog.** The first run opens it beside the first chat and not in front of it,
 * so it asks nothing until the operator goes to it: W10's budget of three prompts before the
 * first answered turn — the forge question when the repo's remote does not say, the trust
 * question and, only when there is a choice, the picker (`interruptBudget.ts`) — is not spent
 * on it. Leaving it unanswered costs nothing; closing it writes nothing.
 *
 * Every file the core offers starts ticked, since the operator is looking at exactly what would
 * be written. A file already in memory, or left out — a link, a file too large, one that looks
 * like it holds a secret — is listed with why, and has no box. The core is handed back the text
 * shown here and writes nothing if a file no longer holds it
 * (`purlis_core::repoinstructions::import`).
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
  /** The boxes the operator has pressed, by file: what each was set to. */
  const [pressed, setPressed] = useState<ReadonlyMap<string, boolean>>(new Map());
  const [adding, setAdding] = useState(false);
  const [added, setAdded] = useState<number>();
  /** Bumped to read the files again: after an import, and after a refusal, so what is shown is
   *  what is on disk now and never the text the core just refused. */
  const [asked, setAsked] = useState(0);

  useEffect(() => {
    let gone = false;
    void commands
      .repoInstructions(plane, workspace)
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setTrouble(answer.error);
        else {
          setFiles(answer.data);
          setPressed(new Map());
        }
      })
      .catch((err: unknown) => {
        if (!gone) setTrouble(String(err));
      });
    return () => {
      gone = true;
    };
  }, [plane, workspace, asked]);

  const ticked = (files ?? []).filter(
    (one) => one.standing.kind === "offered" && isTicked(one, pressed),
  );

  function add() {
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
        else setAdded(answer.data);
      })
      .catch((err: unknown) => setTrouble(String(err)))
      .finally(() => {
        setAdding(false);
        setAsked((was) => was + 1);
      });
  }

  if (files === undefined && trouble === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the repo…
      </p>
    );
  }

  // Nothing to offer: the sentence below would say the repo has instructions over an empty
  // list. Read again is the tab's own read, for a file added since it opened.
  if (files !== undefined && files.length === 0 && trouble === undefined) {
    return (
      <EmptyState
        mark={FileText}
        headline="No instructions for AI agents in this workspace's repos"
        body={
          <>
            purlis offers a repo&apos;s <code>CLAUDE.md</code> and <code>AGENTS.md</code> at its
            top, and the rules under <code>.cursor/rules</code>, to this workspace&apos;s memory.
            Add one to a repo, then press Read again.
          </>
        }
        action={
          <button type="button" tabIndex={0} onClick={() => setAsked((was) => was + 1)}>
            Read again
          </button>
        }
        testid="repo-instructions-empty"
      />
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
            ticked={isTicked(one, pressed)}
            onTicked={(on) => setPressed((was) => new Map(was).set(named(one), on))}
          />
        ))}
      </ul>

      {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#189). */}
      <SettingActions>
        <button type="button" tabIndex={0} disabled={ticked.length === 0 || adding} onClick={add}>
          Add to memory
        </button>
        <button type="button" tabIndex={0} onClick={onClose}>
          {added === undefined ? "Not now" : "Done"}
        </button>
      </SettingActions>
    </div>
  );
}

/** How a file is named to the operator, and keyed: `<repo>/<path>`. */
function named(file: InstructionFile): string {
  return `${file.repo}/${file.file}`;
}

/** Whether `file`'s box is ticked: as the operator last pressed it, else ticked unless the core
 *  gave a caution — a long file, or one with invisible characters, starts unticked. */
function isTicked(file: InstructionFile, pressed: ReadonlyMap<string, boolean>): boolean {
  const caution = file.standing.kind === "offered" ? file.standing.caution : null;
  return pressed.get(named(file)) ?? caution === null;
}

/**
 * Zero-width characters, the bidirectional controls and the Unicode tag block — the characters
 * `purlis_core::repoinstructions::is_invisible` names. Text the operator approves goes into
 * every chat's briefing, so the preview draws each one as its code point instead of nothing.
 */
const INVISIBLE =
  /[\u00AD\u061C\u180E\u200B-\u200F\u202A-\u202E\u2060-\u2064\u2066-\u2069\uFEFF\u{E0000}-\u{E007F}]/u;

/** `text` with every invisible character drawn as a marked `U+XXXX`. */
function Visible({ text }: { text: string }) {
  const parts: React.ReactNode[] = [];
  let rest = text;
  let at = 0;
  for (let hit = INVISIBLE.exec(rest); hit !== null; hit = INVISIBLE.exec(rest)) {
    parts.push(rest.slice(0, hit.index));
    const code = (hit[0].codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, "0");
    parts.push(
      <mark key={at++} className="invisible-char" title="An invisible character">
        {`U+${code}`}
      </mark>,
    );
    rest = rest.slice(hit.index + hit[0].length);
  }
  parts.push(rest);
  return <>{parts}</>;
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
  const name = named(file);
  const { standing } = file;
  // An offered file is a row of the settings set (DS-3e): its name, its box, and the core's
  // caution as the row's help. One charter will not offer says why, in place of a box.
  return (
    <li>
      {standing.kind === "offered" ? (
        <SettingRow
          label={name}
          help={standing.caution ?? undefined}
          control={(ids) => (
            <Choice ids={ids} kind="toggle" checked={ticked} onCheckedChange={onTicked} />
          )}
        />
      ) : (
        <p className="came-back">
          {name}
          {": "}
          {standing.kind === "in-memory" ? "already in memory" : `left out; ${standing.why}`}
        </p>
      )}
      {file.text !== "" && (
        <pre className="repo-instruction-text">
          <Visible text={file.text} />
        </pre>
      )}
    </li>
  );
}
