/**
 * **The light editor's grammars: a fixed set charter ships** (ADR 0081 §2, its *Decided in
 * drafting* 9). A grammar is code, so no extension, pack or persona adds one; a language that
 * is not here is drawn as plain text, and only a charter release adds one.
 *
 * Each grammar is its own chunk, fetched the first time a file of its language opens, so a
 * window that never opens a Go file never loads Go's parser (ADR 0086 row M2).
 */
import type { StreamParser } from "@codemirror/language";
import type { Extension } from "@codemirror/state";

type Load = () => Promise<Extension>;

/** A grammar from `@codemirror/legacy-modes`, for a language with no Lezer parser. */
const legacy =
  (load: () => Promise<StreamParser<unknown>>): Load =>
  async () => {
    const [{ StreamLanguage }, mode] = await Promise.all([import("@codemirror/language"), load()]);
    return StreamLanguage.define(mode);
  };

const javascript =
  (options: { typescript?: boolean; jsx?: boolean }): Load =>
  async () =>
    (await import("@codemirror/lang-javascript")).javascript(options);

/** By the file's extension, lower-cased, without its dot. */
const BY_EXTENSION: Record<string, Load> = {
  js: javascript({}),
  mjs: javascript({}),
  cjs: javascript({}),
  jsx: javascript({ jsx: true }),
  ts: javascript({ typescript: true }),
  mts: javascript({ typescript: true }),
  cts: javascript({ typescript: true }),
  tsx: javascript({ typescript: true, jsx: true }),
  rs: async () => (await import("@codemirror/lang-rust")).rust(),
  py: async () => (await import("@codemirror/lang-python")).python(),
  json: async () => (await import("@codemirror/lang-json")).json(),
  jsonc: async () => (await import("@codemirror/lang-json")).json(),
  md: async () => (await import("@codemirror/lang-markdown")).markdown(),
  markdown: async () => (await import("@codemirror/lang-markdown")).markdown(),
  html: async () => (await import("@codemirror/lang-html")).html(),
  htm: async () => (await import("@codemirror/lang-html")).html(),
  css: async () => (await import("@codemirror/lang-css")).css(),
  yaml: async () => (await import("@codemirror/lang-yaml")).yaml(),
  yml: async () => (await import("@codemirror/lang-yaml")).yaml(),
  go: async () => (await import("@codemirror/lang-go")).go(),
  toml: legacy(async () => (await import("@codemirror/legacy-modes/mode/toml")).toml),
  sh: legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
  bash: legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
  zsh: legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
};

/** Files known by their whole name rather than an extension. */
const BY_NAME: Record<string, Load> = {
  "Cargo.lock": BY_EXTENSION.toml,
  Dockerfile: legacy(
    async () => (await import("@codemirror/legacy-modes/mode/dockerfile")).dockerFile,
  ),
};

/** The grammar for a path, or `undefined` for plain text. */
export function grammarFor(path: string): Load | undefined {
  const name = path.slice(path.lastIndexOf("/") + 1);
  if (BY_NAME[name]) return BY_NAME[name];
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return undefined;
  return BY_EXTENSION[name.slice(dot + 1).toLowerCase()];
}
