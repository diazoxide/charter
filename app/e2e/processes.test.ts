import { describe, expect, it } from "vitest";
import { descriptorsListed } from "./processes.js";

/**
 * How a macOS run counts the app's open descriptors: `lsof -F f`, whose `f` field is either a
 * descriptor number or a name for something that is not one (the working directory, the
 * program's text, a mapped library). Only the numbers are descriptors (SC-15).
 */
describe("the descriptors lsof lists", () => {
  it("are its numbered f fields, and not the cwd, txt or mapped entries", () => {
    const listed = ["p4242", "fcwd", "ftxt", "ftxt", "f0", "f1", "f2", "f17", "f256", ""].join(
      "\n",
    );
    expect(descriptorsListed(listed)).toBe(5);
  });

  it("are none when lsof listed nothing for the process", () => {
    expect(descriptorsListed("")).toBe(0);
  });
});
