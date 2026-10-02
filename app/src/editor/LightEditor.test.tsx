import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { LightEditor, MergeViewer } from "./LightEditor";
import { gitHunks } from "./gitdiff.testkit";

afterEach(cleanup);

/** The text of each line an editor draws, in order. */
function lines(root: Element): string[] {
  return [...root.querySelectorAll(".cm-line")].map((line) => line.textContent ?? "");
}

describe("the light editor", () => {
  it("draws a file's text, a line to a line", async () => {
    const { container } = render(
      <LightEditor path="src/lib.rs" text={"pub fn one() -> u8 {\n    1\n}\n"} />,
    );

    await waitFor(() => expect(lines(container)).toContain("pub fn one() -> u8 {"));
    expect(lines(container)).toEqual(["pub fn one() -> u8 {", "    1", "}", ""]);
  });

  it("is for reading: the text cannot be typed into", async () => {
    const { container } = render(<LightEditor path="notes.txt" text={"one\n"} />);

    await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
    expect(container.querySelector(".cm-content")).toHaveAttribute("aria-readonly", "true");
  });

  it("colours a language it ships a grammar for", async () => {
    const { container } = render(<LightEditor path="src/lib.rs" text={"pub fn one() {}\n"} />);

    // The grammar is its own chunk, so the highlighting arrives after the text.
    await waitFor(() =>
      expect(container.querySelectorAll(".cm-line span").length).toBeGreaterThan(0),
    );
  });
});

describe("the merge view", () => {
  it("renders a diff: the base beside the head, with git's changed lines marked", async () => {
    const base = "one\ntwo\nthree\n";
    const head = "one\nTWO\nthree\nfour\n";

    const { container } = render(
      <MergeViewer path="notes.txt" base={base} head={head} hunks={gitHunks(base, head)} />,
    );

    await waitFor(() => expect(container.querySelector(".cm-mergeView")).not.toBeNull());
    const [a, b] = [...container.querySelectorAll(".cm-editor")];
    expect(lines(a)).toEqual(["one", "two", "three", ""]);
    expect(lines(b)).toEqual(["one", "TWO", "three", "four", ""]);
    const changed = (side: Element) =>
      [...side.querySelectorAll(".cm-changedLine")].map((line) => line.textContent);
    expect(changed(a)).toEqual(["two"]);
    expect(changed(b)).toEqual(["TWO", "four"]);
  });

  it("marks nothing git did not report, even where the texts differ", async () => {
    const { container } = render(
      <MergeViewer path="notes.txt" base={"one\n"} head={"two\n"} hunks={[]} />,
    );

    await waitFor(() => expect(container.querySelector(".cm-mergeView")).not.toBeNull());
    expect(container.querySelectorAll(".cm-changedLine")).toHaveLength(0);
  });
});
