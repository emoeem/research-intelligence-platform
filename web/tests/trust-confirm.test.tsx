import { cleanup, render, screen } from "@testing-library/react";
import { fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P10-2 信任级分级确认（PluginsPage 安装面板 + PluginDetailPage 操作闸）：
 *  - official：常规确认（一次点击，无额外字段）；
 *  - local：来源核对勾选后确认；
 *  - remote：输入插件 id 二次确认（不一致时阻断）；
 *  - 策略不可得：fail-closed 最强档（仍要求输入插件 id）。
 *  POST 体断言确认字段随档位走（服务端同一策略表强制）。 */

const POLICY_STUB = {
  schema_version: "1",
  source: "plugin-registry::trust_policy (PLUGIN-CONTRACT §6 + D10)",
  tiers: [
    {
      trust_level: "official",
      install_confirmation: "single",
      confirmation_label: "官方来源：常规确认",
      extra_steps: 0,
      requires_provenance: false,
      requires_typed_id: false,
      required_confirm_fields: [],
      auto_update_prompt: true,
    },
    {
      trust_level: "community",
      install_confirmation: "confirm",
      confirmation_label: "社区来源：确认安装",
      extra_steps: 1,
      requires_provenance: false,
      requires_typed_id: false,
      required_confirm_fields: ["confirm"],
      auto_update_prompt: false,
    },
    {
      trust_level: "local",
      install_confirmation: "confirm_with_provenance",
      confirmation_label: "本地来源：核对来源后确认",
      extra_steps: 1,
      requires_provenance: true,
      requires_typed_id: false,
      required_confirm_fields: ["confirm"],
      auto_update_prompt: false,
    },
    {
      trust_level: "remote",
      install_confirmation: "double_confirm_typed",
      confirmation_label: "远程来源：输入插件 id 二次确认",
      extra_steps: 1,
      requires_provenance: false,
      requires_typed_id: true,
      required_confirm_fields: ["confirm", "confirm_id"],
      auto_update_prompt: false,
    },
  ],
};

const LIST_STUB = {
  available: true,
  total: 0,
  items: [],
  categories: { installed: [], updatable: [], disabled: [], uninstalled: [] },
};

const INSPECT_STUB = {
  ok: true,
  result: {
    id: "knowledge.rtis-redteam",
    version: "0.1.0",
    kind: "knowledge",
    license: "UNLICENSED",
    source: { type: "local_dir", path: "integrations/rtis" },
    source_default_trust: "local",
    install_confirmation: "confirm_with_provenance",
    confirmation_label: "本地来源：核对来源后确认",
    auto_update_prompt: false,
  },
};

const WRITES: { path: string; body: Record<string, unknown> }[] = [];

function stubFetch(handler: (path: string, body: unknown) => unknown): void {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const body = init?.body ? (JSON.parse(String(init.body)) as unknown) : undefined;
      return Promise.resolve(
        new Response(JSON.stringify(handler(String(input), body)), { status: 200 }),
      );
    }),
  );
}

function recordWrites(path: string, body: unknown): void {
  if (WRITES.length < 100) WRITES.push({ path, body: (body ?? {}) as Record<string, unknown> });
}

const DETAIL_STUB = {
  plugin_id: "knowledge.rtis-redteam",
  version: "0.1.0",
  name: "RTIS 红队知识包",
  kind: "knowledge",
  description: "RTIS kit 语料接入",
  license: "UNLICENSED",
  trust_level: "local",
  state: "REGISTERED",
  update_pending: true,
  is_active: true,
  source_type: "local_dir",
  source_ref: "integrations/rtis",
  capabilities: [],
  dependencies: [],
  audit: [],
};

describe("P10-2 信任级分级确认", () => {
  beforeEach(() => {
    WRITES.length = 0;
    stubFetch((path, body) => {
      recordWrites(path, body);
      if (path === "/api/plugins/trust-policy") return POLICY_STUB;
      if (path === "/api/plugins/manifest-inspect") return INSPECT_STUB;
      if (path.startsWith("/api/plugins/")) return DETAIL_STUB;
      if (path === "/api/plugins") return LIST_STUB;
      return {};
    });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("official：常规确认，一次点击提交且不要求额外字段", async () => {
    window.history.pushState({}, "", "/plugins");
    render(<App />);
    const pathInput = await screen.findByTestId("install-manifest-path");
    fireEvent.change(pathInput, {
      target: { value: "schemas/plugin-manifest/examples/knowledge.rtis-redteam.json" },
    });
    fireEvent.change(screen.getByTestId("install-trust-select"), { target: { value: "official" } });
    expect(screen.getByTestId("install-tier-hint").textContent).toContain("D10 允许自动更新提示");
    fireEvent.click(screen.getByTestId("install-inspect"));
    expect(await screen.findByTestId("install-preview")).toBeTruthy();
    fireEvent.click(screen.getByTestId("install-open-confirm"));
    const dialog = await screen.findByTestId("trust-confirm-dialog");
    expect(dialog.textContent).toContain("官方来源：常规确认");
    const proceed = screen.getByTestId("confirm-proceed") as HTMLButtonElement;
    expect(proceed.disabled).toBe(false);
    fireEvent.click(proceed);
    const installWrite = WRITES.find((w) => w.path === "/api/plugins/install");
    expect(installWrite).toBeTruthy();
    expect(installWrite?.body.trust).toBe("official");
    expect(installWrite?.body.confirm).toBeUndefined();
  });

  it("local：来源核对勾选前确认键禁用，勾选后放行", async () => {
    window.history.pushState({}, "", "/plugins");
    render(<App />);
    fireEvent.change(await screen.findByTestId("install-manifest-path"), {
      target: { value: "x.json" },
    });
    fireEvent.change(screen.getByTestId("install-trust-select"), { target: { value: "local" } });
    fireEvent.click(screen.getByTestId("install-inspect"));
    await screen.findByTestId("install-preview");
    fireEvent.click(screen.getByTestId("install-open-confirm"));
    await screen.findByTestId("trust-confirm-dialog");
    const proceed = screen.getByTestId("confirm-proceed") as HTMLButtonElement;
    expect(proceed.disabled).toBe(true);
    const checkbox = screen
      .getByTestId("confirm-provenance")
      .querySelector("input") as HTMLInputElement;
    fireEvent.click(checkbox);
    expect(proceed.disabled).toBe(false);
    fireEvent.click(proceed);
    const installWrite = WRITES.find((w) => w.path === "/api/plugins/install");
    expect(installWrite?.body.confirm).toBe(true);
    expect(installWrite?.body.confirm_id).toBeUndefined();
  });

  it("remote：输入插件 id 二次确认，不一致阻断，一致后带 confirm_id 提交", async () => {
    window.history.pushState({}, "", "/plugins");
    render(<App />);
    fireEvent.change(await screen.findByTestId("install-manifest-path"), {
      target: { value: "x.json" },
    });
    fireEvent.change(screen.getByTestId("install-trust-select"), { target: { value: "remote" } });
    expect(screen.getByTestId("install-tier-hint").textContent).toContain("D10 需手动检查更新");
    fireEvent.click(screen.getByTestId("install-inspect"));
    await screen.findByTestId("install-preview");
    fireEvent.click(screen.getByTestId("install-open-confirm"));
    await screen.findByTestId("trust-confirm-dialog");
    const proceed = screen.getByTestId("confirm-proceed") as HTMLButtonElement;
    expect(proceed.disabled).toBe(true);
    const typed = screen.getByTestId("confirm-typed-id") as HTMLInputElement;
    fireEvent.change(typed, { target: { value: "knowledge.wrong-id" } });
    expect(screen.getByTestId("confirm-id-mismatch")).toBeTruthy();
    expect(proceed.disabled).toBe(true);
    fireEvent.change(typed, { target: { value: "" } });
    fireEvent.change(typed, { target: { value: "knowledge.rtis-redteam" } });
    expect(proceed.disabled).toBe(false);
    fireEvent.click(proceed);
    const installWrite = WRITES.find((w) => w.path === "/api/plugins/install");
    expect(installWrite?.body.confirm).toBe(true);
    expect(installWrite?.body.confirm_id).toBe("knowledge.rtis-redteam");
  });

  it("策略不可得：fail-closed 最强档（仍要求输入插件 id）", async () => {
    stubFetch((path, body) => {
      recordWrites(path, body);
      if (path === "/api/plugins/trust-policy") return { schema_version: "1", tiers: [] };
      if (path === "/api/plugins/manifest-inspect") return INSPECT_STUB;
      if (path.startsWith("/api/plugins/")) return { ok: true, result: { command: "install" } };
      if (path === "/api/plugins") return LIST_STUB;
      return {};
    });
    window.history.pushState({}, "", "/plugins");
    render(<App />);
    fireEvent.change(await screen.findByTestId("install-manifest-path"), {
      target: { value: "x.json" },
    });
    fireEvent.change(screen.getByTestId("install-trust-select"), { target: { value: "official" } });
    fireEvent.click(screen.getByTestId("install-inspect"));
    await screen.findByTestId("install-preview");
    fireEvent.click(screen.getByTestId("install-open-confirm"));
    const dialog = await screen.findByTestId("trust-confirm-dialog");
    expect(dialog.textContent).toContain("未知来源：输入插件 id 二次确认");
  });

  it("详情页升级：确认强度按当前插件信任级（local 需来源核对）", async () => {
    window.history.pushState({}, "", "/plugins/knowledge.rtis-redteam");
    render(<App />);
    const upgrade = await screen.findByTestId("action-upgrade");
    fireEvent.click(upgrade);
    const dialog = await screen.findByTestId("trust-confirm-dialog");
    expect(dialog.textContent).toContain("本地来源：核对来源后确认");
    const proceed = screen.getByTestId("confirm-proceed") as HTMLButtonElement;
    expect(proceed.disabled).toBe(true);
    const checkbox = screen
      .getByTestId("confirm-provenance")
      .querySelector("input") as HTMLInputElement;
    fireEvent.click(checkbox);
    fireEvent.click(proceed);
    const upgradeWrite = WRITES.find((w) =>
      w.path.startsWith("/api/plugins/knowledge.rtis-redteam/upgrade"),
    );
    expect(upgradeWrite?.body.confirm).toBe(true);
    expect(upgradeWrite?.body.manifest_path).toContain("0.2.0");
  });
});
