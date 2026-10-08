import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { AnswerQuestion } from "./AnswerQuestion";

/**
 * **The form another surface mounts** (#1496): a task's row or its tab's menu names the task and
 * not its question, so the form asks the core which question the task is paused on.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function core(question: { task: string; asked: string; question: string } | null) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "task_question") return question;
    return null;
  });
  return (cmd: string) => asked.filter((one) => one.cmd === cmd).map((one) => one.args);
}

describe("the form that answers a task's question", () => {
  it("asks the core for the question where the surface gave none, and answers that one", async () => {
    const asked = core({ task: "talk", asked: "steward 3", question: "Which host?" });
    const onDone = vi.fn();
    render(
      <AnswerQuestion plane={PLANE} session={7} task="talk" onDone={onDone} onCancel={vi.fn()} />,
    );

    const form = await screen.findByRole("form", { name: "Answer talk's question" });
    expect(form).toHaveTextContent("Which host?");
    expect(asked("task_question")).toEqual([{ plane: PLANE, session: 7 }]);
    await userEvent.type(screen.getByRole("textbox", { name: "Your answer" }), "prod-2.{Enter}");

    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1));
    expect(asked("answer_task_question")).toEqual([
      { plane: PLANE, session: 7, question: "Which host?", text: "prod-2." },
    ]);
  });

  it("says so where the task has no question to answer, and offers nothing to send", async () => {
    const asked = core(null);
    const onCancel = vi.fn();
    render(
      <AnswerQuestion plane={PLANE} session={7} task="talk" onDone={vi.fn()} onCancel={onCancel} />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "talk has no question waiting for an answer now.",
    );
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.queryByRole("button", { name: "Send" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(asked("answer_task_question")).toEqual([]);
  });

  it("does not send while an answer is on its way", async () => {
    let taken: (() => void) | undefined;
    const asked: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "answer_task_question") return null;
      asked.push(args);
      return new Promise<null>((resolve) => {
        taken = () => resolve(null);
      });
    });
    const onDone = vi.fn();
    render(
      <AnswerQuestion
        plane={PLANE}
        session={7}
        task="talk"
        question="Which host?"
        onDone={onDone}
        onCancel={vi.fn()}
      />,
    );
    const box = screen.getByRole("textbox", { name: "Your answer" });
    await userEvent.type(box, "prod-2.{Enter}");
    await waitFor(() => expect(asked).toHaveLength(1));

    // A second press before the core answers sends nothing more.
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    expect(box).toBeDisabled();
    taken?.();

    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1));
    expect(asked).toHaveLength(1);
  });
});
