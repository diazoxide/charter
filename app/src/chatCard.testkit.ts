import { expect } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";

/**
 * Test-only: **a Chats row's card, as a person brings it up** (#1675): a popover with the
 * `tooltip` role, which is what these find it by.
 */

/** The card that is up, or nothing. */
export const card = () => screen.queryByRole("tooltip");

/** The card, once it has come up: after the rest it waits for. */
export const theCard = () => screen.findByRole("tooltip", {}, { timeout: 3000 });

/** What a card says of each of its facts, by the fact's word. */
export const facts = (shown: HTMLElement | null): Record<string, string | undefined> =>
  Object.fromEntries(
    [...(shown?.querySelectorAll("dt") ?? [])].map((term) => [
      term.textContent ?? "",
      term.nextElementSibling?.textContent ?? undefined,
    ]),
  );

/**
 * What `row`'s card says, brought up by a pointer resting on the row and taken down again once
 * read: its whole text, and its facts.
 */
export async function cardOf(row: HTMLElement): Promise<{
  text: string;
  facts: Record<string, string | undefined>;
}> {
  await userEvent.hover(row);
  const shown = await theCard();
  const read = { text: shown.textContent ?? "", facts: facts(shown) };
  await userEvent.unhover(row);
  await waitFor(() => expect(card()).toBeNull());
  return read;
}
