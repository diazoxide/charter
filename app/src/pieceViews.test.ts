import { describe, expect, it } from "vitest";
import { pieceFileTitle, pieceFileView, pieceFilesView, pieceOf } from "./pieceViews";

const CUT = { workspace: "alpha", repo: "svc", piece: "fix-it" };

describe("a piece's files and one of its files are views named by data", () => {
  it("names the piece's files by its workspace, repo and piece", () => {
    expect(pieceOf(pieceFilesView(CUT))).toEqual({ place: CUT });
  });

  it("names one file by the piece and its path, slashes and all", () => {
    expect(pieceOf(pieceFileView(CUT, "src/a/b.rs"))).toEqual({ place: CUT, path: "src/a/b.rs" });
  });

  it("names a file of the repo's own folder, which has no piece (#948)", () => {
    const own = { workspace: "alpha", repo: "svc", piece: null };

    expect(pieceOf(pieceFileView(own, "src/a.rs"))).toEqual({ place: own, path: "src/a.rs" });
    expect(pieceOf(pieceFilesView(own))).toEqual({ place: own });
    expect(pieceFileTitle(own, "src/a.rs")).toBe("a.rs · svc");
  });

  it("is no piece's view for any other view", () => {
    expect(pieceOf({ from: null, view: "persona", key: "steward" })).toBeUndefined();
    expect(pieceOf({ from: "ext", view: "piece-files", key: "alpha/svc/fix-it" })).toBeUndefined();
    expect(pieceOf({ from: null, view: "piece-file", key: "alpha/svc" })).toBeUndefined();
  });
});
