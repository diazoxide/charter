import { describe, expect, it } from "vitest";
import type { ChatRow } from "./chatsTree";
import type { Line } from "./tabTasks";
import { askOf, tokensOfLine, tokensSaid } from "./tasksUsed";

function line(key: string, more: Partial<Line> = {}): Line {
  return {
    key,
    name: key,
    persona: null,
    level: 2,
    askedBy: null,
    elsewhere: null,
    current: false,
    ...more,
  };
}

const own = { session: 1, level: 1 } as ChatRow;
const task = { session: 2, level: 2 } as ChatRow;

describe("what a tab's menu asks of the core (#1500)", () => {
  it("names the session's own chat, its open tasks, and its ended ones by their record", () => {
    const ask = askOf([
      line("chat:1", { session: 1, level: 1, row: own }),
      line("chat:2", { session: 2, row: task }),
      line("finished:abc"),
      // An ended task the tab still shows, whose finished row is not read yet.
      line("ended:7", { session: 7 }),
    ]);

    expect(ask).toEqual({ own: 1, chats: [2, 7], finished: ["abc"] });
  });

  it("finds a line's figure by its chat or its record, and nothing before a read", () => {
    const used = {
      chats: [{ session: 2, tokens: "1k in, 200 out", unsaid: null }],
      finished: [{ id: "abc", tokens: null, unsaid: "nothing" as const }],
      total: null,
    };

    expect(tokensOfLine(used, line("chat:2", { session: 2, row: task }))?.tokens).toBe(
      "1k in, 200 out",
    );
    expect(tokensOfLine(used, line("finished:abc"))).toMatchObject({
      tokens: null,
      unsaid: "nothing",
    });
    expect(tokensOfLine(undefined, line("finished:abc"))).toBeUndefined();
  });

  it("says a missing figure as a dash and why, true in every case, never a zero", () => {
    // No turn has ended yet, or a harness that reports none: its conversation is known.
    expect(tokensSaid({ tokens: null, unsaid: "not_yet" })).toBe(
      "Tokens: — (nothing reported yet)",
    );
    // A task that ended with no figure kept.
    expect(tokensSaid({ tokens: null, unsaid: "nothing" })).toBe("Tokens: — (nothing reported)");
    // An unknown conversation, or a record that could not be read.
    expect(tokensSaid({ tokens: null, unsaid: "not_known" })).toBe("Tokens: — (not known)");
    expect(tokensSaid({ tokens: "15k in, 4k out", unsaid: null })).toBe("Tokens: 15k in, 4k out");
    expect(tokensSaid(undefined)).toBeUndefined();
  });
});
