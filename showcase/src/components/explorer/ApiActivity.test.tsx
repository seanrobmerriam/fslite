import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { ActivityRecord } from "../../lib/shared/contracts";
import { ApiActivity } from "./ApiActivity";

const entry: ActivityRecord = {
  id: "activity-1",
  timestamp: "2026-08-22T00:00:00.000Z",
  method: "POST",
  path: "/v1/files/readme.txt",
  status: 201,
  durationMs: 42,
  requestId: "request-1",
  request: { path: "/readme.txt", authorization: "Bearer secret" },
  response: { truncated: true, headers: { server: "private" }, ok: true },
  curl: "curl -H 'Authorization: Bearer secret' http://fslite-server:8080/v1/files/readme.txt",
};

describe("ApiActivity", () => {
  it("shows the sanitized request summary, bounded notice, details, and clears only local history", async () => {
    const user = userEvent.setup();
    const clear = vi.fn();
    render(<ApiActivity activities={[entry]} onClear={clear} />);
    expect(screen.getByText("POST /v1/files/readme.txt")).toBeInTheDocument();
    expect(screen.getByText("201 · 42 ms · request-1")).toBeInTheDocument();
    expect(screen.getByText(/truncated or bounded/i)).toBeInTheDocument();
    expect(
      screen.queryByText(/secret|fslite-server|private/i),
    ).not.toBeInTheDocument();
    await user.click(screen.getByText(/POST \/v1\/files\/readme.txt/));
    expect(screen.getByText(/\$FSLITE_TOKEN/)).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Clear local activity" }),
    );
    expect(clear).toHaveBeenCalledTimes(1);
  });

  it("copies only the sanitized curl and reports clipboard success or failure", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    render(<ApiActivity activities={[entry]} onClear={vi.fn()} />);
    await user.click(screen.getByText(/POST \/v1\/files\/readme.txt/));
    await user.click(screen.getByRole("button", { name: "Copy curl" }));
    expect(writeText).toHaveBeenCalledWith(
      expect.stringContaining("$FSLITE_TOKEN"),
    );
    expect(writeText).not.toHaveBeenCalledWith(
      expect.stringContaining("secret"),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(/copied/i);
  });

  it("renders only the most recent 100 local activity entries", () => {
    const activities = Array.from({ length: 101 }, (_, index) => ({
      ...entry,
      id: String(index),
      path: `/v1/files/${index}.txt`,
    }));
    render(<ApiActivity activities={activities} onClear={vi.fn()} />);
    expect(screen.queryByText("POST /v1/files/0.txt")).not.toBeInTheDocument();
    expect(screen.getByText("POST /v1/files/100.txt")).toBeInTheDocument();
  });
});
