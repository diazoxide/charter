import { describe, expect, it } from "vitest";
import { pieceFileView, pieceFilesView, pieceOf } from "./pieceViews";

const CUT = { workspace: "alpha", repo: "svc", piece: "fix-it" };

describe("a piece's files and one of its files are views named by data", () => {
  it("names the piece's files by its workspace, repo and piece", () => {
    expect(pieceOf(pieceFilesView(CUT))).toEqual({ cut: CUT });
  });

  it("names one file by the piece and its path, slashes and all", () => {
    expect(pieceOf(pieceFileView(CUT, "src/a/b.rs"))).toEqual({ cut: CUT, path: "src/a/b.rs" });
  });

  it("is no piece's view for any other view", () => {
    expect(pieceOf({ from: null, view: "persona", key: "steward" })).toBeUndefined();
    expect(pieceOf({ from: "ext", view: "piece-files", key: "alpha/svc/fix-it" })).toBeUndefined();
    expect(pieceOf({ from: null, view: "piece-file", key: "alpha/svc" })).toBeUndefined();
  });
});
