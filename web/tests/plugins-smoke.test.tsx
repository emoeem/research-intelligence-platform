import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P6-8 插件中心冒烟：目录分类（已安装/可更新/已禁用/其他）、状态徽章、
 *  可更新标记、数据面未就绪诚实空态、详情页（capability 清单、兼容性列、
 *  依赖图、L3 Manifest JSON、审计 trail）。全部 /api/** 走 fetch stub。 */

const LIST_STUB = {
  available: true,
  db: "migrations/dev-p6.db",
  total: 3,
  items: [
    {
      plugin_id: "knowledge.rtis-redteam",
      version: "0.1.0",
      name: "RTIS 红队知识包",
      kind: "knowledge",
      description: "RTIS kit 语料接入",
      license: "UNLICENSED",
      trust_level: "local",
      state: "ENABLED",
      update_pending: false,
      is_active: true,
      governance: "ACTIVE",
      source_type: "local_dir",
      source_ref: "integrations/rtis",
      capabilities_count: 2,
      updated_at: "2026-11-11T10:00:00Z",
    },
    {
      plugin_id: "tool.rip.python-workers",
      version: "0.1.0",
      name: "RIP Python Workers",
      kind: "tool",
      description: "三个 Python Worker",
      license: "UNLICENSED",
      trust_level: "local",
      state: "REGISTERED",
      update_pending: true,
      is_active: true,
      governance: "ACTIVE",
      source_type: "local_dir",
      source_ref: "workers/python",
      capabilities_count: 3,
      updated_at: "2026-11-11T10:00:00Z",
    },
    {
      plugin_id: "legacy.plugin",
      version: "0.1.0",
      name: "Legacy",
      kind: "knowledge",
      description: "已卸载样例",
      license: "MIT",
      trust_level: "remote",
      state: "UNINSTALLED",
      update_pending: false,
      is_active: false,
      governance: "ACTIVE",
      source_uninstalled: true,
      capabilities_count: 0,
      updated_at: "2026-11-11T09:00:00Z",
    },
  ],
  categories: {
    installed: ["knowledge.rtis-redteam", "tool.rip.python-workers"],
    updatable: ["tool.rip.python-workers"],
    disabled: [],
    uninstalled: ["legacy.plugin"],
  },
  audit_summary: [{ action: "plugin.install", n: 3 }],
};

const DETAIL_STUB = {
  level: 2,
  plugin_id: "knowledge.rtis-redteam",
  version: "0.1.0",
  name: "RTIS 红队知识包",
  kind: "knowledge",
  description: "RTIS kit 语料接入",
  license: "UNLICENSED",
  trust_level: "local",
  state: "ENABLED",
  update_pending: false,
  is_active: true,
  governance: "ACTIVE",
  source_type: "local_dir",
  source_ref: "integrations/rtis",
  lock_sha256: "a".repeat(64),
  installed_dir: "rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0",
  source_uninstalled: false,
  updated_at: "2026-11-11T10:00:00Z",
  context: { summary: "红队 POC 与技能知识源", docs: "README.md" },
  capabilities: [
    {
      capability_id: "knowledge-source.rtis.poc-corpus",
      capability_type: "knowledge_source",
      entry: "README.md",
      summary: "POC 正文与技能文档语料",
      runtime: null,
      requires: [],
    },
  ],
  compatibility: { rip_core: ">=0.2.0, <1.0.0", manifest_schema: ["1"] },
  dependencies: [],
  audit: [
    {
      event_id: 1,
      ts: "2026-11-11T10:00:00Z",
      actor: "user:cli",
      action: "plugin.install",
      entity_id: "knowledge.rtis-redteam@0.1.0",
    },
  ],
};

function stubFetch(handler: (path: string) => unknown): void {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) =>
      Promise.resolve(new Response(JSON.stringify(handler(String(input))), { status: 200 })),
    ),
  );
}

describe("插件中心（P6-8）", () => {
  beforeEach(() => {
    stubFetch((path) => {
      if (path.startsWith("/api/plugins/")) return DETAIL_STUB;
      if (path === "/api/plugins") return LIST_STUB;
      return {};
    });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("目录分类与状态徽章（已安装/可更新/其他含已卸载）", async () => {
    window.history.pushState({}, "", "/plugins");
    render(<App />);
    expect(await screen.findByTestId("plugins-installed")).toBeTruthy();
    expect(screen.getByTestId("plugins-updatable")).toBeTruthy();
    expect(screen.getByTestId("plugins-others")).toBeTruthy();
    expect(screen.getByTestId("plugin-card-knowledge.rtis-redteam")).toBeTruthy();
    expect(screen.getAllByTestId("plugin-state").length).toBeGreaterThanOrEqual(3);
    expect(screen.getAllByTestId("update-pending").length).toBeGreaterThanOrEqual(1);
    // 行点击进详情路由
    expect(
      (screen.getByTestId("plugin-card-knowledge.rtis-redteam") as HTMLAnchorElement).getAttribute(
        "href",
      ),
    ).toBe("/plugins/knowledge.rtis-redteam");
  });

  it("详情页：capability 清单、兼容性列、审计 trail，写按钮按状态出现", async () => {
    window.history.pushState({}, "", "/plugins/knowledge.rtis-redteam");
    render(<App />);
    expect(await screen.findByTestId("capability-list")).toBeTruthy();
    expect(screen.getByTestId("compat-table")).toBeTruthy();
    expect(screen.getByTestId("audit-trail")).toBeTruthy();
    expect(screen.getByTestId("action-disable")).toBeTruthy();
    expect(screen.getByTestId("action-uninstall")).toBeTruthy();
    expect(screen.getByTestId("action-ingest")).toBeTruthy();
    expect(screen.queryByTestId("action-enable")).toBeNull();
    expect(screen.getByTestId("context-summary")).toBeTruthy();
  });

  it("数据面未就绪：诚实空态（available=false 不造假数据）", async () => {
    stubFetch(() => ({
      available: false,
      reason: "plugin_db_not_configured",
      items: [],
      total: 0,
    }));
    window.history.pushState({}, "", "/plugins");
    render(<App />);
    expect(await screen.findByText(/插件数据面/)).toBeTruthy();
    expect(screen.queryByTestId("plugins-installed")).toBeNull();
  });
});
