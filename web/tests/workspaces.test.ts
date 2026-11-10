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

/** P5-4：workspaces 纯数据模型用例（零 import 约定、稳定编号坐标、全键覆盖、
 *  buildFeatureMap 推导）。 */

function tabOf(workspace: Workspace, order: number): WorkspaceNavItem {
  const tab = workspace.tabs.find((item) => item.order === order);
  if (!tab) throw new Error(`workspace ${workspace.id} 缺少 tab order=${order}`);
  return tab;
}

describe("workspaces 数据模型（P5-4）", () => {
  it("工作区序号从 1 起连续，BY_ORDER 防御式排序生效", () => {
    WORKSPACES_BY_ORDER.forEach((workspace, index) => {
      expect(workspace.order).toBe(index + 1);
    });
  });

  it("每个工作区的 tab 序号从 1 起连续（编号坐标无空洞）", () => {
    for (const workspace of WORKSPACES) {
      workspace.tabs.forEach((tab, index) => {
        expect(tab.order).toBe(index + 1);
      });
    }
  });

  it("tabNumberOf 只取显式 order：过滤掉部分 tab 后剩余编号不漂移", () => {
    const ops = WORKSPACES.find((workspace) => workspace.id === "ops");
    if (!ops) throw new Error("缺少执行工作区");
    const before = ops.tabs.map((tab) => tabNumberOf(ops, tab));
    // 模拟将来的角色/能力过滤：去掉第一个 tab，其余编号必须原样保留。
    const filtered = { ...ops, tabs: ops.tabs.filter((tab) => tab.order !== 1) };
    const after = filtered.tabs.map((tab) => tabNumberOf(filtered, tab));
    expect(after).toEqual(before.filter((number) => number !== tabNumberOf(ops, tabOf(ops, 1))));
    expect(after).toContain("2.3");
  });

  it("数据里出现的图标键全部在键全集内（全键覆盖）", () => {
    for (const workspace of WORKSPACES) {
      expect(WORKSPACE_ICON_KEYS).toContain(workspace.icon);
      for (const tab of workspace.tabs) {
        expect(WORKSPACE_ICON_KEYS).toContain(tab.icon);
      }
    }
  });

  it("六页路由与工作区数据一一对应且无重复", () => {
    const routes = ["/", ...WORKSPACES.flatMap((workspace) => workspace.tabs.map((tab) => tab.to))];
    expect(routes).toHaveLength(6);
    expect(new Set(routes).size).toBe(6);
    for (const route of ["/", "/projects", "/knowledge", "/tools", "/runs", "/runtime"]) {
      expect(routes).toContain(route);
    }
  });

  it("buildFeatureMap 从同一份数据推导编号工作区卡与 tab 芯片", () => {
    const map = buildFeatureMap({ knowledge: 46, tools: 20, runs: 3 });
    expect(map.map((workspace) => workspace.number)).toEqual(["1", "2"]);
    const research = map[0];
    const ops = map[1];
    if (!research || !ops) throw new Error("功能地图缺工作区");
    expect(research.badges).toEqual([{ label: "知识条目", value: 46 }]);
    expect(ops.badges).toEqual([
      { label: "工具", value: 20 },
      { label: "运行", value: 3 },
    ]);
    expect(ops.tabs.map((tab) => tab.number)).toEqual(["2.1", "2.2", "2.3"]);
    expect(research.tabs.map((tab) => tab.number)).toEqual(["1.1", "1.2"]);
    // tab 芯片与 tab 条共用同一数据源：路由一致。
    expect(ops.tabs.map((tab) => tab.to)).toEqual(["/tools", "/runs", "/runtime"]);
  });
});
