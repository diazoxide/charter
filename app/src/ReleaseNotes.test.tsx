import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ReleaseNotes } from "./ReleaseNotes";

/**
 * A changelog section is Markdown and is drawn as Markdown. The defect this is for: About
 * printed an entry's body as one paragraph, `- **Switching tabs errored.** Pressing a tab…`,
 * list markers and asterisks included.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const SECTION = `### Added

- The **first** thing, with \`code\`. ([#7](https://github.com/diazoxide/charter-app/pull/7))
- The second thing.

### Fixed

- A [link that is not the web](file:///etc/passwd), drawn as its words.
- <img src=x onerror="alert(1)"> raw HTML is dropped.`;

describe("ReleaseNotes", () => {
  it("draws a bullet list as a list and bold as bold", () => {
    const { container } = render(<ReleaseNotes markdown={SECTION} />);

    const items = container.querySelectorAll("li");
    expect(items).toHaveLength(4);
    expect(within(items[0] as HTMLElement).getByText("first").tagName).toBe("STRONG");
    expect(within(items[0] as HTMLElement).getByText("code").tagName).toBe("CODE");
    expect(container.textContent).not.toContain("**");
    expect(container.textContent).not.toContain("- The");
  });

  it("draws a section heading one level below the surface's own heading", () => {
    render(<ReleaseNotes markdown={SECTION} />);

    expect(screen.getByRole("heading", { name: "Added", level: 4 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Fixed", level: 4 })).toBeInTheDocument();
  });

  it("opens a link in the browser rather than inside the window", async () => {
    const opened: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "plugin:opener|open_url") opened.push((args as { url: string }).url);
      return null;
    });
    render(<ReleaseNotes markdown={SECTION} />);

    const link = screen.getByRole("link", { name: "#7" });
    expect(link).toHaveAttribute("tabindex", "0");
    await userEvent.click(link);

    await vi.waitFor(() =>
      expect(opened).toEqual(["https://github.com/diazoxide/charter-app/pull/7"]),
    );
  });

  it("draws a link to anything but the web as its words, and no raw HTML at all", () => {
    const { container } = render(<ReleaseNotes markdown={SECTION} />);

    expect(screen.queryByRole("link", { name: "link that is not the web" })).toBeNull();
    expect(screen.getByText(/link that is not the web/)).toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).not.toContain("onerror");
  });
});
