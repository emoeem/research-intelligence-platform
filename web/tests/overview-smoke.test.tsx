import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import i18n from "../src/i18n";

const OVERVIEW_STUB = {
  knowledge: { total: 46, by_verified: { verified: 37, unverified: 9 } },
  tasks: { total: 42, by_state: { ACTIVE: 6, NEEDS_REVIEW: 3, COMPLETED: 33 } },
  evidence: { total: 700 },
  runs: { available: true, total: 3691, by_status: { SUCCEEDED: 3661, FAILED: 30 } },
};

const STATS_STUB = {
  reviews: {
    total: 165,
    by_queue: {
      d5: { total: 68, by_decision: { PENDING: 2, ACCEPTED: 66 } },
      license: { total: 79, by_decision: { PENDING: 1, ACCEPTED: 78 } },
      mapping: { total: 18, by_decision: { PENDING: 0, ACCEPTED: 18 } },
    },
  },
};

const RUNTIME_STUB = {
  available: true,
  total: 2,
  items: [
    {
      node_id: "node-python-01",
      kind: "python_worker",
      health: "ok",
      last_heartbeat_at: "2026-11-24T06:00:00Z",
      heartbeat_age_seconds: 8,
    },
    {
      node_id: "node-podman-01",
      kind: "podman_host",
      health: "degraded",
      last_heartbeat_at: "2026-11-24T05:59:00Z",
      heartbeat_age_seconds: 68,
    },
  ],
};

const RECENT_RUNS_STUB = {
  available: true,
  total: 2,
  items: [
    {
      run_id: "run-latest-failed",
      tool_id: "rip.http.inspect",
      status: "FAILED",
      error_category: "timeout",
      duration_ms: 42,
      created_at: "2026-11-24T06:00:00Z",
    },
    {
      run_id: "run-previous-success",
      tool_id: "rip.core.validate",
      status: "SUCCEEDED",
      duration_ms: 18,
      created_at: "2026-11-24T05:50:00Z",
    },
  ],
};

type FetchMode = "ok" | "pending" | "error";
let fetchMode: FetchMode = "ok";

const RESPONSE_BY_PATH: Record<string, unknown> = {
  "/api/overview": OVERVIEW_STUB,
  "/api/stats": STATS_STUB,
  "/api/runtime": RUNTIME_STUB,
  "/api/runs": RECENT_RUNS_STUB,
};

function stubFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      if (fetchMode === "pending") return new Promise(() => {});
      if (fetchMode === "error") {
        return Promise.resolve(
          new Response(JSON.stringify({ error: { code: "internal", message: "internal error" } }), {
            status: 500,
          }),
        );
      }

      const path = String(input).split("?")[0] ?? "";
      return Promise.resolve(
        new Response(JSON.stringify(RESPONSE_BY_PATH[path] ?? {}), {
          status: 200,
          headers: { "content-type": "application/json; charset=utf-8" },
        }),
      );
    }),
  );
}

beforeEach(async () => {
  localStorage.clear();
  await i18n.changeLanguage("en-US");
  fetchMode = "ok";
  stubFetch();
  window.history.pushState({}, "", "/");
});

afterEach(async () => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
  localStorage.clear();
  await i18n.changeLanguage("zh-CN");
});

describe("Dashboard reference layout", () => {
  it("renders four real-data KPI cards and links to the corresponding workspaces", async () => {
    render(<App />);

    expect(await screen.findByRole("link", { name: /Knowledge items/ })).toBeTruthy();
    expect(screen.getByRole("link", { name: /Research tasks/ })).toBeTruthy();
    expect(screen.getByRole("link", { name: /Tool runs/ })).toBeTruthy();
    expect(screen.getByRole("link", { name: /Evidence records/ })).toBeTruthy();

    expect(screen.getByText("46")).toBeTruthy();
    expect(screen.getByText("42")).toBeTruthy();
    expect(screen.getByText("3691")).toBeTruthy();
    expect(screen.getByText("700")).toBeTruthy();

    expect(screen.getByRole("link", { name: /Knowledge items/ }).getAttribute("href")).toBe(
      "/knowledge",
    );
    expect(screen.getByRole("link", { name: /Research tasks/ }).getAttribute("href")).toBe(
      "/research/tasks",
    );
    expect(screen.getByRole("link", { name: /Tool runs/ }).getAttribute("href")).toBe("/runs");
    expect(screen.getByRole("link", { name: /Evidence records/ }).getAttribute("href")).toBe(
      "/research/evidence",
    );
  });

  it("shows only pending review counts in Needs attention", async () => {
    render(<App />);

    expect(await screen.findByRole("heading", { name: "Needs attention" })).toBeTruthy();
    expect(screen.getByText("3 pending")).toBeTruthy();
    expect(screen.getByText("D5 duplicate pairs")).toBeTruthy();
    expect(screen.getByText("License review")).toBeTruthy();
    expect(screen.queryByText("Mapping review")).toBeNull();
    expect(screen.getByText(/2 items pending/)).toBeTruthy();
    expect(screen.getByText(/1 items pending/)).toBeTruthy();
  });

  it("shows runtime nodes and their actual health/heartbeat state", async () => {
    render(<App />);

    const runtime = await screen.findByRole("heading", { name: "Runtime health" });
    expect(runtime).toBeTruthy();
    expect(screen.getByText(/node-python-01/)).toBeTruthy();
    expect(screen.getByText(/node-podman-01/)).toBeTruthy();
    expect(screen.getByText("Python Worker")).toBeTruthy();
    expect(screen.getByText("Podman Host")).toBeTruthy();
    expect(screen.getByText("Healthy")).toBeTruthy();
    expect(screen.getByText("Degraded")).toBeTruthy();
  });

  it("shows recent runs from the read-only runs API and links to the run details", async () => {
    render(<App />);

    expect(await screen.findByRole("heading", { name: "Recent activity" })).toBeTruthy();
    const failed = await screen.findByRole("link", { name: /rip\.http\.inspect/ });
    expect(failed.getAttribute("href")).toBe("/runs/run-latest-failed");
    expect(screen.getByText(/timeout/)).toBeTruthy();
    expect(screen.getByText("rip.core.validate")).toBeTruthy();
  });

  it("uses a single time-aware greeting rather than a duplicate page title", async () => {
    render(<App />);
    const heading = await screen.findByRole("heading", { level: 1 });
    expect(["Good morning", "Good afternoon", "Good evening"]).toContain(heading.textContent);
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(
      screen.getByText("Research workspace overview · Activity and governance at a glance"),
    ).toBeTruthy();
  });

  it("Loading state keeps the Dashboard shell while API data is pending", () => {
    fetchMode = "pending";
    render(<App />);
    expect(document.querySelector(".skeleton")).toBeTruthy();
  });

  it("Error state uses the retryable ErrorPanel and reloads the Dashboard data", async () => {
    fetchMode = "error";
    render(<App />);
    expect(await screen.findByText("Failed to load")).toBeTruthy();

    const retry = screen.getByRole("button", { name: /Retry/ });
    fetchMode = "ok";
    fireEvent.click(retry);

    expect(await screen.findByText("3691")).toBeTruthy();
  });

  it("Empty state keeps all dashboard panels and shows accurate empty messages", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input).split("?")[0] ?? "";
        const bodies: Record<string, unknown> = {
          "/api/overview": {
            knowledge: { total: 0, by_verified: {} },
            tasks: { total: 0, by_state: {} },
            evidence: { total: 0 },
            runs: { available: true, total: 0, by_status: {} },
          },
          "/api/stats": { reviews: { total: 0, by_queue: {} } },
          "/api/runtime": {
            available: true,
            total: 0,
            items: [],
            message: "No registered runtime nodes.",
          },
          "/api/runs": { available: true, total: 0, items: [] },
        };
        return new Response(JSON.stringify(bodies[path] ?? {}), {
          status: 200,
          headers: { "content-type": "application/json; charset=utf-8" },
        });
      }),
    );
    render(<App />);

    expect(await screen.findByText("No pending review items.")).toBeTruthy();
    expect(screen.getByText("No registered runtime nodes.")).toBeTruthy();
    expect(screen.getByText("No recent activity yet.")).toBeTruthy();
    expect(screen.getAllByText("0").length).toBeGreaterThanOrEqual(4);
  });
});
