import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Joins class names, with the later of two conflicting Tailwind utilities winning.
 *
 * This is shadcn/ui's `cn`, at shadcn/ui's address, with shadcn/ui's two dependencies, because
 * a component pasted from shadcn/ui expects to find it here and rewriting it would be the
 * custom tooling `AGENTS.md` forbids. It is the whole of the shadcn "library" that exists
 * ahead of a component needing it; see `docs/design-system.md` for what else is adopted and
 * what is deliberately not.
 *
 * `clsx` alone would be half of it. The half that matters is `twMerge`: `clsx("p-2", "p-4")`
 * gives `"p-2 p-4"` and the winner is then whichever Tailwind happened to emit last, which is
 * a fact about the generated stylesheet and not about the call. `cn` makes the call decide.
 */
export function cn(...classes: ClassValue[]): string {
  return twMerge(clsx(classes));
}
