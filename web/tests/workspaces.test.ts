import { describe, expect, it } from "vitest";
import {
  WORKSPACES,
  WORKSPACES_BY_ORDER,
  WORKSPACE_ICON_KEYS,
  buildFeatureMap,
  tabNumberOf,
  type Workspace,
  type WorkspaceNavItem,
} from "../src/workspaces";

/** P5-4 建立、P7-1 扩为十一工作区：workspaces 纯数据模型用例（零 import 约定、
 *  稳定编号坐标、全键覆盖、buildFeatureMap 推导）。锁的结构不变量与参照实现
 *  tests/unit/admin/workspaces.test.ts 同口径：
 *   1. 编号漂移：tab 编号若取自渲染位置，过滤后「5.1」就换了页面；
 *   2. 路径冲突：workspaceIdOf 按首个匹配归属，路径重复会串工作区。 */

/** SESSION-HANDOFF §11.2 的十一工作区（顺序即 order 1 到 11）。 */
const EXPECTED_IDS = [
  "dashboard",
  "projects",
  "research",
  "knowledge",
  "tools",
  "plugins",
  "agents",
  "workflows",
  "runs",
  "runtime",
  "settings",
] as const;

function tabOf(workspace: Workspace, order: number): WorkspaceNavItem {
  const tab = workspace.tabs.find((item) => item.order === order);
  if (!tab) throw new Error(`workspace ${workspace.id} 缺少 tab order=${order}`);
  return tab;
}

describe("workspaces 数据模型（P7-1 十一工作区）", () => {
  it("十一个工作区按 1 到 11 编号、id 唯一且顺序与 §11.2 清单一致，BY_ORDER 是防御式排序", () => {
    expect(WORKSPACES.map((workspace) => workspace.id)).toEqual([...EXPECTED_IDS]);
    expect(WORKSPACES.map((workspace) => workspace.order)).toEqual([
      1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    ]);
    expect(WORKSPACES_BY_ORDER.map((workspace) => workspace.id)).toEqual(
      WORKSPACES.map((workspace) => workspace.id),
    );
    // 防御式排序不得改写原数组
    expect(WORKSPACES_BY_ORDER).not.toBe(WORKSPACES);
  });

  it("每个工作区的 entry 落在自身 tabs 内（图标轨点击后标签区仍然在场）", () => {
    for (const workspace of WORKSPACES) {
      expect(workspace.tabs.some((tab) => tab.to === workspace.entry)).toBe(true);
    }
  });

  it("tab 路径全局唯一：workspaceIdOf 按首个匹配归属，重复会串工作区", () => {
    const paths = WORKSPACES.flatMap((workspace) => workspace.tabs.map((tab) => tab.to));
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("每个工作区的 tab 序号从 1 起连续（编号坐标无空洞）", () => {
    for (const workspace of WORKSPACES) {
      workspace.tabs.forEach((tab, index) => {
        expect(tab.order).toBe(index + 1);
      });
    }
  });

  it("tabNumberOf 只取显式 order：过滤掉部分 tab 后剩余编号不漂移", () => {
    const beforeByPath = new Map(
      WORKSPACES.map((workspace) => [
        workspace.id,
        new Map(workspace.tabs.map((tab) => [tab.to, tabNumberOf(workspace, tab)])),
      ]),
    );
    // 模拟将来的角色/能力过滤：去掉第一个 tab，其余编号必须原样保留。
    for (const workspace of WORKSPACES) {
      const filtered = { ...workspace, tabs: workspace.tabs.filter((tab) => tab.order !== 1) };
      for (const tab of filtered.tabs) {
        expect(tabNumberOf(filtered, tab)).toBe(beforeByPath.get(workspace.id)!.get(tab.to));
      }
    }
    expect(tabNumberOf(WORKSPACES[6]!, tabOf(WORKSPACES[6]!, 1))).toBe("7.1");
  });

  it("数据里出现的图标键全部在键全集内（全键覆盖）", () => {
    for (const workspace of WORKSPACES) {
      expect(WORKSPACE_ICON_KEYS).toContain(workspace.icon);
      for (const tab of workspace.tabs) {
        expect(WORKSPACE_ICON_KEYS).toContain(tab.icon);
      }
    }
  });

  it("路由与工作区数据一一对应且无重复（十一工作区 + 总览）", () => {
    const routes = ["/", ...WORKSPACES.flatMap((workspace) => workspace.tabs.map((tab) => tab.to))];
    // Dashboard 的 entry 即「/」：列表含一次重复，去重后恰为 11 条路由。
    expect(new Set(routes).size).toBe(11);
    for (const route of [
      "/",
      "/projects",
      "/research",
      "/knowledge",
      "/tools",
      "/plugins",
      "/agents",
      "/workflows",
      "/runs",
      "/runtime",
      "/settings",
    ]) {
      expect(routes).toContain(route);
    }
  });

  it("每个工作区与每个 tab 都有一句话说明", () => {
    for (const workspace of WORKSPACES) {
      expect(workspace.hint.trim()).not.toBe("");
      for (const tab of workspace.tabs) {
        expect(tab.hint.trim()).not.toBe("");
      }
    }
  });

  it("概念分离：数据里只有界面组织字段，不带 Project/Agent/Workflow 领域对象模型", () => {
    for (const workspace of WORKSPACES) {
      // 工作区数据只描述导航（id/order/label/icon/entry/hint/tabs），不承载领域字段。
      const keys = Object.keys(workspace).sort();
      expect(keys).toEqual(["entry", "hint", "icon", "id", "label", "order", "tabs"]);
      for (const tab of workspace.tabs) {
        expect(Object.keys(tab).sort()).toEqual(["hint", "icon", "label", "order", "to"]);
      }
    }
  });

  it("buildFeatureMap 从同一份数据推导编号工作区卡与 tab 芯片", () => {
    const map = buildFeatureMap({ knowledge: 46, tools: 20, runs: 3 });
    expect(map.map((workspace) => workspace.number)).toEqual([
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      "8",
      "9",
      "10",
      "11",
    ]);
    const dashboard = map.find((workspace) => workspace.id === "dashboard");
    const tools = map.find((workspace) => workspace.id === "tools");
    if (!dashboard || !tools) throw new Error("功能地图缺工作区");
    expect(dashboard.badges).toEqual([
      { label: "知识条目", value: 46 },
      { label: "工具", value: 20 },
      { label: "运行", value: 3 },
    ]);
    expect(tools.badges).toEqual([{ label: "工具", value: 20 }]);
    // tab 芯片与标签区固定 tab 共用同一数据源：路由一致。
    expect(tools.tabs.map((tab) => tab.to)).toEqual(["/tools"]);
    expect(tools.tabs[0]!.number).toBe("5.1");
    // 无真实数据来源的工作区宁缺毋滥：徽章为空；knowledge/runs 各拿自己的计数。
    const runs = map.find((workspace) => workspace.id === "runs");
    const knowledge = map.find((workspace) => workspace.id === "knowledge");
    expect(runs?.badges).toEqual([{ label: "运行", value: 3 }]);
    expect(knowledge?.badges).toEqual([{ label: "知识条目", value: 46 }]);
    for (const id of [
      "projects",
      "research",
      "plugins",
      "agents",
      "workflows",
      "runtime",
      "settings",
    ]) {
      expect(map.find((workspace) => workspace.id === id)?.badges).toEqual([]);
    }
  });
});
