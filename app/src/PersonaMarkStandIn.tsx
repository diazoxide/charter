/**
 * **A persona's mark, standing in** (#1447) until the persona icon lands (#1449): its initials.
 * The same name and props as that component (`PersonaMark`, the persona and a class), so the
 * swap is the import in the two files that draw it and nothing else.
 *
 * Hidden from a screen reader and named in a tooltip, as that one is: every row that draws it
 * says the chat's name in words beside it. The initials are drawn by the stylesheet, so they
 * are no part of the row's text.
 */
export function PersonaMark({ persona, className }: { persona: string; className?: string }) {
  return (
    <span
      className={className ? `persona-initials ${className}` : "persona-initials"}
      data-initials={initialsOf(persona)}
      title={persona}
      aria-hidden="true"
    />
  );
}

/** `platform-steward` is `PS` and `steward` is `ST`: two letters, from two words where there are. */
function initialsOf(persona: string): string {
  const words = persona.split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  const letters =
    words.length > 1
      ? [...words[0]][0] + [...words[1]][0]
      : [...(words[0] ?? "")].slice(0, 2).join("");
  return letters.toUpperCase();
}
