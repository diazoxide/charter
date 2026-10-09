import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Palette } from "./Palette";
import type { Offer, Ran } from "./actions";
import type { FileScope, FilesFound, FoundFile, PlaneId } from "./bindings";
import { scopeLadder } from "./fileFind";
import { forgetProjectThemes } from "./projectTheme";

/**
 * **⌘P's files in the palette** (FM-7): the render states the real-app scenario
 * (`workspace-explorer.e2e.ts`) does not reach on its fixture — a branch the core could not
 * list, nothing matching, a lone project, and the two groups kept apart. The core is mocked
 * here; what it finds on a real branch is `a_file_is_found_by_fuzzy_name.rs`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  forgetProjectThemes();
});

const PLANE = "/projects/alpha" as unknown as PlaneId;
const OTHER = "/projects/beta" as unknown as PlaneId;
const BRANCH = { workspace: "web", repo: "svc", piece: "fix-login" };

const switchTo = (plane: string, available: boolean): Offer => ({
  id: `project.select:${plane}`,
  title: `Switch to project ${plane.split("/").pop()}`,
  name: plane.split("/").pop(),
  available,
  reason: available ? "" : "It is already in front.",
  does: { verb: "selectProject", plane },
});

const file = (path: string, plane: PlaneId = PLANE, matched: number[] = []): FoundFile => ({
  plane,
  workspace: BRANCH.workspace,
  repo: BRANCH.repo,
  piece: BRANCH.piece,
  path,
  matched,
});

/** The core, answering `find_files` with `answer(scope, query)` and recording each ask; any
 *  other command is answered by `rest`, or with `null`. */
function core(
  answer: (scope: FileScope, query: string) => FilesFound,
  rest: (cmd: string, args: unknown) => unknown = () => null,
) {
  const asked: { scope: FileScope; query: string }[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "find_files") {
      const { scope, query } = args as { scope: FileScope; query: string };
      asked.push({ scope, query });
      return answer(scope, query);
    }
    return rest(cmd, args);
  });
  return asked;
}

const ok = (): Ran => ({ ok: true });

/** Renders the palette and opens it as ⌘P does, through the switcher's count. */
function opened(rows: Offer[], onOpen = vi.fn(), open = 2) {
  const files = {
    ladder: scopeLadder(PLANE, BRANCH, open),
    nameOf: (plane: PlaneId) => plane.split("/").pop() ?? plane,
    onOpen,
  };
  const view = render(
    <Palette offers={[]} projects={{ rows, asked: 0 }} files={files} onRun={ok} />,
  );
  view.rerender(<Palette offers={[]} projects={{ rows, asked: 1 }} files={files} onRun={ok} />);
  return onOpen;
}

describe("⌘P's files", () => {
  it("are a group of their own after the projects, and the arrows run on into them", async () => {
    core(() => ({ files: [file("src/beta.ts")], branches: 1, refused: [], partial: [] }));
    const onOpen = opened([switchTo(PLANE, false), switchTo(OTHER, true)]);

    await userEvent.keyboard("beta");

    const projects = screen.getByRole("listbox", { name: "Projects" });
    const files = await screen.findByRole("listbox", { name: "Files" });
    expect(
      within(projects)
        .getAllByRole("option")
        .map((row) => row.textContent),
    ).toEqual(["beta"]);
    expect(within(files).getByRole("option").textContent).toContain("beta.ts");
    // Enter is on the project, which runs; one down is the file.
    expect(within(projects).getByRole("option").getAttribute("aria-selected")).toBe("true");
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(onOpen).toHaveBeenCalledWith(file("src/beta.ts"));
  });

  it("say where each file is: its folder, its branch and its project", async () => {
    core(() => ({
      files: [file("src/a/login.ts"), file("login.ts", OTHER)],
      branches: 2,
      refused: [],
      partial: [],
    }));
    opened([switchTo(PLANE, false), switchTo(OTHER, true)]);

    await userEvent.keyboard("login");

    const rows = within(await screen.findByRole("listbox", { name: "Files" })).getAllByRole(
      "option",
    );
    expect(rows.map((row) => row.textContent)).toEqual([
      "login.tssrc/a · fix-login · alpha",
      "login.tsfix-login · beta",
    ]);
  });

  it("mark the letters the query matched, in the name and in the folder (#1131)", async () => {
    // `s` of src, then `login`: characters 0 and 6 to 10 of `src/a/login.ts`.
    core(() => ({
      files: [
        file("src/a/login.ts", PLANE, [0, 6, 7, 8, 9, 10]),
        file("café/menü.md", PLANE, [5, 7]),
      ],
      branches: 1,
      refused: [],
      partial: [],
    }));
    opened([switchTo(PLANE, false)]);

    await userEvent.keyboard("slogin");

    const [login, menu] = within(
      await screen.findByRole("listbox", { name: "Files" }),
    ).getAllByRole("option");
    const marks = (row: HTMLElement) =>
      [...row.querySelectorAll("mark")].map((mark) => mark.textContent);
    expect(marks(login)).toEqual(["login", "s"]);
    expect(login.textContent).toBe("login.tssrc/a · fix-login · alpha");
    // Counted in characters, as the core counts them: `é` is one, though it is two bytes.
    expect(marks(menu)).toEqual(["m", "n"]);
  });

  it("draw each file's icon in the icon theme of the project it was found in (#1145)", async () => {
    const square = { viewBox: "0 0 16 16", paths: [{ d: "M0 0h16v16H0z", tone: "icon.pink" }] };
    const seti = {
      extension: "seti",
      name: "Seti",
      text: JSON.stringify({
        name: "Seti",
        symbols: { "seti-file": square, "seti-folder": square, "seti-md": square },
        file: "seti-file",
        folder: "seti-folder",
        extensions: { md: "seti-md" },
      }),
    };
    core(
      () => ({
        files: [file("docs/README.md"), file("README.md", OTHER), file("src/lib.rs", OTHER)],
        branches: 2,
        refused: [],
        partial: [],
      }),
      (cmd, args) => {
        // Only the first project picks the contributed theme, and only in its workspace.
        if (cmd === "project_icons_drawn") {
          const { plane, workspace } = args as { plane: string; workspace: string | null };
          return plane === PLANE && workspace === BRANCH.workspace ? "seti/Seti" : null;
        }
        if (cmd === "extension_icon_themes") return [seti];
        return null;
      },
    );
    opened([switchTo(PLANE, false), switchTo(OTHER, true)]);

    await userEvent.keyboard("r");

    const files = await screen.findByRole("listbox", { name: "Files" });
    const icons = () =>
      within(files)
        .getAllByRole("option")
        .map((row) => row.querySelector("svg.file-icon")?.getAttribute("data-icon"));
    await vi.waitFor(() => expect(icons()).toEqual(["seti-md", "readme", "rust"]));
    // Decorative: the row is still named by its words alone.
    expect(within(files).getAllByRole("option")[0].textContent).toBe(
      "README.mddocs · fix-login · alpha",
    );
  });

  it("show their scope, which Tab widens and Shift+Tab narrows", async () => {
    const asked = core(() => ({ files: [], branches: 1, refused: [], partial: [] }));
    opened([switchTo(PLANE, false)]);

    expect(screen.getByText(/Files in branch fix-login/)).toBeTruthy();
    await userEvent.keyboard("{Tab}");
    expect(screen.getByText(/Files in project alpha/)).toBeTruthy();
    await userEvent.keyboard("{Tab}");
    expect(screen.getByText(/Files in all open projects/)).toBeTruthy();
    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.getByText(/Files in project alpha/)).toBeTruthy();
    await userEvent.keyboard("x");
    await act(async () => {});
    // The project rung keeps the picked branch first.
    expect(asked.at(-1)).toEqual({
      scope: { kind: "project", plane: PLANE, near: BRANCH },
      query: "x",
    });
    // The scope was listed as the palette opened, before anything was typed.
    expect(asked[0]).toEqual({ scope: { kind: "branch", plane: PLANE, ...BRANCH }, query: "" });
  });

  it("say so when nothing matches, and say a branch the core could not list in its words", async () => {
    core(() => ({
      files: [],
      branches: 2,
      refused: ["svc in workspace 'web' has no branch folder called 'gone'"],
      partial: [],
    }));
    opened([switchTo(PLANE, false)]);

    await userEvent.keyboard("zzz");

    expect(await screen.findByText("No file matches what you typed.")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toBe(
      "svc in workspace 'web' has no branch folder called 'gone'",
    );
  });

  it("leave a lone project's own row out: it could only say it is in front", async () => {
    core(() => ({ files: [file("alpha.md")], branches: 1, refused: [], partial: [] }));
    const onOpen = opened([switchTo(PLANE, false)]);

    await userEvent.keyboard("alpha");

    expect(screen.queryByRole("listbox", { name: "Projects" })).toBeNull();
    await screen.findByRole("listbox", { name: "Files" });
    await userEvent.keyboard("{Enter}");
    expect(onOpen).toHaveBeenCalledWith(file("alpha.md"));
  });

  it("have no rung for every open project while only one is open", async () => {
    core(() => ({ files: [], branches: 1, refused: [], partial: [] }));
    opened([switchTo(PLANE, false)], vi.fn(), 1);

    const scope = screen.getByRole("status", { name: "Where files are found" });
    expect(scope.textContent).toContain("Files in branch fix-login");
    await userEvent.keyboard("{Tab}");
    expect(scope.textContent).toContain("Files in project alpha");
    await userEvent.keyboard("{Tab}");
    expect(scope.textContent).toContain("Files in branch fix-login");
  });

  it("say a branch listed only in part, in the core's words", async () => {
    core(() => ({
      files: [file("a.md")],
      branches: 1,
      refused: [],
      partial: [
        "fix-login has 250000 files, and only the first 200000 of them are searched by name",
      ],
    }));
    opened([switchTo(PLANE, false)]);

    await userEvent.keyboard("a");

    expect(
      await screen.findByText(
        "fix-login has 250000 files, and only the first 200000 of them are searched by name",
      ),
    ).toBeTruthy();
  });
});
