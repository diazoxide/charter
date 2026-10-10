import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { QuitWarning, type Ending } from "./QuitWarning";

afterEach(cleanup);

const ONE: Ending = {
  key: "/ide.1",
  name: "ide.1",
  harness: "claude",
  cwd: null,
  state: "waiting",
};

describe("the quit warning", () => {
  it("writes each answer's type, as every other dialog does (#1719)", () => {
    render(<QuitWarning chats={[ONE]} onQuit={() => {}} onCancel={() => {}} />);

    const dialog = screen.getByRole("dialog");
    for (const name of ["Cancel", "Quit purlis"])
      expect(within(dialog).getByRole("button", { name })).toHaveAttribute("type", "button");
  });
});
