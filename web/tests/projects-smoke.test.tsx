import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

const project = {
  project_id: "prj-test",
  workspace_id: "default",
  name: "Kernel review",
  slug: "kernel-review",
  summary: "",
  objective: "Review kernel interfaces",
  status: "active",
  priority: "high",
  scope: {},
  tags: [],
  owner_id: "test",
  created_at: "2026-11-24T10:00:00Z",
  updated_at: "2026-11-24T10:00:00Z",
};
const asset = {
  asset_id: "ast-test",
  workspace_id: "default",
  asset_type: "domain",
  canonical_value: "example.test",
  display_name: "Example target",
  description: "",
  lifecycle_status: "active",
  first_seen_at: "2026-11-24T10:00:00Z",
  last_seen_at: "2026-11-24T10:00:00Z",
  created_at: "2026-11-24T10:00:00Z",
  updated_at: "2026-11-24T10:00:00Z",
};

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = init?.method ?? "GET";
      if (url.includes("/api/projects") && method === "POST") {
        return new Response(JSON.stringify(project), {
          status: 201,
          headers: { "Content-Type": "application/json" },
        });
      }
      if (url.includes("/api/research/assets") && method === "POST") {
        return new Response(JSON.stringify(asset), {
          status: 201,
          headers: { "Content-Type": "application/json" },
        });
      }
      if (url.includes("/api/projects")) {
        return new Response(JSON.stringify({ items: [project], total: 1, available: true }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      if (url.includes("/api/research/assets")) {
        return new Response(JSON.stringify({ items: [asset], total: 1, available: true }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response("{}", { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/projects");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Projects workspace domain boundary", () => {
  it("renders independent project records from /api/projects", async () => {
    render(<App />);
    expect(await screen.findByText("项目目录")).toBeTruthy();
    expect(await screen.findByText("Kernel review")).toBeTruthy();
    expect(
      vi.mocked(fetch).mock.calls.some(([input]) => String(input).includes("/api/projects")),
    ).toBe(true);
  });

  it("does not read RTIS task rows as project entities", async () => {
    render(<App />);
    await screen.findByText("项目目录");
    expect(
      vi.mocked(fetch).mock.calls.some(([input]) => String(input).includes("/api/tasks")),
    ).toBe(false);
  });

  it("creates a durable Project through the independent Project API", async () => {
    render(<App />);
    await screen.findByText("项目目录");
    fireEvent.click(screen.getByRole("button", { name: "新建项目" }));
    fireEvent.change(screen.getByPlaceholderText("例如：Linux 内核攻击面审查"), {
      target: { value: "New independent project" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建项目" }));
    await waitFor(() => {
      expect(vi.mocked(fetch).mock.calls.some(([, init]) => init?.method === "POST")).toBe(true);
    });
    const call = vi.mocked(fetch).mock.calls.find(([, init]) => init?.method === "POST");
    expect(call?.[0]).toBe("/api/projects");
    expect(JSON.parse(String(call?.[1]?.body))).toMatchObject({
      name: "New independent project",
      slug: "new-independent-project",
    });
  });
});

describe("Research Assets workspace domain boundary", () => {
  it("renders independent asset identities from /api/research/assets", async () => {
    window.history.pushState({}, "", "/research/assets");
    render(<App />);
    expect(await screen.findByText("资产清单")).toBeTruthy();
    expect(await screen.findByText("Example target")).toBeTruthy();
    expect(
      vi.mocked(fetch).mock.calls.some(([input]) => String(input).includes("/api/research/assets")),
    ).toBe(true);
  });

  it("manually imports a Research Asset through the independent Asset API", async () => {
    window.history.pushState({}, "", "/research/assets");
    render(<App />);
    await screen.findByText("资产清单");
    fireEvent.click(screen.getByRole("button", { name: "导入资产" }));
    fireEvent.change(
      screen.getByPlaceholderText("example.org / 192.0.2.10 / https://example.org"),
      {
        target: { value: "manual-import.example" },
      },
    );
    fireEvent.click(screen.getAllByRole("button", { name: "导入资产" }).at(-1)!);
    await waitFor(() => {
      expect(vi.mocked(fetch).mock.calls.some(([, init]) => init?.method === "POST")).toBe(true);
    });
    const call = vi.mocked(fetch).mock.calls.find(([, init]) => init?.method === "POST");
    expect(call?.[0]).toBe("/api/research/assets");
    expect(JSON.parse(String(call?.[1]?.body))).toMatchObject({
      asset_type: "domain",
      canonical_value: "manual-import.example",
    });
  });
});
