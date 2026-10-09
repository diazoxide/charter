import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Notice, NoticeBand } from "./Notice";

/**
 * **Where the summary of a time away stands in the band** (#1514, #1551): it leads the news
 * Notices, after only an Undo that lasts seconds and what the person just did, so it is not
 * behind "+N more" while news that waits stands in front of it. Trouble still comes first
 * (V91i).
 */

afterEach(cleanup);

const band = () =>
  [...document.querySelectorAll(".notice-band-shown [data-cause]")].map((one) =>
    one.getAttribute("data-cause"),
  );

const news = (cause: string) => (
  <Notice key={cause} cause={cause} onDismiss={() => undefined}>
    {cause}
  </Notice>
);

describe("the summary of a time away, in the band", () => {
  it("stands before news that waits, though it was drawn after it", () => {
    render(
      <NoticeBand>
        {news("pin-dormant:able")}
        {news("chat-resumed:1:conv-a")}
        {news("away-summary")}
      </NoticeBand>,
    );

    expect(band()).toEqual(["away-summary", "pin-dormant:able"]);
  });

  it("stands after an Undo that lasts seconds, and after trouble", () => {
    render(
      <NoticeBand>
        {news("away-summary")}
        {news("memory-deleted")}
        <Notice cause="window-trouble" tone="trouble" onDismiss={() => undefined}>
          trouble
        </Notice>
      </NoticeBand>,
    );

    expect(band()).toEqual(["window-trouble", "memory-deleted"]);
  });
});
