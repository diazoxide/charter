import { describe, expect, it } from "vitest";
import { wordsOutsideTheFirstHour } from "./firstHour";

describe("the first hour's words (ADR 0072 §3)", () => {
  it("finds a listed word, its plural, and nothing inside another word", () => {
    expect(wordsOutsideTheFirstHour("Pick a harness")).toEqual(["harness"]);
    expect(wordsOutsideTheFirstHour("two runs")).toEqual(["run"]);
    expect(wordsOutsideTheFirstHour("Start the first chat. It changed nothing.")).toEqual([]);
    expect(wordsOutsideTheFirstHour("a truncated line")).toEqual([]);
  });

  it("matches LIVE and LOCAL only as the shouted labels", () => {
    expect(wordsOutsideTheFirstHour("on this machine, local only")).toEqual([]);
    expect(wordsOutsideTheFirstHour("the workspace is LIVE")).toEqual(["LIVE"]);
  });
});
