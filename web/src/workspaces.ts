/**
 * 工作区信息架构的唯一数据源（RIP 版，模型沿自参照实现 admin/src/workspaces.ts，
 * UI 复用盘点 §2.1：零 import 纯数据、图标字符串键、稳定编号、buildFeatureMap）。
 *
 * Layout 的图标轨 / 工作区 tab 条与总览页的功能地图都从这里取数，数据只维护
 * 一份。本模块保持零 import：图标以字符串键存储，由 Layout 侧映射成 lucide
 * 组件；类型全部就地声明。
 *
 * RIP 两层信息架构（Phase 5 六页）：
 *  - 全局图标轨（56px 常驻）：品牌（回总览）+ 每个工作区一个图标；
 *  - 工作区子导航：进入工作区后，主区域顶部出现该工作区的横向 tab 条，
 *    其余工作区不可见。总览（/）不属于任何工作区，无 tab 条。
 */

export type WorkspaceId = "research" | "ops";

/** 图标键 → Layout 侧 ICONS 表映射为 lucide-react 组件（Record 全键覆盖，漏键 tsc 报错）。 */
export type WorkspaceIconKey =
  "radar" | "cpu" | "folderKanban" | "database" | "wrench" | "circlePlay" | "serverCog";

/** 键全集（测试用：数据里出现的图标键必须都在其中）。 */
export const WORKSPACE_ICON_KEYS: readonly WorkspaceIconKey[] = [
  "radar",
  "cpu",
  "folderKanban",
  "database",
  "wrench",
  "circlePlay",
  "serverCog",
];

export interface WorkspaceNavItem {
  /** 工作区内序号（1 起连续）：编号取自数据，过滤后不漂移。 */
  order: number;
  to: string;
  label: string;
  icon: WorkspaceIconKey;
  /** 一句话说明（tab tooltip 与功能地图芯片）。 */
  hint: string;
}

export interface Workspace {
  id: WorkspaceId;
  /** 工作区序号（1 起）：图标轨 tooltip、tab 条编号与功能地图共用。 */
  order: number;
  label: string;
  icon: WorkspaceIconKey;
  /** 图标轨点击后的着陆页（第一个 tab）。 */
  entry: string;
  /** 一句话说明（图标轨 tooltip / 功能地图工作区卡副标题）。 */
  hint: string;
  /** 工作区顶部 tab 条（含着陆页自身）。 */
  tabs: WorkspaceNavItem[];
}

export const WORKSPACES: readonly Workspace[] = [
  {
    id: "research",
    order: 1,
    label: "研究",
    icon: "radar",
    entry: "/projects",
    hint: "任务、知识与证据",
    tabs: [
      {
        order: 1,
        to: "/projects",
        label: "项目",
        icon: "folderKanban",
        hint: "项目、任务树与研究时间线",
      },
      { order: 2, to: "/knowledge", label: "知识库", icon: "database", hint: "知识条目检索与详情" },
    ],
  },
  {
    id: "ops",
    order: 2,
    label: "执行",
    icon: "cpu",
    entry: "/tools",
    hint: "工具、运行与运行时",
    tabs: [
      { order: 1, to: "/tools", label: "工具中心", icon: "wrench", hint: "工具目录、详情与试运行" },
      {
        order: 2,
        to: "/runs",
        label: "运行记录",
        icon: "circlePlay",
        hint: "Run 列表与 Artifact 预览",
      },
      { order: 3, to: "/runtime", label: "运行时", icon: "serverCog", hint: "节点在线状态与心跳" },
    ],
  },
];

/** 按序号排列的工作区（防御式排序：数据文件内乱序也不影响展示顺序）。 */
export const WORKSPACES_BY_ORDER: readonly Workspace[] = [...WORKSPACES].sort(
  (a, b) => a.order - b.order,
);

/**
 * tab 的稳定编号（如 "2.1"）：workspace 序号 + tab 在工作区内的序号。
 * 编号只取显式 order 字段：将来按角色/能力过滤 tab 后编号也不漂移，
 * 口头「去 2.3」始终成立（参照实现的 T2 纪律）。
 */
export function tabNumberOf(workspace: Workspace, tab: WorkspaceNavItem): string {
  return `${workspace.order}.${tab.order}`;
}

/** 功能地图的计数输入（来自 /api/overview 的真实计数，总览页传入）。 */
export interface FeatureMapCounts {
  knowledge: number;
  tools: number;
  runs: number;
}

export interface FeatureMapBadge {
  label: string;
  value: number;
}

export interface FeatureMapTab {
  /** 稳定编号（tabNumberOf），如 "2.1"。 */
  number: string;
  label: string;
  to: string;
  hint: string;
}

export interface FeatureMapWorkspace {
  number: string;
  id: WorkspaceId;
  label: string;
  hint: string;
  entry: string;
  badges: FeatureMapBadge[];
  tabs: FeatureMapTab[];
}

/**
 * 从工作区数据推导功能地图模型：编号工作区卡 → tab 芯片（一句话 hint）+ 计数徽章。
 * 徽章只标在有真实数据来源的工作区上（knowledge 来自 /api/knowledge 面，
 * tools/runs 来自 /api/tools 与 /api/runs 面），宁缺毋滥不猜语义。
 */
export function buildFeatureMap(counts: FeatureMapCounts): FeatureMapWorkspace[] {
  return WORKSPACES_BY_ORDER.map((workspace) => ({
    number: String(workspace.order),
    id: workspace.id,
    label: workspace.label,
    hint: workspace.hint,
    entry: workspace.entry,
    badges:
      workspace.id === "research"
        ? [{ label: "知识条目", value: counts.knowledge }]
        : [
            { label: "工具", value: counts.tools },
            { label: "运行", value: counts.runs },
          ],
    tabs: workspace.tabs.map((tab) => ({
      number: tabNumberOf(workspace, tab),
      label: tab.label,
      to: tab.to,
      hint: tab.hint,
    })),
  }));
}
