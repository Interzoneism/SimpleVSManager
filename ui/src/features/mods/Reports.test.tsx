import { fireEvent, render, screen } from "@testing-library/react";
import type { VoteSummary } from "../../bindings";
import { Reports, verdictOf } from "./Reports";

const mutate = vi.fn();
const summary: VoteSummary = {
  fullyFunctional: 0, noIssuesSoFar: 33, someIssuesButWorks: 0, notFunctional: 1, crashesOrFreezesGame: 2,
  problemComments: ["Crashes on world load"], myVote: null, myComment: null, fetchedMs: 0,
};

vi.mock("../../api", () => ({
  useCloudStatus: () => ({ data: { offline: false, legacyIdentity: false, signInProblem: null } }),
  useModReports: () => ({ data: summary, isPending: false, isError: false }),
  useSubmitModReport: () => ({ mutate, reset: () => {}, isPending: false, isError: false, variables: undefined }),
}));

beforeEach(() => mutate.mockReset());

test("shows how many players say it works", () => {
  render(<Reports modId="carryon" version="1.2.3" />);
  expect(screen.getByText("92%")).toBeInTheDocument();
  expect(screen.getByText(/of 36 players say it works/)).toBeInTheDocument();
});

test("a good report is sent with one click", () => {
  render(<Reports modId="carryon" version="1.2.3" />);
  fireEvent.click(screen.getByRole("button", { name: "No issues" }));
  expect(mutate).toHaveBeenCalledWith(
    { modId: "carryon", version: "1.2.3", option: "noIssuesSoFar", comment: null },
    expect.anything(),
  );
});

test("a problem report asks what goes wrong first", () => {
  render(<Reports modId="carryon" version="1.2.3" />);
  fireEvent.click(screen.getByRole("button", { name: "Crashes" }));
  expect(mutate).not.toHaveBeenCalled();
  const send = screen.getByRole("button", { name: "Send report" });
  expect(send).toBeDisabled();
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Freezes when I open a chest" } });
  fireEvent.click(send);
  expect(mutate).toHaveBeenCalledWith(
    { modId: "carryon", version: "1.2.3", option: "crashesOrFreezesGame", comment: "Freezes when I open a chest" },
    expect.anything(),
  );
});

describe("verdictOf", () => {
  it("is the average answer, rounded, or nothing without reports", () => {
    expect(verdictOf([0, 0, 0, 0, 0])).toBeNull();
    expect(verdictOf([5, 0, 0, 0, 0])).toBe("fullyFunctional");
    expect(verdictOf([3, 3, 0, 0, 0])).toBe("noIssuesSoFar");
    expect(verdictOf([1, 0, 0, 0, 3])).toBe("notFunctional");
    expect(verdictOf([0, 0, 0, 0, 2])).toBe("crashesOrFreezesGame");
  });
});
